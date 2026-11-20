//! Cut shapes, split a path and break compounds apart.

use std::collections::HashMap;

use uuid::Uuid;

use crate::document::{Object, Shape};
use crate::geometry::boolean::{
    BooleanOp, boolean_contours, point_in_polygon, polygon_area, split_polyline,
};
use crate::geometry::path::contour_parents_with_rule;
use crate::geometry::{Path, Point2, Transform2};

use super::state::EditorState;

const CLIP_TOLERANCE_MM: f64 = 0.05;
const MAX_PIECES: usize = 2_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CutReport {
    pub pieces: usize,
    pub removed: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BreakReport {
    pub released: usize,
    pub outlines: usize,
}

impl EditorState {
    fn polygon(object: &Object) -> Option<Vec<(f64, f64)>> {
        let path = object.world_path();
        if !path.closed {
            return None;
        }
        let mut points: Vec<(f64, f64)> = path
            .flatten(CLIP_TOLERANCE_MM)
            .into_iter()
            .map(|point| (point.x, point.y))
            .collect();
        if points.len() > 1 {
            let first = points[0];
            let last = points[points.len() - 1];
            if (first.0 - last.0).abs() <= 1e-9 && (first.1 - last.1).abs() <= 1e-9 {
                points.pop();
            }
        }
        (points.len() >= 3).then_some(points)
    }

    fn constraints_reference(&self, ids: &[String]) -> usize {
        self.document
            .constraints
            .iter()
            .filter(|constraint| {
                serde_json::to_string(&constraint.kind)
                    .is_ok_and(|text| ids.iter().any(|id| text.contains(id)))
            })
            .count()
    }

    /// Cut every selected target with one closed cutter, keeping the inside.
    #[allow(clippy::too_many_lines)]
    pub fn cut_shapes(&mut self, cutter_id: Option<&str>) -> Result<CutReport, String> {
        let selected: Vec<String> = self.selection.ids().to_vec();
        let cutter_id = cutter_id
            .map(str::to_owned)
            .or_else(|| selected.last().cloned())
            .ok_or_else(|| "Select at least two shapes".to_string())?;
        let targets: Vec<String> = selected.into_iter().filter(|id| id != &cutter_id).collect();
        if targets.is_empty() {
            return Err("Select at least two shapes".to_string());
        }
        let cutter_index = self
            .document
            .objects
            .iter()
            .position(|object| object.id == cutter_id)
            .ok_or_else(|| format!("unknown cutter: {cutter_id}"))?;
        self.ensure_editable(cutter_index)?;
        let cutter = self.document.objects[cutter_index].clone();
        if matches!(&cutter.shape, Shape::Text(_) | Shape::Image(_)) {
            return Err("The cutter must be a single closed path".to_string());
        }
        if cutter.group_id.is_some() {
            return Err("The cutter must be a single shape; ungroup it first".to_string());
        }
        if self
            .document
            .objects
            .iter()
            .any(|object| object.hole_of.as_deref() == Some(cutter_id.as_str()))
        {
            return Err("Break the cutter apart first".to_string());
        }
        let cutter_polygon = Self::polygon(&cutter)
            .ok_or_else(|| "The cutter must be a single closed path".to_string())?;

        for id in &targets {
            let index = self
                .document
                .objects
                .iter()
                .position(|object| &object.id == id)
                .ok_or_else(|| format!("unknown object: {id}"))?;
            self.ensure_editable(index)?;
            if self.document.objects[index].array_id.is_some() {
                return Err("Flatten the array first".to_string());
            }
        }
        let constraints = self.constraints_reference(&targets);
        if constraints > 0 {
            return Err(format!(
                "{constraints} constraints reference the selection; remove them first"
            ));
        }

        let mut plans: HashMap<String, Vec<Object>> = HashMap::new();
        let mut detached: Vec<String> = Vec::new();
        let mut removed = 0;
        let mut warnings = Vec::new();
        for id in &targets {
            let target = self
                .document
                .object(id)
                .cloned()
                .ok_or_else(|| format!("unknown object: {id}"))?;
            for child in &self.document.objects {
                if child.hole_of.as_deref() == Some(id.as_str()) {
                    detached.push(child.id.clone());
                    warnings.push(format!("hole '{}' was detached and left uncut", child.name));
                }
            }
            let path = target.world_path();
            let points: Vec<(f64, f64)> = path
                .flatten(CLIP_TOLERANCE_MM)
                .into_iter()
                .map(|point| (point.x, point.y))
                .collect();
            let mut pieces: Vec<Vec<(f64, f64)>> = Vec::new();
            let mut closed = false;
            if path.closed {
                closed = true;
                let mut contours =
                    boolean_contours(BooleanOp::Intersection, &points, &cutter_polygon);
                for contour in &mut contours {
                    if polygon_area(contour) < 0.0 {
                        contour.reverse();
                    }
                }
                if contours.len() == 1
                    && (polygon_area(&contours[0]) - polygon_area(&points)).abs() <= 1e-6
                {
                    plans.insert(id.clone(), vec![target.clone()]);
                    continue;
                }
                pieces = contours;
            } else {
                for piece in split_polyline(&points, false, &cutter_polygon) {
                    if piece.len() < 2 {
                        continue;
                    }
                    let middle = (piece.len() - 1) / 2;
                    let mid = (
                        f64::midpoint(piece[middle].0, piece[middle + 1].0),
                        f64::midpoint(piece[middle].1, piece[middle + 1].1),
                    );
                    if point_in_polygon(mid.0, mid.1, &cutter_polygon) {
                        pieces.push(piece);
                    }
                }
            }
            if pieces.is_empty() {
                removed += 1;
                plans.insert(id.clone(), Vec::new());
                continue;
            }
            if closed {
                let paths: Vec<Path> = pieces
                    .iter()
                    .map(|contour| Path::from_points(contour, true))
                    .collect();
                let parents = contour_parents_with_rule(&paths, true);
                let mut objects = Vec::new();
                for (index, piece) in pieces.iter().enumerate() {
                    objects.push(piece_object(&target, piece, true, index, pieces.len()));
                }
                for (index, parent) in parents.iter().enumerate() {
                    if let Some(parent) = parent {
                        let parent_id = objects[*parent].id.clone();
                        objects[index].hole_of = Some(parent_id);
                    }
                }
                plans.insert(id.clone(), objects);
            } else {
                let mut objects = Vec::new();
                for (index, piece) in pieces.iter().enumerate() {
                    objects.push(piece_object(&target, piece, false, index, pieces.len()));
                }
                plans.insert(id.clone(), objects);
            }
        }

        let piece_count: usize = plans.values().map(Vec::len).sum();
        if piece_count == 0 {
            return Err("Nothing was inside the cutter".to_string());
        }
        if piece_count > MAX_PIECES {
            return Err(format!(
                "The cut would create {piece_count} pieces; the limit is {MAX_PIECES}"
            ));
        }

        self.record_action("Cut shapes");
        for child in &mut self.document.objects {
            if detached.contains(&child.id) {
                child.hole_of = None;
            }
        }
        self.selection.clear();
        let mut rebuilt = Vec::with_capacity(self.document.objects.len());
        for object in &self.document.objects {
            if object.id == cutter_id {
                continue;
            }
            if let Some(pieces) = plans.get(&object.id) {
                for piece in pieces {
                    self.selection.add(piece.id.clone());
                    rebuilt.push(piece.clone());
                }
            } else {
                rebuilt.push(object.clone());
            }
        }
        self.document.objects = rebuilt;
        Ok(CutReport {
            pieces: piece_count,
            removed,
            warnings,
        })
    }

    /// Split a path at the segment point closest to `point`.
    pub fn split_path_at(&mut self, object_id: &str, point: Point2) -> Result<usize, String> {
        let index = self
            .document
            .objects
            .iter()
            .position(|object| object.id == object_id)
            .ok_or_else(|| format!("unknown object: {object_id}"))?;
        self.ensure_editable(index)?;
        let object = self.document.objects[index].clone();
        if matches!(object.shape, Shape::Text(_) | Shape::Image(_)) {
            return Err("Text and images cannot be cut".to_string());
        }
        if object.array_id.is_some() {
            return Err("Flatten the array first".to_string());
        }
        let path = object.world_path();
        let (segment, t) = nearest_segment(&path, point)
            .ok_or_else(|| "Click closer to a path to cut it".to_string())?;
        let (first, second) = path
            .split_at(segment, t)
            .ok_or_else(|| "This path cannot be cut at that point".to_string())?;
        self.record_action("Trim shapes");
        self.selection.clear();
        let mut pieces = Vec::new();
        pieces.push(piece_from_path(&object, first, "part 1"));
        if let Some(second) = second {
            pieces.push(piece_from_path(&object, second, "part 2"));
        }
        let count = pieces.len();
        for (offset, piece) in pieces.into_iter().enumerate() {
            self.selection.add(piece.id.clone());
            self.document.objects.insert(index + offset, piece);
        }
        self.document.objects.remove(index + count);
        Ok(count)
    }

    /// Release holes and convert text to outlines in one undo step.
    pub fn break_apart(&mut self) -> Result<BreakReport, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Nothing to break apart".to_string());
        }
        let mut release: Vec<String> = Vec::new();
        let mut outlines = 0;
        let mut plans: HashMap<String, Vec<Object>> = HashMap::new();
        for id in &ids {
            let Some(object) = self.document.object(id).cloned() else {
                continue;
            };
            if let Shape::Text(text) = &object.shape
                && let Ok(paths) = crate::objects::text_to_paths(text)
            {
                let mut converted = Vec::new();
                for (index, path) in paths.into_iter().enumerate() {
                    let mut piece = object.clone();
                    piece.id = Uuid::new_v4().to_string();
                    piece.name = format!("{} outline {}", object.name, index + 1);
                    piece.shape = Shape::Path(path.transform(&object.transform));
                    piece.transform = Transform2::identity();
                    piece.group_id.clone_from(&object.group_id);
                    piece.hole_of = None;
                    converted.push(piece);
                }
                outlines += converted.len();
                plans.insert(id.clone(), converted);
            }
            if object.hole_of.is_some() {
                release.push(id.clone());
            }
            for child in &self.document.objects {
                if child.hole_of.as_deref() == Some(id.as_str()) {
                    release.push(child.id.clone());
                }
            }
        }
        release.sort();
        release.dedup();
        if release.is_empty() && outlines == 0 {
            return Err("Nothing to break apart".to_string());
        }
        self.record_action("Break apart");
        for id in &release {
            if let Some(object) = self.document.object_mut(id) {
                object.hole_of = None;
            }
        }
        self.selection.clear();
        let mut rebuilt = Vec::new();
        for object in &self.document.objects {
            if let Some(converted) = plans.get(&object.id) {
                for piece in converted {
                    self.selection.add(piece.id.clone());
                    rebuilt.push(piece.clone());
                }
            } else {
                rebuilt.push(object.clone());
            }
        }
        self.document.objects = rebuilt;
        Ok(BreakReport {
            released: release.len(),
            outlines,
        })
    }
}

fn piece_object(
    template: &Object,
    points: &[(f64, f64)],
    closed: bool,
    index: usize,
    total: usize,
) -> Object {
    let mut piece = template.clone();
    piece.id = Uuid::new_v4().to_string();
    piece.name = if total == 1 {
        format!("{} cut", template.name)
    } else {
        format!("{} cut {}", template.name, index + 1)
    };
    piece.shape = Shape::Path(Path::from_points(points, closed));
    piece.transform = Transform2::identity();
    piece.hole_of = None;
    piece
}

fn piece_from_path(template: &Object, path: Path, suffix: &str) -> Object {
    let mut piece = template.clone();
    piece.id = Uuid::new_v4().to_string();
    piece.name = format!("{} {suffix}", template.name);
    piece.shape = Shape::Path(path);
    piece.transform = Transform2::identity();
    piece.hole_of = None;
    piece
}

fn nearest_segment(path: &Path, point: Point2) -> Option<(usize, f64)> {
    let mut best: Option<(usize, f64, f64)> = None;
    for (index, segment) in path.segments.iter().enumerate() {
        for step in 0..=64 {
            let t = f64::from(step) / 64.0;
            let at = match segment {
                crate::geometry::path::Segment::Line { from, to } => {
                    Point2::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
                }
                crate::geometry::path::Segment::Cubic(cubic) => cubic.eval(t),
            };
            let distance = (at.x - point.x).hypot(at.y - point.y);
            if best.is_none_or(|(_, _, current)| distance < current) {
                best = Some((index, t, distance));
            }
        }
    }
    best.map(|(index, t, _)| (index, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::geometry::BooleanOp;

    fn editor() -> EditorState {
        EditorState::new(Document::new("Cut", 300.0, 200.0))
    }

    fn rect(editor: &mut EditorState, x0: f64, y0: f64, x1: f64, y1: f64) -> String {
        editor
            .add_rectangle(Point2::new(x0, y0), Point2::new(x1, y1))
            .unwrap()
    }

    fn select(editor: &mut EditorState, ids: &[String]) {
        editor.selection.clear();
        for id in ids {
            editor.selection.add(id.clone());
        }
    }

    #[test]
    fn cut_keeps_the_intersection_and_removes_the_cutter() {
        let mut editor = editor();
        let target = rect(&mut editor, 0.0, 0.0, 40.0, 20.0);
        let cutter = rect(&mut editor, 15.0, -5.0, 35.0, 25.0);
        select(&mut editor, &[target, cutter.clone()]);
        let report = editor.cut_shapes(None).unwrap();
        assert_eq!(report.pieces, 1);
        assert_eq!(report.removed, 0);
        assert!(editor.document.object(&cutter).is_none());
        let piece = editor
            .document
            .objects
            .iter()
            .find(|o| o.array_id.is_none());
        assert!(piece.is_some());
        assert_eq!(editor.document.objects.len(), 1);
    }

    #[test]
    fn cut_outside_targets_are_removed_and_counted() {
        let mut editor = editor();
        let inside = rect(&mut editor, 10.0, 10.0, 20.0, 20.0);
        let outside = rect(&mut editor, 100.0, 100.0, 110.0, 110.0);
        let cutter = rect(&mut editor, 0.0, 0.0, 50.0, 50.0);
        select(&mut editor, &[inside.clone(), outside, cutter]);
        let report = editor.cut_shapes(None).unwrap();
        assert_eq!(report.removed, 1);
        assert_eq!(report.pieces, 1);
    }

    #[test]
    fn cut_fully_inside_keeps_the_target_identity() {
        let mut editor = editor();
        let target = rect(&mut editor, 10.0, 10.0, 20.0, 20.0);
        let cutter = rect(&mut editor, 0.0, 0.0, 50.0, 50.0);
        select(&mut editor, &[target.clone(), cutter]);
        editor.cut_shapes(None).unwrap();
        assert!(editor.document.object(&target).is_some());
    }

    #[test]
    fn cut_is_one_undo_step() {
        let mut editor = editor();
        let target = rect(&mut editor, 0.0, 0.0, 40.0, 20.0);
        let cutter = rect(&mut editor, 10.0, -5.0, 30.0, 25.0);
        select(&mut editor, &[target, cutter]);
        editor.cut_shapes(None).unwrap();
        assert_eq!(editor.document.objects.len(), 1);
        assert!(editor.undo().is_some());
        assert_eq!(editor.document.objects.len(), 2);
    }

    #[test]
    fn boolean_difference_links_the_inner_contour_as_a_hole() {
        let mut editor = editor();
        let outer = rect(&mut editor, 0.0, 0.0, 40.0, 40.0);
        let inner = rect(&mut editor, 10.0, 10.0, 30.0, 30.0);
        select(&mut editor, &[outer, inner]);
        editor.boolean_selection(BooleanOp::Difference).unwrap();
        assert_eq!(editor.document.objects.len(), 2);
        assert!(editor.document.objects.iter().any(|o| o.hole_of.is_some()));
    }

    #[test]
    fn break_apart_releases_holes() {
        let mut editor = editor();
        let outer = rect(&mut editor, 0.0, 0.0, 40.0, 40.0);
        let inner = rect(&mut editor, 10.0, 10.0, 30.0, 30.0);
        editor.document.object_mut(&inner).unwrap().hole_of = Some(outer.clone());
        select(&mut editor, &[outer]);
        let report = editor.break_apart().unwrap();
        assert_eq!(report.released, 1);
        assert!(editor.document.object(&inner).unwrap().hole_of.is_none());
    }

    #[test]
    fn break_apart_keeps_a_moved_text_in_place() {
        let mut editor = editor();
        let id = editor.add_text(Point2::new(10.0, 20.0)).unwrap();
        editor.selection.clear();
        editor.selection.add(id.clone());
        editor.move_selection(30.0, 40.0);
        let before = editor.document.object(&id).unwrap().bounds().unwrap();
        let report = editor.break_apart().unwrap();
        assert!(report.outlines > 0);
        let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
        for object in &editor.document.objects {
            for point in object.world_path().flatten(0.05) {
                min.x = min.x.min(point.x);
                min.y = min.y.min(point.y);
            }
        }
        assert!(
            (min.x - before.0.x).abs() < 1.5,
            "outlines jumped in x: {min:?} vs {before:?}"
        );
        assert!(
            (min.y - before.0.y).abs() < 1.5,
            "outlines jumped in y: {min:?} vs {before:?}"
        );
    }

    #[test]
    fn split_path_opens_a_closed_rectangle() {
        let mut editor = editor();
        let rect = rect(&mut editor, 0.0, 0.0, 20.0, 20.0);
        let pieces = editor.split_path_at(&rect, Point2::new(10.0, 0.0)).unwrap();
        assert_eq!(pieces, 1);
        let object = &editor.document.objects[0];
        assert!(matches!(&object.shape, Shape::Path(path) if !path.closed));
    }
}
