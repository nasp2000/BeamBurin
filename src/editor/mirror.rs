//! Mirror across a line and two-point rotate/scale.

use crate::document::{Object, Shape};
use crate::geometry::path::Segment;
use crate::geometry::{Path, Point2, Transform2};

use super::state::EditorState;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MirrorReport {
    pub mirrored: usize,
    pub skipped_locked: usize,
    pub converted_text: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TwoPointReport {
    pub transformed: usize,
    pub skipped_locked: usize,
    pub converted_text: usize,
    pub angle_degrees: f64,
    pub scale: f64,
}

impl EditorState {
    fn expand_with_holes(&self, ids: &mut Vec<String>) {
        loop {
            let mut changed = false;
            for id in ids.clone() {
                if let Some(parent) = self
                    .document
                    .object(&id)
                    .and_then(|object| object.hole_of.clone())
                    && !ids.contains(&parent)
                {
                    ids.push(parent);
                    changed = true;
                }
                for child in &self.document.objects {
                    if child.hole_of.as_deref() == Some(id.as_str()) && !ids.contains(&child.id) {
                        ids.push(child.id.clone());
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn prepare_targets(
        &self,
        ids: &[String],
    ) -> Result<(Vec<String>, Vec<(String, Vec<Path>)>), String> {
        let mut targets = ids.to_vec();
        self.expand_with_holes(&mut targets);
        let mut texts = Vec::new();
        for id in &targets {
            let object = self
                .document
                .object(id)
                .ok_or_else(|| format!("unknown object: {id}"))?;
            if object.array_id.is_some() {
                return Err("Flatten the array first".to_string());
            }
            match &object.shape {
                Shape::Image(_) => {
                    return Err(
                        "Images cannot be mirrored or rotated; scale them instead".to_string()
                    );
                }
                Shape::Text(text) => {
                    let paths = crate::objects::text_to_paths(text)?;
                    texts.push((id.clone(), paths));
                }
                _ => {}
            }
        }
        Ok((targets, texts))
    }

    fn apply_to_targets(&mut self, ids: &[String], matrix: &Transform2) -> usize {
        let mut changed = 0;
        for id in ids {
            let Some(object) = self.document.object(id) else {
                continue;
            };
            let layer_locked = self
                .document
                .layer(&object.layer_id)
                .is_some_and(|layer| layer.locked);
            if object.locked || layer_locked {
                continue;
            }
            if let Some(object) = self.document.object_mut(id) {
                object.transform = matrix.compose(&object.transform);
                changed += 1;
            }
        }
        changed
    }

    fn convert_texts(&mut self, texts: Vec<(String, Vec<Path>)>) -> Vec<String> {
        let mut outlines = Vec::new();
        for (id, paths) in texts {
            let Some(object) = self.document.object(&id).cloned() else {
                continue;
            };
            self.document.remove_object(&id);
            for (index, path) in paths.into_iter().enumerate() {
                let path = path.transform(&object.transform);
                let mut outline = object.clone();
                outline.id = uuid::Uuid::new_v4().to_string();
                outline.name = format!("{} outline {}", object.name, index + 1);
                outline.shape = Shape::Path(path);
                outline.transform = Transform2::identity();
                outline.hole_of = None;
                outlines.push(outline.id.clone());
                self.document.add_object(outline);
            }
        }
        outlines
    }

    /// Mirror targets across the axis formed by the last selected straight path.
    pub fn mirror_across_line(&mut self) -> Result<MirrorReport, String> {
        let selected = self.selection.ids().to_vec();
        if selected.len() < 2 {
            return Err("Select the shapes and a straight line last".to_string());
        }
        let axis_id = selected.last().cloned().unwrap_or_default();
        let targets: Vec<String> = selected.into_iter().filter(|id| id != &axis_id).collect();
        let axis = self
            .document
            .object(&axis_id)
            .ok_or_else(|| format!("unknown object: {axis_id}"))?;
        let (a, b) = axis_points(axis)
            .ok_or_else(|| "The mirror line must be a straight line".to_string())?;
        self.mirror_ids_across_line(&targets, a, b)
    }

    /// Mirror explicit target ids across the line through `a` and `b`.
    pub fn mirror_ids_across_line(
        &mut self,
        ids: &[String],
        a: Point2,
        b: Point2,
    ) -> Result<MirrorReport, String> {
        if ids.is_empty() {
            return Err("Select the shapes and a straight line last".to_string());
        }
        let matrix = Transform2::reflect_across_line(a, b)
            .ok_or_else(|| "The mirror line must be a straight line".to_string())?;
        let (targets, texts) = self.prepare_targets(ids)?;
        let unlocked: Vec<String> = targets
            .iter()
            .filter(|id| {
                self.document.object(id).is_some_and(|object| {
                    !object.locked
                        && !self
                            .document
                            .layer(&object.layer_id)
                            .is_some_and(|layer| layer.locked)
                })
            })
            .cloned()
            .collect();
        if unlocked.is_empty() {
            return Err("Every target is locked".to_string());
        }
        self.record_action("Mirror across line");
        let outlines = self.convert_texts(texts);
        let mut targets = targets;
        targets.retain(|id| !outlines.contains(id));
        targets.extend(outlines.iter().cloned());
        let mirrored = self.apply_to_targets(&targets, &matrix);
        self.selection.clear();
        for id in &targets {
            self.selection.add(id.clone());
        }
        Ok(MirrorReport {
            mirrored,
            skipped_locked: targets.len().saturating_sub(mirrored),
            converted_text: outlines.len(),
        })
    }

    /// Rotate and optionally scale the selection around an explicit pivot.
    pub fn two_point_transform(
        &mut self,
        pivot: Point2,
        from: Point2,
        to: Point2,
        scale: bool,
    ) -> Result<TwoPointReport, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one shape first".to_string());
        }
        self.two_point_ids(&ids, pivot, from, to, scale)
    }

    pub fn two_point_ids(
        &mut self,
        ids: &[String],
        pivot: Point2,
        from: Point2,
        to: Point2,
        scale: bool,
    ) -> Result<TwoPointReport, String> {
        let v0 = Point2::new(from.x - pivot.x, from.y - pivot.y);
        let v1 = Point2::new(to.x - pivot.x, to.y - pivot.y);
        let r0 = v0.x.hypot(v0.y);
        let r1 = v1.x.hypot(v1.y);
        if r0 < 0.5 || r1 < 0.5 {
            return Err("Drag further from the centre".to_string());
        }
        let angle = v1.y.atan2(v1.x) - v0.y.atan2(v0.x);
        let ratio = if scale {
            (r1 / r0).clamp(0.01, 100.0)
        } else {
            1.0
        };
        let matrix = Transform2::similarity_about(pivot, angle, ratio)
            .ok_or_else(|| "The transform is not finite".to_string())?;
        let (targets, texts) = self.prepare_targets(ids)?;
        let unlocked: Vec<String> = targets
            .iter()
            .filter(|id| {
                self.document.object(id).is_some_and(|object| {
                    !object.locked
                        && !self
                            .document
                            .layer(&object.layer_id)
                            .is_some_and(|layer| layer.locked)
                })
            })
            .cloned()
            .collect();
        if unlocked.is_empty() {
            return Err("Every target is locked".to_string());
        }
        self.record_action(if scale { "Rotate and scale" } else { "Rotate" });
        let outlines = self.convert_texts(texts);
        let mut targets = targets;
        targets.retain(|id| !outlines.contains(id));
        targets.extend(outlines.iter().cloned());
        let transformed = self.apply_to_targets(&targets, &matrix);
        self.selection.clear();
        for id in &targets {
            self.selection.add(id.clone());
        }
        Ok(TwoPointReport {
            transformed,
            skipped_locked: targets.len().saturating_sub(transformed),
            converted_text: outlines.len(),
            angle_degrees: angle.to_degrees(),
            scale: ratio,
        })
    }
}

fn axis_points(object: &Object) -> Option<(Point2, Point2)> {
    let Shape::Path(path) = &object.shape else {
        return None;
    };
    if path.closed || path.segments.is_empty() {
        return None;
    }
    let mut points = Vec::new();
    for segment in &path.segments {
        let Segment::Line { from, to } = segment else {
            return None;
        };
        if points.is_empty() {
            points.push(*from);
        }
        points.push(*to);
    }
    if points.len() < 2 {
        return None;
    }
    let first = points[0];
    let second = *points
        .iter()
        .find(|point| (point.x - first.x).abs() > 1e-9 || (point.y - first.y).abs() > 1e-9)?;
    let dx = second.x - first.x;
    let dy = second.y - first.y;
    if dx.hypot(dy) < 0.01 {
        return None;
    }
    let collinear = points.iter().all(|point| {
        let px = point.x - first.x;
        let py = point.y - first.y;
        (dx * py - dy * px).abs() <= 1e-6 * dx.hypot(dy)
    });
    collinear.then_some((first, *points.last().unwrap_or(&second)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn editor_with_rect_and_axis() -> (EditorState, String, String) {
        let mut editor = EditorState::new(Document::new("Mirror", 200.0, 200.0));
        let target = editor
            .add_rectangle(Point2::new(10.0, 10.0), Point2::new(20.0, 20.0))
            .unwrap();
        let layer = editor.document.layers[0].id.clone();
        let axis = Object::new(
            layer,
            Shape::Path(Path::from_points(&[(0.0, 0.0), (0.0, 40.0)], false)),
        );
        let axis_id = axis.id.clone();
        editor.document.add_object(axis);
        (editor, target, axis_id)
    }

    #[test]
    fn mirror_uses_the_last_selected_line_and_keeps_it() {
        let (mut editor, target, axis) = editor_with_rect_and_axis();
        editor.selection.clear();
        editor.selection.add(target.clone());
        editor.selection.add(axis.clone());
        let report = editor.mirror_across_line().unwrap();
        assert_eq!(report.mirrored, 1);
        assert!(editor.document.object(&axis).is_some());
        let bounds = editor.document.object(&target).unwrap().bounds().unwrap();
        assert!((bounds.0.x + 20.0).abs() < 1e-6 && (bounds.1.x + 10.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_is_undoable() {
        let (mut editor, target, axis) = editor_with_rect_and_axis();
        editor.selection.clear();
        editor.selection.add(target);
        editor.selection.add(axis);
        editor.mirror_across_line().unwrap();
        assert!(editor.undo().is_some());
        let bounds = editor.document.objects[0].bounds().unwrap();
        assert!((bounds.0.x - 10.0).abs() < 1e-6);
    }

    #[test]
    fn mirror_refuses_images() {
        let (mut editor, _, axis) = editor_with_rect_and_axis();
        let image = Object::new(
            editor.document.layers[0].id.clone(),
            Shape::Image(crate::objects::ImageObject::new(
                "missing.png",
                10,
                10,
                96.0,
            )),
        );
        let image_id = image.id.clone();
        editor.document.add_object(image);
        editor.selection.clear();
        editor.selection.add(image_id);
        editor.selection.add(axis);
        assert!(editor.mirror_across_line().is_err());
    }

    #[test]
    fn two_point_rotates_and_scales_around_the_pivot() {
        let mut editor = EditorState::new(Document::new("Two", 200.0, 200.0));
        let target = editor
            .add_rectangle(Point2::new(10.0, 0.0), Point2::new(20.0, 10.0))
            .unwrap();
        editor.selection.clear();
        editor.selection.add(target.clone());
        let report = editor
            .two_point_transform(
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(0.0, 20.0),
                true,
            )
            .unwrap();
        assert!((report.angle_degrees - 90.0).abs() < 1e-6);
        assert!((report.scale - 2.0).abs() < 1e-6);
        let bounds = editor.document.object(&target).unwrap().bounds().unwrap();
        assert!((bounds.0.x + 20.0).abs() < 1e-6);
    }
}
