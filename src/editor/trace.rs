//! One-image tracing action.

use std::path::Path;

use crate::document::Shape;
use crate::geometry::Point2;
use crate::objects::ImageMode;
use crate::raster::TRACE_MAX_SIDE;
use crate::trace::{TraceOptions, TracePlacement, trace_bitmap};

use super::state::EditorState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TraceReport {
    pub objects: usize,
    pub contours: usize,
    pub holes: usize,
}

impl EditorState {
    pub fn trace_image(
        &mut self,
        image_id: Option<&str>,
        path: Option<&Path>,
        options: &TraceOptions,
        delete_source: bool,
    ) -> Result<TraceReport, String> {
        options.validate()?;
        let (image, source_path, layer_id, operation_id, origin) = if let Some(id) = image_id {
            let index = self
                .document
                .objects
                .iter()
                .position(|object| object.id == id)
                .ok_or_else(|| format!("unknown object: {id}"))?;
            self.ensure_editable(index)?;
            let object = self.document.objects[index].clone();
            let Shape::Image(image) = &object.shape else {
                return Err("Select exactly one image".to_string());
            };
            let Some((min, _)) = object.bounds() else {
                return Err("The image has no bounds".to_string());
            };
            (
                image.clone(),
                std::path::PathBuf::from(&image.path),
                object.layer_id.clone(),
                object.operation_id.clone(),
                min,
            )
        } else if let Some(path) = path {
            let document =
                crate::import::raster::import_raster_with_limit(path, crate::raster::MAX_PIXELS)
                    .map_err(|error| format!("Cannot read '{}': {error}", path.display()))?;
            let object = document
                .objects
                .first()
                .ok_or_else(|| "The file is not an image".to_string())?;
            let Shape::Image(image) = &object.shape else {
                return Err("The file is not an image".to_string());
            };
            (
                image.clone(),
                path.to_path_buf(),
                self.active_layer_id.clone(),
                None,
                Point2::origin(),
            )
        } else {
            return Err("Select one image or provide a file".to_string());
        };

        let mut prepared = image.clone();
        if let crate::trace::ThresholdSource::Custom { value, invert } = options.threshold {
            prepared.settings.mode = ImageMode::Threshold;
            prepared.settings.threshold = value;
            prepared.settings.invert = invert;
        }
        prepared.mask_alpha = options.transparency;
        let bitmap = crate::raster::process_object_preview(&source_path, &prepared, TRACE_MAX_SIDE)
            .map_err(|error| format!("Cannot read '{}': {error}", source_path.display()))?;
        let stem = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("image")
            .to_string();
        let group_id = uuid::Uuid::new_v4().to_string();
        let placement = TracePlacement {
            layer_id,
            operation_id,
            origin,
            stem,
            group_id,
        };
        let outcome = trace_bitmap(
            &bitmap,
            image.width_mm(),
            image.height_mm(),
            &placement,
            options,
        )?;
        self.record_action(if options.mode == crate::trace::TraceMode::Centerline {
            "Trace centerline"
        } else {
            "Trace image"
        });
        if delete_source && let Some(id) = image_id {
            self.document.remove_object(id);
        }
        self.selection.clear();
        let count = outcome.objects.len();
        for object in outcome.objects {
            self.selection.add(object.id.clone());
            self.document.add_object(object);
        }
        Ok(TraceReport {
            objects: count,
            contours: outcome.contours,
            holes: outcome.holes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_square(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut image = image::RgbImage::new(32, 32);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let inside = (8..24).contains(&x) && (8..24).contains(&y);
            *pixel = if inside {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            };
        }
        image.save(&path).unwrap();
        path
    }

    #[test]
    fn trace_image_inserts_objects_and_is_undoable() {
        let path = write_square("bw_trace_core.png");
        let document = crate::import::raster::import_raster(&path).unwrap();
        let image_id = document.objects[0].id.clone();
        let mut editor = EditorState::new(document);
        editor.selection.add(image_id.clone());
        let report = editor
            .trace_image(
                Some(&image_id),
                None,
                &crate::trace::TraceOptions::default(),
                false,
            )
            .unwrap();
        assert!(report.objects >= 1);
        assert!(editor.document.objects.len() > 1);
        assert!(editor.undo().is_some());
        assert_eq!(editor.document.objects.len(), 1);
        let _ = std::fs::remove_file(path);
    }
}
