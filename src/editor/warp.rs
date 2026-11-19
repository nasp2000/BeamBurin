//! Non-affine warps: quad, 4×4 lattice and taper.

use crate::document::{Object, Shape};
use crate::geometry::warp::{LatticeWarp, MAX_WARP_SEGMENTS, QuadWarp, TaperSpec, warp_path};
use crate::geometry::{Path, Point2, Transform2};

use super::state::EditorState;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WarpReport {
    pub objects: usize,
    pub segments: usize,
    pub bounds: Option<(Point2, Point2)>,
    pub converted_text: usize,
}

impl EditorState {
    #[allow(clippy::type_complexity)]
    fn warp_targets(
        &self,
        ids: &[String],
    ) -> Result<(Vec<String>, Vec<(String, Vec<Path>)>), String> {
        let mut targets = ids.to_vec();
        loop {
            let mut changed = false;
            for id in targets.clone() {
                if let Some(parent) = self
                    .document
                    .object(&id)
                    .and_then(|object| object.hole_of.clone())
                    && !targets.contains(&parent)
                {
                    targets.push(parent);
                    changed = true;
                }
                for child in &self.document.objects {
                    if child.hole_of.as_deref() == Some(id.as_str()) && !targets.contains(&child.id)
                    {
                        targets.push(child.id.clone());
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut texts = Vec::new();
        for id in &targets {
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
                Shape::Image(_) => {
                    return Err("Images cannot be warped; deselect them first".to_string());
                }
                Shape::Text(text) => {
                    texts.push((id.clone(), crate::objects::text_to_paths(text)?));
                }
                _ => {}
            }
        }
        let constraints = self
            .document
            .constraints
            .iter()
            .filter(|constraint| {
                serde_json::to_string(&constraint.kind)
                    .is_ok_and(|text| targets.iter().any(|id| text.contains(id)))
            })
            .count();
        if constraints > 0 {
            return Err(format!(
                "{constraints} constraints reference the selection; remove them first"
            ));
        }
        Ok((targets, texts))
    }

    fn selection_domain(&self, ids: &[String]) -> Result<(Point2, Point2), String> {
        let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for id in ids {
            if let Some((low, high)) = self.document.object(id).and_then(Object::bounds) {
                min.x = min.x.min(low.x);
                min.y = min.y.min(low.y);
                max.x = max.x.max(high.x);
                max.y = max.y.max(high.y);
            }
        }
        if !min.x.is_finite() || !min.y.is_finite() {
            return Err("Select at least one shape to warp".to_string());
        }
        if max.x - min.x < 1.0 {
            let center = f64::midpoint(min.x, max.x);
            min.x = center - 0.5;
            max.x = center + 0.5;
        }
        if max.y - min.y < 1.0 {
            let center = f64::midpoint(min.y, max.y);
            min.y = center - 0.5;
            max.y = center + 0.5;
        }
        Ok((min, max))
    }

    fn convert_warp_texts(&mut self, texts: Vec<(String, Vec<Path>)>) -> Vec<String> {
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

    fn apply_map(
        &mut self,
        ids: &[String],
        domain: (Point2, Point2),
        map: impl Fn(f64, f64) -> Point2,
        action: &str,
    ) -> Result<WarpReport, String> {
        let (targets, texts) = self.warp_targets(ids)?;
        let converted = !texts.is_empty();
        self.record_action(action);
        let outlines = self.convert_warp_texts(texts);
        let mut targets = targets;
        targets.retain(|id| !outlines.contains(id));
        targets.extend(outlines.iter().cloned());
        let mut segments = 0;
        let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut objects = 0;
        for id in &targets {
            let Some(object) = self.document.object(id) else {
                continue;
            };
            let warped = warp_path(&object.world_path(), domain, &map)?;
            if warped.segments.len() > MAX_WARP_SEGMENTS {
                return Err(format!(
                    "The warp exceeds the {MAX_WARP_SEGMENTS} segment limit; simplify the selection first"
                ));
            }
            segments += warped.segments.len();
            for point in warped.flatten(0.1) {
                min.x = min.x.min(point.x);
                min.y = min.y.min(point.y);
                max.x = max.x.max(point.x);
                max.y = max.y.max(point.y);
            }
            if let Some(object) = self.document.object_mut(id) {
                object.shape = Shape::Path(warped);
                object.transform = Transform2::identity();
                objects += 1;
            }
        }
        if objects == 0 {
            return Err("Nothing changed".to_string());
        }
        self.selection.clear();
        for id in &targets {
            self.selection.add(id.clone());
        }
        Ok(WarpReport {
            objects,
            segments,
            bounds: min.x.is_finite().then_some((min, max)),
            converted_text: if converted { targets.len() } else { 0 },
        })
    }

    pub fn warp_selection(&mut self, corners: [Point2; 4]) -> Result<WarpReport, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one shape to warp".to_string());
        }
        let domain = self.selection_domain(&ids)?;
        let quad =
            QuadWarp::new(corners).ok_or_else(|| "The target quad has no area".to_string())?;
        let identity = corners
            .iter()
            .zip([
                Point2::new(domain.0.x, domain.0.y),
                Point2::new(domain.1.x, domain.0.y),
                Point2::new(domain.0.x, domain.1.y),
                Point2::new(domain.1.x, domain.1.y),
            ])
            .all(|(a, b)| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        if identity {
            return Err("Nothing changed".to_string());
        }
        self.apply_map(&ids, domain, |u, v| quad.map(u, v), "Warp selection")
    }

    pub fn deform_selection(&mut self, nodes: [[Point2; 4]; 4]) -> Result<WarpReport, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one shape to warp".to_string());
        }
        let domain = self.selection_domain(&ids)?;
        let lattice =
            LatticeWarp::new(nodes).ok_or_else(|| "The target lattice has no area".to_string())?;
        self.apply_map(&ids, domain, |u, v| lattice.map(u, v), "Deform selection")
    }

    pub fn taper_warp(&mut self, spec: TaperSpec) -> Result<WarpReport, String> {
        let ids = self.selection.ids().to_vec();
        if ids.is_empty() {
            return Err("Select at least one shape to warp".to_string());
        }
        let domain = self.selection_domain(&ids)?;
        let quad = spec
            .quad(domain)
            .ok_or_else(|| "The taper has invalid widths or length".to_string())?;
        self.apply_map(&ids, domain, |u, v| quad.map(u, v), "Taper warp")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::geometry::warp::TaperOrientation;

    fn editor_with_rect() -> (EditorState, String) {
        let mut editor = EditorState::new(Document::new("Warp", 300.0, 200.0));
        let id = editor
            .add_rectangle(Point2::new(10.0, 10.0), Point2::new(50.0, 40.0))
            .unwrap();
        (editor, id)
    }

    #[test]
    fn warp_bakes_geometry_and_is_undoable() {
        let (mut editor, id) = editor_with_rect();
        let report = editor
            .warp_selection([
                Point2::new(10.0, 10.0),
                Point2::new(60.0, 10.0),
                Point2::new(10.0, 40.0),
                Point2::new(50.0, 40.0),
            ])
            .unwrap();
        assert_eq!(report.objects, 1);
        let object = editor.document.object(&id).unwrap();
        assert!(matches!(&object.shape, Shape::Path(_)));
        assert_eq!(object.transform, Transform2::identity());
        assert!(editor.undo().is_some());
        assert!(matches!(
            &editor.document.object(&id).unwrap().shape,
            Shape::Rect { .. }
        ));
    }

    #[test]
    fn identity_warp_is_a_no_op() {
        let (mut editor, _) = editor_with_rect();
        assert!(
            editor
                .warp_selection([
                    Point2::new(10.0, 10.0),
                    Point2::new(50.0, 10.0),
                    Point2::new(10.0, 40.0),
                    Point2::new(50.0, 40.0),
                ])
                .is_err()
        );
    }

    #[test]
    fn taper_deforms_the_selection() {
        let (mut editor, _) = editor_with_rect();
        let report = editor
            .taper_warp(crate::geometry::warp::TaperSpec {
                top_width: 10.0,
                bottom_width: 40.0,
                length: 30.0,
                orientation: TaperOrientation::Top,
                diameters: false,
                wrap_fraction: 100.0,
            })
            .unwrap();
        assert_eq!(report.objects, 1);
        assert!(report.segments > 0);
    }

    #[test]
    fn warp_refuses_images() {
        let (mut editor, _) = editor_with_rect();
        let image = crate::document::Object::new(
            editor.document.layers[0].id.clone(),
            crate::document::Shape::Image(crate::objects::ImageObject::new(
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
        assert!(
            editor
                .warp_selection([
                    Point2::new(0.0, 0.0),
                    Point2::new(10.0, 0.0),
                    Point2::new(0.0, 10.0),
                    Point2::new(10.0, 10.0),
                ])
                .is_err()
        );
    }
}
