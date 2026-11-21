//! One-image tracing action.

use std::path::Path;

use crate::document::Shape;
use crate::geometry::Point2;
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
    #[allow(clippy::too_many_lines)]
    pub fn trace_image(
        &mut self,
        image_id: Option<&str>,
        path: Option<&Path>,
        options: &TraceOptions,
        delete_source: bool,
    ) -> Result<TraceReport, String> {
        options.validate()?;
        let (image, source_path, layer_id, operation_id, origin, size) = if let Some(id) = image_id
        {
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
            let Some((min, max)) = object.bounds() else {
                return Err("The image has no bounds".to_string());
            };
            // A scaled image is traced at the size it occupies on the page;
            // a rotated one keeps its own size so the paths stay axis-aligned.
            let axis_aligned = object.transform.b.abs() < 1e-9 && object.transform.c.abs() < 1e-9;
            let size = if axis_aligned {
                (max.x - min.x, max.y - min.y)
            } else {
                (image.width_mm(), image.height_mm())
            };
            (
                image.clone(),
                std::path::PathBuf::from(&image.path),
                object.layer_id.clone(),
                object.operation_id.clone(),
                min,
                size,
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
                (image.width_mm(), image.height_mm()),
            )
        } else {
            return Err("Select one image or provide a file".to_string());
        };

        let mut prepared = image.clone();
        prepared.mask_alpha = options.transparency;
        let bitmap = match options.threshold {
            crate::trace::ThresholdSource::Custom { low, high, invert } => {
                crate::raster::process_object_band(
                    &source_path,
                    &prepared,
                    TRACE_MAX_SIDE,
                    low,
                    high,
                    invert,
                )
            }
            crate::trace::ThresholdSource::ImageSettings => {
                crate::raster::process_object_preview(&source_path, &prepared, TRACE_MAX_SIDE)
            }
        }
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
        let outcome = trace_bitmap(&bitmap, size.0, size.1, &placement, options)?;
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

    fn write_bands(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut image = image::RgbImage::new(30, 10);
        for (x, _y, pixel) in image.enumerate_pixels_mut() {
            let tone = match x {
                0..=9 => 40,
                10..=19 => 128,
                _ => 235,
            };
            *pixel = image::Rgb([tone, tone, tone]);
        }
        image.save(&path).unwrap();
        path
    }

    #[test]
    fn a_custom_band_traces_only_the_chosen_tones() {
        let path = write_bands("bw_trace_band.png");
        let document = crate::import::raster::import_raster(&path).unwrap();
        let image_id = document.objects[0].id.clone();
        let (image_min, _) = document.objects[0].bounds().expect("image bounds");
        let image_width = match &document.objects[0].shape {
            crate::document::Shape::Image(image) => image.width_mm(),
            _ => panic!("not an image"),
        };
        let mut editor = EditorState::new(document);
        let options = crate::trace::TraceOptions {
            threshold: crate::trace::ThresholdSource::Custom {
                low: 100,
                high: 150,
                invert: false,
            },
            // Straight segments keep every traced point, so the bounds are the
            // band's own edges.
            smooth: false,
            ..crate::trace::TraceOptions::default()
        };
        let report = editor
            .trace_image(Some(&image_id), None, &options, false)
            .unwrap();
        assert_eq!(report.contours, 1, "the band did not trace one contour");
        let traced = editor
            .document
            .objects
            .iter()
            .filter(|object| object.id != image_id)
            .collect::<Vec<_>>();
        assert_ne!(traced.len(), 0, "no paths were traced");
        let (min, max) = traced[0].bounds().expect("the traced path has no bounds");
        let third = image_width / 3.0;
        assert!(
            min.x >= image_min.x + third * 0.8 && max.x <= image_min.x + third * 2.2,
            "the traced path left the middle band: {min:?}..{max:?} of {image_width}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_scaled_image_is_traced_at_the_size_on_the_page() {
        let path = write_square("bw_trace_scaled.png");
        let document = crate::import::raster::import_raster(&path).unwrap();
        let image_id = document.objects[0].id.clone();
        let mut editor = EditorState::new(document);
        editor.document.object_mut(&image_id).unwrap().transform =
            crate::geometry::Transform2::scale(2.0, 2.0);
        let (image_min, image_max) = editor.document.object(&image_id).unwrap().bounds().unwrap();
        let scaled_width = image_max.x - image_min.x;
        let report = editor
            .trace_image(
                Some(&image_id),
                None,
                &crate::trace::TraceOptions {
                    smooth: false,
                    ..crate::trace::TraceOptions::default()
                },
                false,
            )
            .unwrap();
        assert!(report.contours >= 1);
        let traced = editor
            .document
            .objects
            .iter()
            .find(|object| object.id != image_id)
            .expect("no traced path");
        let (min, max) = traced.bounds().expect("the traced path has no bounds");
        let width = max.x - min.x;
        // The fixture squares the middle half of the picture.
        let expected = scaled_width * 0.5;
        assert!(
            (width - expected).abs() < 0.5,
            "the trace ignored the scale: {width} against {expected}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn tracing_can_delete_the_source_image() {
        let path = write_square("bw_trace_delete.png");
        let document = crate::import::raster::import_raster(&path).unwrap();
        let image_id = document.objects[0].id.clone();
        let mut editor = EditorState::new(document);
        let report = editor
            .trace_image(
                Some(&image_id),
                None,
                &crate::trace::TraceOptions::default(),
                true,
            )
            .unwrap();
        assert!(report.objects >= 1);
        assert!(
            editor.document.object(&image_id).is_none(),
            "the source image was not deleted"
        );
        assert!(editor.undo().is_some());
        let _ = std::fs::remove_file(path);
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
