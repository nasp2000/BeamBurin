//! Editing the start point and direction of a path.

use crate::document::Shape;
use crate::geometry::path::Segment;
use crate::geometry::{Path, Point2, Transform2};

use super::state::EditorState;

impl EditorState {
    fn start_constraints(&self, ids: &[String]) -> usize {
        self.document
            .constraints
            .iter()
            .filter(|constraint| {
                serde_json::to_string(&constraint.kind)
                    .is_ok_and(|text| ids.iter().any(|id| text.contains(id)))
            })
            .count()
    }

    fn editable_path(&self, id: &str) -> Result<Path, String> {
        let object = self
            .document
            .object(id)
            .ok_or_else(|| format!("unknown object: {id}"))?;
        if object.locked {
            return Err(format!("'{}' is locked; unlock it first", object.name));
        }
        if self
            .document
            .layer(&object.layer_id)
            .is_some_and(|layer| layer.locked)
        {
            return Err(format!("'{}' is locked; unlock it first", object.name));
        }
        if object.array_id.is_some() {
            return Err("Flatten the array first".to_string());
        }
        match &object.shape {
            Shape::Text(_) => Err("Convert the text to outlines first".to_string()),
            Shape::Image(_) => Err("Images have no start point".to_string()),
            Shape::Path(path) => Ok(path.clone()),
            _ => Ok(object.world_path()),
        }
    }

    fn store_path(&mut self, id: &str, path: Path) {
        if let Some(object) = self.document.object_mut(id) {
            object.shape = Shape::Path(path);
            object.transform = Transform2::identity();
        }
    }

    /// Set the start of one path at the point closest to `point`.
    pub fn set_start_point(&mut self, id: &str, point: Point2) -> Result<Point2, String> {
        let constraints = self.start_constraints(&[id.to_string()]);
        if constraints > 0 {
            return Err(format!(
                "{constraints} constraints reference the selection; remove them first"
            ));
        }
        let mut path = self.editable_path(id)?;
        let result = if path.closed {
            let Some((segment, t, _)) = nearest_segment(&path, point) else {
                return Err("Click closer to a path".to_string());
            };
            path.rotate_start(segment, t)
        } else {
            let start = path
                .start_point()
                .ok_or_else(|| "The path has no start point".to_string())?;
            let end = path
                .last_point()
                .ok_or_else(|| "The path has no end point".to_string())?;
            let to_start = (start.x - point.x).hypot(start.y - point.y);
            let to_end = (end.x - point.x).hypot(end.y - point.y);
            if to_end + 1e-9 < to_start {
                path.reverse();
                true
            } else {
                false
            }
        };
        if !result {
            return Err("The start point is already there".to_string());
        }
        let start = path
            .start_point()
            .ok_or_else(|| "The path has no start point".to_string())?;
        self.record_action("Set start point");
        self.store_path(id, path);
        self.selection.clear();
        self.selection.add(id.to_string());
        Ok(start)
    }

    /// Reverse every path in the selection, converting primitives first.
    pub fn reverse_paths(&mut self) -> Result<usize, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one path".to_string());
        }
        let constraints = self.start_constraints(&ids);
        if constraints > 0 {
            return Err(format!(
                "{constraints} constraints reference the selection; remove them first"
            ));
        }
        let mut paths = Vec::new();
        for id in &ids {
            paths.push((id.clone(), self.editable_path(id)?));
        }
        self.record_action("Reverse path");
        for (id, mut path) in paths {
            if path.closed {
                path.close_explicitly();
            }
            path.reverse();
            self.store_path(&id, path);
        }
        Ok(ids.len())
    }

    /// Reset the selected paths to the canonical start.
    pub fn reset_start_points(&mut self) -> Result<usize, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one path".to_string());
        }
        let constraints = self.start_constraints(&ids);
        if constraints > 0 {
            return Err(format!(
                "{constraints} constraints reference the selection; remove them first"
            ));
        }
        let mut changed = Vec::new();
        for id in &ids {
            let mut path = self.editable_path(id)?;
            if path.rotate_to_default() {
                changed.push((id.clone(), path));
            }
        }
        if changed.is_empty() {
            return Err("The start point is already there".to_string());
        }
        self.record_action("Reset start point");
        for (id, path) in changed {
            self.store_path(&id, path);
        }
        Ok(ids.len())
    }
}

fn nearest_segment(path: &Path, point: Point2) -> Option<(usize, f64, f64)> {
    let mut best: Option<(usize, f64, f64)> = None;
    for (index, segment) in path.segments.iter().enumerate() {
        for step in 0..=64 {
            let t = f64::from(step) / 64.0;
            let at = match segment {
                Segment::Line { from, to } => {
                    Point2::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
                }
                Segment::Cubic(cubic) => cubic.eval(t),
            };
            let distance = (at.x - point.x).hypot(at.y - point.y);
            if best.is_none_or(|(_, _, current)| distance < current) {
                best = Some((index, t, distance));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, Object};

    fn editor_with_rect() -> (EditorState, String) {
        let mut editor = EditorState::new(Document::new("Start", 200.0, 100.0));
        let id = editor
            .add_rectangle(Point2::new(10.0, 10.0), Point2::new(50.0, 30.0))
            .unwrap();
        (editor, id)
    }

    #[test]
    fn set_start_on_a_closed_path_is_one_undo_step() {
        let (mut editor, id) = editor_with_rect();
        let start = editor
            .set_start_point(&id, Point2::new(30.0, 10.0))
            .unwrap();
        assert!((start.x - 30.0).abs() < 1e-6 && (start.y - 10.0).abs() < 1e-6);
        assert!(editor.undo().is_some());
        assert!(matches!(
            &editor.document.object(&id).unwrap().shape,
            Shape::Rect { .. }
        ));
    }

    #[test]
    fn reverse_changes_the_open_path_ends() {
        let mut editor = EditorState::new(Document::new("Start", 200.0, 100.0));
        let layer = editor.document.layers[0].id.clone();
        let object = Object::new(
            layer,
            Shape::Path(Path::from_points(&[(0.0, 0.0), (10.0, 0.0)], false)),
        );
        let id = object.id.clone();
        editor.document.add_object(object);
        editor.selection.add(id.clone());
        editor.reverse_paths().unwrap();
        let path = editor.document.object(&id).unwrap().world_path();
        assert_eq!(path.start_point(), Some(Point2::new(10.0, 0.0)));
    }

    #[test]
    fn reset_returns_to_the_canonical_start() {
        let (mut editor, id) = editor_with_rect();
        editor
            .set_start_point(&id, Point2::new(50.0, 20.0))
            .unwrap();
        editor.reset_start_points().unwrap();
        let path = editor.document.object(&id).unwrap().world_path();
        let start = path.start_point().unwrap();
        assert!((start.x - 10.0).abs() < 1e-6 && (start.y - 10.0).abs() < 1e-6);
    }
}
