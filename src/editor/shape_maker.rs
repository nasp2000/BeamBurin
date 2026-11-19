//! Shape Maker: close open chains, union the loops and keep holes linked.
//!
//! The action flattens every selected outline into the document frame, welds
//! open chains whose ends meet within the join tolerance, unions the resulting
//! loops pairwise and creates one closed path per outer contour. Interior
//! contours stay as holes linked through `Object::hole_of`. Source objects are
//! kept; the created shapes become the selection.

use crate::document::{Object, Shape};
use crate::geometry::boolean::{BooleanOp, boolean_contours, point_in_polygon, polygon_area};
use crate::geometry::path::contour_parents_with_rule;
use crate::geometry::{Path, Transform2};

use super::state::EditorState;

/// Distance under which two chain ends are welded together.
pub const SHAPE_MAKER_TOLERANCE_MM: f64 = 0.05;
const MAX_CONTOURS: usize = 10_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShapeMakerReport {
    /// Outer contours created.
    pub shapes: usize,
    /// Interior contours linked as holes.
    pub holes: usize,
    /// Chains that could not be closed and were ignored.
    pub open_chains_ignored: usize,
}

struct Chain {
    points: Vec<(f64, f64)>,
    closed: bool,
}

impl EditorState {
    /// Close and union every selected outline into new closed shapes.
    ///
    /// `tolerance` defaults to [`SHAPE_MAKER_TOLERANCE_MM`]; the action records
    /// a single undo step only when it succeeds.
    pub fn shape_maker(&mut self, tolerance: Option<f64>) -> Result<ShapeMakerReport, String> {
        let tolerance = tolerance.unwrap_or(SHAPE_MAKER_TOLERANCE_MM);
        if !tolerance.is_finite() || !(0.0..=10.0).contains(&tolerance) {
            return Err("the join tolerance must be between 0 and 10 mm".to_string());
        }
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one path".to_string());
        }

        let (chains, template) = collect_chains(&self.document, &ids)?;

        let (loops, open_chains_ignored) = close_chains(chains, tolerance);
        if loops.is_empty() {
            return Err("No closed region found; close the paths first".to_string());
        }

        let contours = union_loops(loops);
        if contours.is_empty() {
            return Err("No closed region found; close the paths first".to_string());
        }
        let paths: Vec<Path> = contours
            .iter()
            .map(|contour| Path::from_points(contour, true))
            .collect();
        let parents = contour_parents_with_rule(&paths, true);
        let holes = parents.iter().filter(|parent| parent.is_some()).count();

        self.record_action("Shape maker");
        let insert_at = self.document.objects.len();
        let mut created: Vec<Object> = contours
            .iter()
            .enumerate()
            .map(|(index, contour)| {
                let mut object = Object::new(
                    template.layer_id.clone(),
                    Shape::Path(Path::from_points(contour, true)),
                )
                .with_style(template.style.clone());
                object.name = format!("Shape maker {}", index + 1);
                object.color.clone_from(&template.color);
                object.transform = Transform2::identity();
                object.operation_id.clone_from(&template.operation_id);
                object.visible = template.visible;
                object
            })
            .collect();
        for (index, parent) in parents.iter().enumerate() {
            if let Some(parent) = parent {
                let parent_id = created[*parent].id.clone();
                created[index].hole_of = Some(parent_id);
            }
        }

        self.selection.clear();
        for (offset, object) in created.into_iter().enumerate() {
            self.selection.add(object.id.clone());
            self.document.objects.insert(insert_at + offset, object);
        }

        Ok(ShapeMakerReport {
            shapes: contours.len() - holes,
            holes,
            open_chains_ignored,
        })
    }
}

/// Validate the selection and flatten it into chains plus a style template.
fn collect_chains(
    document: &crate::document::Document,
    ids: &[String],
) -> Result<(Vec<Chain>, Object), String> {
    let mut chains = Vec::new();
    let mut template: Option<Object> = None;
    for id in ids {
        let object = document
            .object(id)
            .ok_or_else(|| format!("unknown object: {id}"))?;
        if object.locked {
            return Err(format!("'{}' is locked; unlock it first", object.name));
        }
        if document
            .layer(&object.layer_id)
            .is_some_and(|layer| layer.locked)
        {
            return Err(format!("'{}' is locked; unlock it first", object.name));
        }
        if object.array_id.is_some() {
            return Err("Flatten the array first".to_string());
        }
        if template.is_none() {
            template = Some(object.clone());
        }
        match &object.shape {
            Shape::Image(_) => {
                return Err(
                    "Images have no vector outline; trace or convert them first".to_string()
                );
            }
            Shape::Text(text) => {
                for path in crate::objects::text_to_paths(text)? {
                    let path = path.transform(&object.transform);
                    push_chain(&mut chains, &path);
                }
            }
            _ => push_chain(&mut chains, &object.world_path()),
        }
    }
    let template = template.ok_or_else(|| "Select at least one path".to_string())?;
    if chains.len() > MAX_CONTOURS {
        return Err(format!(
            "Shape maker accepts at most {MAX_CONTOURS} contours"
        ));
    }
    Ok((chains, template))
}

fn push_chain(chains: &mut Vec<Chain>, path: &Path) {
    let mut points: Vec<(f64, f64)> = path
        .flatten(SHAPE_MAKER_TOLERANCE_MM)
        .into_iter()
        .map(|point| (point.x, point.y))
        .collect();
    if path.closed
        && points.len() > 1
        && distance(points[0], *points.last().unwrap_or(&points[0])) <= 1e-9
    {
        points.pop();
    }
    let minimum = if path.closed { 3 } else { 2 };
    if points.len() >= minimum {
        chains.push(Chain {
            points,
            closed: path.closed,
        });
    }
}

/// Weld chain ends within `tolerance` and return the closed loops plus the
/// number of chains that could not be closed.
fn close_chains(mut chains: Vec<Chain>, tolerance: f64) -> (Vec<Vec<(f64, f64)>>, usize) {
    let mut loops = Vec::new();
    let mut ignored = 0;
    while let Some(chain) = chains.pop() {
        if chain.closed {
            if is_loop(&chain.points) {
                loops.push(chain.points);
            } else {
                ignored += 1;
            }
            continue;
        }

        let mut points = chain.points;
        let mut closed = false;
        loop {
            let start = points[0];
            let end = *points.last().unwrap_or(&start);
            let gap = distance(start, end);
            if points.len() >= 3 && gap <= tolerance {
                if gap <= 1e-9 {
                    points.pop();
                }
                if points.len() >= 3 {
                    closed = true;
                    break;
                }
            }

            let mut found: Option<(usize, bool, bool)> = None;
            for (index, other) in chains.iter().enumerate() {
                if other.closed || other.points.is_empty() {
                    continue;
                }
                let other_start = other.points[0];
                let other_end = *other.points.last().unwrap_or(&other_start);
                if distance(end, other_start) <= tolerance {
                    found = Some((index, false, false));
                    break;
                }
                if distance(end, other_end) <= tolerance {
                    found = Some((index, true, false));
                    break;
                }
                if distance(start, other_end) <= tolerance {
                    found = Some((index, false, true));
                    break;
                }
                if distance(start, other_start) <= tolerance {
                    found = Some((index, true, true));
                    break;
                }
            }
            let Some((index, reverse, prepend)) = found else {
                break;
            };
            let mut other = chains.remove(index).points;
            if reverse {
                other.reverse();
            }
            if prepend {
                append(&mut other, points);
                points = other;
            } else {
                append(&mut points, other);
            }
        }

        if closed && is_loop(&points) {
            loops.push(points);
        } else {
            ignored += 1;
        }
    }
    (loops, ignored)
}

fn is_loop(points: &[(f64, f64)]) -> bool {
    points.len() >= 3 && polygon_area(points).abs() > 1e-9
}

/// Append `source` to `target`, dropping a duplicated joint point.
fn append(target: &mut Vec<(f64, f64)>, source: Vec<(f64, f64)>) {
    let skip = target
        .last()
        .zip(source.first())
        .is_some_and(|(last, first)| distance(*last, *first) <= 1e-9);
    if skip {
        target.extend(source.into_iter().skip(1));
    } else {
        target.extend(source);
    }
}

/// Union every pair of loops that overlaps, sharing an edge or encloses a new
/// region. Nested loops are left alone so they become holes later.
fn union_loops(loops: Vec<Vec<(f64, f64)>>) -> Vec<Vec<(f64, f64)>> {
    let mut parts = loops;
    let mut guard = 0;
    loop {
        guard += 1;
        if guard > 10_000 {
            break;
        }
        let mut merged = false;
        'search: for first in 0..parts.len() {
            for second in (first + 1)..parts.len() {
                if nested(&parts[first], &parts[second]) {
                    continue;
                }
                let union = boolean_contours(BooleanOp::Union, &parts[first], &parts[second]);
                if union.is_empty() {
                    continue;
                }
                let area_first = polygon_area(&parts[first]).abs();
                let area_second = polygon_area(&parts[second]).abs();
                let union_area: f64 = union
                    .iter()
                    .map(|contour| polygon_area(contour))
                    .sum::<f64>()
                    .abs();
                let single_contour = union.len() < 2;
                let larger = union
                    .iter()
                    .map(|contour| polygon_area(contour).abs())
                    .fold(0.0, f64::max)
                    > area_first.max(area_second) + 1e-6;
                if single_contour || larger || union_area < area_first + area_second - 1e-6 {
                    parts.splice(first..=second, union);
                    merged = true;
                    break 'search;
                }
            }
        }
        if !merged {
            break;
        }
    }
    for contour in &mut parts {
        if polygon_area(contour) < 0.0 {
            contour.reverse();
        }
    }
    parts.sort_by(|left, right| {
        polygon_area(right)
            .abs()
            .total_cmp(&polygon_area(left).abs())
    });
    parts
}

fn nested(left: &[(f64, f64)], right: &[(f64, f64)]) -> bool {
    contained(left, right) || contained(right, left)
}

fn contained(inner: &[(f64, f64)], outer: &[(f64, f64)]) -> bool {
    inner.len() >= 3
        && outer.len() >= 3
        && inner
            .iter()
            .all(|point| point_in_polygon(point.0, point.1, outer))
}

fn distance(left: (f64, f64), right: (f64, f64)) -> f64 {
    (left.0 - right.0).hypot(left.1 - right.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::geometry::Point2;

    fn new_editor() -> EditorState {
        EditorState::new(Document::new("Shape maker", 300.0, 200.0))
    }

    fn select(editor: &mut EditorState, ids: &[&str]) {
        editor.selection.clear();
        for id in ids {
            editor.selection.add((*id).to_string());
        }
    }

    fn closed_count(editor: &EditorState) -> usize {
        editor
            .document
            .objects
            .iter()
            .filter(|object| matches!(&object.shape, Shape::Path(path) if path.closed))
            .count()
    }

    #[test]
    fn three_open_lines_form_a_triangle() {
        let mut editor = new_editor();
        let first = editor
            .add_line(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))
            .unwrap();
        let second = editor
            .add_line(Point2::new(10.0, 0.0), Point2::new(10.0, 10.0))
            .unwrap();
        let third = editor
            .add_line(Point2::new(10.0, 10.0), Point2::new(0.0, 0.0))
            .unwrap();
        select(&mut editor, &[&first, &second, &third]);

        let report = editor.shape_maker(None).unwrap();

        assert_eq!(report.shapes, 1);
        assert_eq!(report.holes, 0);
        assert_eq!(report.open_chains_ignored, 0);
        assert_eq!(editor.document.objects.len(), 4);
        let created = editor.document.objects.last().unwrap();
        assert!(matches!(&created.shape, Shape::Path(path) if path.closed));
    }

    #[test]
    fn adjacent_rectangles_are_unioned() {
        let mut editor = new_editor();
        let first = editor
            .add_rectangle(Point2::new(0.0, 0.0), Point2::new(12.0, 10.0))
            .unwrap();
        let second = editor
            .add_rectangle(Point2::new(10.0, 4.0), Point2::new(20.0, 14.0))
            .unwrap();
        select(&mut editor, &[&first, &second]);

        let report = editor.shape_maker(None).unwrap();

        assert_eq!(report.shapes, 1);
        assert_eq!(closed_count(&editor), 1);
        assert_eq!(editor.document.objects.len(), 3);
    }

    #[test]
    fn overlapping_circles_are_unioned() {
        let mut editor = new_editor();
        let first = editor
            .add_ellipse(Point2::new(0.0, 0.0), Point2::new(20.0, 20.0))
            .unwrap();
        let second = editor
            .add_ellipse(Point2::new(10.0, 0.0), Point2::new(30.0, 20.0))
            .unwrap();
        select(&mut editor, &[&first, &second]);

        let report = editor.shape_maker(None).unwrap();

        assert_eq!(report.shapes, 1);
        assert_eq!(editor.document.objects.len(), 3);
    }

    #[test]
    fn an_open_chain_is_ignored_with_a_warning() {
        let mut editor = new_editor();
        let closed = editor
            .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
            .unwrap();
        let open = editor
            .add_line(Point2::new(40.0, 40.0), Point2::new(50.0, 50.0))
            .unwrap();
        select(&mut editor, &[&closed, &open]);

        let report = editor.shape_maker(None).unwrap();

        assert_eq!(report.shapes, 1);
        assert_eq!(report.open_chains_ignored, 1);
    }

    #[test]
    fn endpoints_within_tolerance_are_welded() {
        let mut editor = new_editor();
        let first = editor
            .add_line(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))
            .unwrap();
        let second = editor
            .add_line(Point2::new(10.0, 0.0), Point2::new(0.0, 10.0))
            .unwrap();
        let third = editor
            .add_line(Point2::new(0.0, 10.0), Point2::new(0.03, 0.0))
            .unwrap();
        select(&mut editor, &[&first, &second, &third]);

        assert_eq!(editor.shape_maker(None).unwrap().shapes, 1);

        let mut editor = new_editor();
        let first = editor
            .add_line(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0))
            .unwrap();
        let second = editor
            .add_line(Point2::new(10.0, 0.0), Point2::new(0.0, 10.0))
            .unwrap();
        let third = editor
            .add_line(Point2::new(0.0, 10.0), Point2::new(0.2, 0.0))
            .unwrap();
        select(&mut editor, &[&first, &second, &third]);

        assert!(editor.shape_maker(None).is_err());
    }

    #[test]
    fn a_ring_links_its_hole() {
        let mut editor = new_editor();
        let outer = editor
            .add_rectangle(Point2::new(0.0, 0.0), Point2::new(30.0, 30.0))
            .unwrap();
        let inner = editor
            .add_rectangle(Point2::new(10.0, 10.0), Point2::new(20.0, 20.0))
            .unwrap();
        select(&mut editor, &[&outer, &inner]);

        let report = editor.shape_maker(None).unwrap();

        assert_eq!(report.shapes, 1);
        assert_eq!(report.holes, 1);
        let created: Vec<&Object> = editor
            .document
            .objects
            .last_chunk::<2>()
            .unwrap()
            .iter()
            .collect();
        let holes: Vec<&&Object> = created
            .iter()
            .filter(|object| object.hole_of.is_some())
            .collect();
        assert_eq!(holes.len(), 1);
        let parent_id = holes[0].hole_of.as_deref().unwrap();
        assert!(created.iter().any(|object| object.id == parent_id));
    }

    #[test]
    fn images_are_refused() {
        let mut editor = new_editor();
        let image = Object::new(
            editor.document.layers[0].id.clone(),
            Shape::Image(crate::objects::ImageObject::new(
                "missing.png",
                10,
                10,
                96.0,
            )),
        );
        let id = image.id.clone();
        editor.document.add_object(image);
        select(&mut editor, &[&id]);

        let error = editor.shape_maker(None).unwrap_err();

        assert!(error.contains("Images have no vector outline"));
        assert!(editor.undo().is_none());
    }

    #[test]
    fn no_closed_region_is_an_error_without_undo() {
        let mut editor = new_editor();
        let layer = editor.document.layers[0].id.clone();
        let line = Object::new(
            layer,
            Shape::Path(Path::from_points(&[(0.0, 0.0), (10.0, 0.0)], false)),
        );
        let id = line.id.clone();
        editor.document.add_object(line);
        select(&mut editor, &[&id]);

        assert!(editor.shape_maker(None).is_err());
        assert!(editor.undo().is_none());
    }

    #[test]
    fn sources_are_kept_and_results_are_selected() {
        let mut editor = new_editor();
        let first = editor
            .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
            .unwrap();
        let second = editor
            .add_rectangle(Point2::new(5.0, 5.0), Point2::new(15.0, 15.0))
            .unwrap();
        select(&mut editor, &[&first, &second]);

        editor.shape_maker(None).unwrap();

        assert!(editor.document.object(&first).is_some());
        assert!(editor.document.object(&second).is_some());
        assert_eq!(editor.selection.len(), 1);
        assert_ne!(editor.selection.ids()[0], first);
        assert_ne!(editor.selection.ids()[0], second);
    }

    #[test]
    fn style_layer_and_operation_are_inherited() {
        let mut editor = new_editor();
        let first = editor
            .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
            .unwrap();
        let second = editor
            .add_rectangle(Point2::new(20.0, 0.0), Point2::new(30.0, 10.0))
            .unwrap();
        {
            let template = editor.document.object_mut(&first).unwrap();
            template.style.stroke = Some("#ff0000".to_string());
            template.style.stroke_width_mm = 0.4;
            template.operation_id = Some("engrave".to_string());
        }
        select(&mut editor, &[&first, &second]);

        editor.shape_maker(None).unwrap();

        let created = editor.document.objects.last().unwrap();
        assert_eq!(created.style.stroke.as_deref(), Some("#ff0000"));
        assert!((created.style.stroke_width_mm - 0.4).abs() < 1e-9);
        assert_eq!(created.operation_id.as_deref(), Some("engrave"));
        assert_eq!(created.layer_id, editor.document.objects[0].layer_id);
        assert!(matches!(&created.shape, Shape::Path(_)));
        assert_eq!(created.transform, Transform2::identity());
    }

    #[test]
    fn shape_maker_is_one_undo_step() {
        let mut editor = new_editor();
        let first = editor
            .add_rectangle(Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
            .unwrap();
        let second = editor
            .add_rectangle(Point2::new(5.0, 5.0), Point2::new(15.0, 15.0))
            .unwrap();
        select(&mut editor, &[&first, &second]);
        let before = editor.document.objects.len();

        editor.shape_maker(None).unwrap();
        assert_eq!(editor.document.objects.len(), before + 1);

        assert_eq!(editor.undo().as_deref(), Some("Shape maker"));
        assert_eq!(editor.document.objects.len(), before);
    }
}
