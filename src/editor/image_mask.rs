//! Vector masks and crops for image objects.

use crate::document::Shape;
use crate::geometry::{Path, Point2, Transform2};
use crate::objects::{ImageCrop, ImageObject};

use super::state::EditorState;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaskReport {
    pub paths: usize,
    pub shapes_removed: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImageCropReport {
    pub crop: Option<ImageCrop>,
    pub width_mm: f64,
    pub height_mm: f64,
}

impl EditorState {
    fn image_index(&self, id: &str) -> Result<usize, String> {
        let index = self
            .document
            .objects
            .iter()
            .position(|object| object.id == id)
            .ok_or_else(|| format!("unknown object: {id}"))?;
        self.ensure_editable(index)?;
        if self.document.objects[index].array_id.is_some() {
            return Err("Flatten the array first".to_string());
        }
        if !matches!(self.document.objects[index].shape, Shape::Image(_)) {
            return Err("Select exactly one image".to_string());
        }
        Ok(index)
    }

    fn closed_shape_paths(&self, ids: &[String], image_id: &str) -> Vec<(String, Path)> {
        ids.iter()
            .filter(|id| id.as_str() != image_id)
            .filter_map(|id| {
                let object = self.document.object(id)?;
                if matches!(object.shape, Shape::Text(_) | Shape::Image(_)) {
                    return None;
                }
                let path = object.world_path();
                (path.closed && !path.is_empty()).then_some((id.clone(), path))
            })
            .collect()
    }

    /// Use closed shapes as a vector mask for one image.
    pub fn apply_image_mask(
        &mut self,
        image_id: &str,
        shape_ids: &[String],
        keep_shapes: bool,
        invert: bool,
    ) -> Result<MaskReport, String> {
        let index = self.image_index(image_id)?;
        let shapes = self.closed_shape_paths(shape_ids, image_id);
        if shapes.is_empty() {
            return Err("Select at least one closed shape".to_string());
        }
        let object = self.document.objects[index].clone();
        let inverse = object
            .transform
            .invert()
            .ok_or_else(|| "The image transform is not invertible".to_string())?;
        let mut paths = Vec::new();
        for (_, path) in &shapes {
            let local = path.transform(&inverse);
            if crate::geometry::boolean::polygon_area(
                &local
                    .flatten(0.2)
                    .into_iter()
                    .map(|point| (point.x, point.y))
                    .collect::<Vec<_>>(),
            )
            .abs()
                < 1e-9
            {
                return Err("Select at least one closed shape".to_string());
            }
            paths.push(local);
        }
        self.record_action("Apply mask to image");
        if let Shape::Image(image) = &mut self.document.objects[index].shape {
            image.mask_paths = paths;
            image.mask_invert = invert;
        }
        let mut removed = 0;
        if !keep_shapes {
            let ids: Vec<String> = shapes.iter().map(|(id, _)| id.clone()).collect();
            for id in &ids {
                if self.document.remove_object(id).is_some() {
                    removed += 1;
                }
            }
            self.selection.clear();
            self.selection.add(image_id.to_string());
        }
        Ok(MaskReport {
            paths: shapes.len(),
            shapes_removed: removed,
        })
    }

    /// Clear the vector mask but leave colour and alpha masks alone.
    pub fn remove_image_mask(&mut self, image_id: &str) -> Result<bool, String> {
        let index = self.image_index(image_id)?;
        let has_mask = match &self.document.objects[index].shape {
            Shape::Image(image) => !image.mask_paths.is_empty() || image.mask_invert,
            _ => false,
        };
        if !has_mask {
            return Err("The image has no vector mask".to_string());
        }
        self.record_action("Remove mask from image");
        if let Shape::Image(image) = &mut self.document.objects[index].shape {
            image.mask_paths.clear();
            image.mask_invert = false;
        }
        Ok(true)
    }

    /// Crop one image to the bounding box of closed shapes.
    pub fn crop_image_to_shapes(
        &mut self,
        image_id: &str,
        shape_ids: &[String],
    ) -> Result<ImageCropReport, String> {
        let index = self.image_index(image_id)?;
        let object = self.document.objects[index].clone();
        let Shape::Image(image) = &object.shape else {
            return Err("Select exactly one image".to_string());
        };
        if image.settings.target_width_px > 0 || image.settings.target_height_px > 0 {
            return Err("Clear the target size before cropping".to_string());
        }
        if !image.mask_paths.is_empty() || image.mask_invert {
            return Err("Remove the image mask before cropping".to_string());
        }
        let shapes = self.closed_shape_paths(shape_ids, image_id);
        if shapes.is_empty() {
            return Err("Select at least one closed shape".to_string());
        }
        let inverse = object
            .transform
            .invert()
            .ok_or_else(|| "The image transform is not invertible".to_string())?;
        let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (_, path) in &shapes {
            let local = path.transform(&inverse);
            let Some((low, high)) = local.bounds() else {
                continue;
            };
            min.x = min.x.min(low.x);
            min.y = min.y.min(low.y);
            max.x = max.x.max(high.x);
            max.y = max.y.max(high.y);
        }
        if !min.x.is_finite() || !min.y.is_finite() {
            return Err("The selection is too small to crop".to_string());
        }
        let width_mm = image.width_mm();
        let height_mm = image.height_mm();
        let x0 = min.x.clamp(0.0, width_mm);
        let y0 = min.y.clamp(0.0, height_mm);
        let x1 = max.x.clamp(0.0, width_mm);
        let y1 = max.y.clamp(0.0, height_mm);
        if x1 - x0 < 1e-6 || y1 - y0 < 1e-6 {
            return Err("The selection is too small to crop".to_string());
        }
        if (x0 - 0.0).abs() < 1e-9
            && (y0 - 0.0).abs() < 1e-9
            && (x1 - width_mm).abs() < 1e-9
            && (y1 - height_mm).abs() < 1e-9
        {
            return Err("Nothing to crop".to_string());
        }
        let pixels_per_mm = image.dpi / 25.4;
        let left = (x0 * pixels_per_mm).floor().max(0.0);
        let right = (x1 * pixels_per_mm).ceil().min(f64::from(image.width_px));
        let top = ((height_mm - y1) * pixels_per_mm).floor().max(0.0);
        let bottom = ((height_mm - y0) * pixels_per_mm)
            .ceil()
            .min(f64::from(image.height_px));
        let (source_x, source_y, source_w, source_h) = source_rect(image, left, top, right, bottom);
        if source_w == 0 || source_h == 0 {
            return Err("The selection is too small to crop".to_string());
        }
        let crop = ImageCrop {
            x: source_x,
            y: source_y,
            width: source_w,
            height: source_h,
        };
        self.record_action("Crop image");
        let offset = Point2::new(x0, y0);
        if let Some(object) = self.document.object_mut(image_id) {
            if let Shape::Image(image) = &mut object.shape {
                image.crop = Some(crop);
            }
            object.transform = Transform2::translate(offset.x, offset.y).compose(&object.transform);
        }
        let updated = self.document.object(image_id).and_then(|object| {
            if let Shape::Image(image) = &object.shape {
                Some((image.width_mm(), image.height_mm()))
            } else {
                None
            }
        });
        let (width_mm, height_mm) = updated.unwrap_or((0.0, 0.0));
        Ok(ImageCropReport {
            crop: Some(crop),
            width_mm,
            height_mm,
        })
    }

    /// Clear a crop, keeping the current local origin.
    pub fn remove_image_crop(&mut self, image_id: &str) -> Result<bool, String> {
        let index = self.image_index(image_id)?;
        let has_crop = matches!(&self.document.objects[index].shape, Shape::Image(image) if image.crop.is_some());
        if !has_crop {
            return Err("The image has no crop".to_string());
        }
        self.record_action("Remove crop from image");
        if let Shape::Image(image) = &mut self.document.objects[index].shape {
            image.crop = None;
        }
        Ok(true)
    }
}

fn source_rect(
    image: &ImageObject,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
) -> (u32, u32, u32, u32) {
    let cropped_w = f64::from(image.cropped_size_px().0);
    let cropped_h = f64::from(image.cropped_size_px().1);
    let rotation = image.settings.rotation_degrees;
    let (cx0, cy0, cx1, cy1) = if (rotation - 90.0).abs() < 0.5 {
        (top, cropped_h - right, bottom, cropped_h - left)
    } else if (rotation - 180.0).abs() < 0.5 {
        (
            cropped_w - right,
            cropped_h - bottom,
            cropped_w - left,
            cropped_h - top,
        )
    } else if (rotation - 270.0).abs() < 0.5 {
        (cropped_w - bottom, left, cropped_w - top, right)
    } else {
        (left, top, right, bottom)
    };
    let offset_x = image.crop.map_or(0.0, |crop| f64::from(crop.x));
    let offset_y = image.crop.map_or(0.0, |crop| f64::from(crop.y));
    let x = (cx0 + offset_x)
        .floor()
        .clamp(0.0, f64::from(image.width_px));
    let y = (cy0 + offset_y)
        .floor()
        .clamp(0.0, f64::from(image.height_px));
    let x1 = (cx1 + offset_x)
        .ceil()
        .clamp(0.0, f64::from(image.width_px));
    let y1 = (cy1 + offset_y)
        .ceil()
        .clamp(0.0, f64::from(image.height_px));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x, y, w, h) = (
        x as u32,
        y as u32,
        (x1 - x).max(1.0) as u32,
        (y1 - y).max(1.0) as u32,
    );
    (x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;
    use crate::objects::{ImageCrop, ImageObject};

    fn editor_with_image() -> (EditorState, String) {
        let mut editor = EditorState::new(Document::new("Mask", 300.0, 200.0));
        let layer = editor.document.layers[0].id.clone();
        let image = Object::new(
            layer,
            Shape::Image(ImageObject::new("test.png", 100, 50, 25.4)),
        );
        let id = image.id.clone();
        editor.document.add_object(image);
        (editor, id)
    }

    fn shape(editor: &mut EditorState, x0: f64, y0: f64, x1: f64, y1: f64) -> String {
        editor
            .add_rectangle(Point2::new(x0, y0), Point2::new(x1, y1))
            .unwrap()
    }

    #[test]
    fn apply_mask_stores_closed_paths_in_image_coordinates() {
        let (mut editor, image) = editor_with_image();
        let shape = shape(&mut editor, 10.0, 10.0, 40.0, 30.0);
        let report = editor
            .apply_image_mask(&image, &[shape], true, false)
            .unwrap();
        assert_eq!(report.paths, 1);
        assert!(editor.document.object(&shape).is_some());
        let object = editor.document.object(&image).unwrap();
        let Shape::Image(image) = &object.shape else {
            panic!("not an image");
        };
        assert_eq!(image.mask_paths.len(), 1);
        assert!(image.mask_paths[0].closed);
        assert!(!image.mask_invert);
    }

    #[test]
    fn remove_mask_keeps_colour_and_alpha_masks() {
        let (mut editor, image) = editor_with_image();
        if let Some(object) = editor.document.object_mut(&image)
            && let Shape::Image(image) = &mut object.shape
        {
            image.mask_colours = vec!["#ff0000".to_string()];
            image.mask_alpha = true;
            image.mask_invert = true;
        }
        editor.remove_image_mask(&image).unwrap();
        let Shape::Image(image) = &editor.document.object(&image).unwrap().shape else {
            panic!("not an image");
        };
        assert!(image.mask_paths.is_empty());
        assert!(!image.mask_invert);
        assert_eq!(image.mask_colours.len(), 1);
        assert!(image.mask_alpha);
    }

    #[test]
    fn crop_uses_the_shape_box_and_is_undoable() {
        let (mut editor, image) = editor_with_image();
        let shape = shape(&mut editor, 10.0, 10.0, 40.0, 30.0);
        let report = editor.crop_image_to_shapes(&image, &[shape]).unwrap();
        assert_eq!(
            report.crop,
            Some(ImageCrop {
                x: 10,
                y: 20,
                width: 30,
                height: 20
            })
        );
        assert!((report.width_mm - 30.0).abs() < 1e-6);
        assert!((report.height_mm - 20.0).abs() < 1e-6);
        assert!(editor.undo().is_some());
        let object = editor.document.object(&image).unwrap();
        assert!(matches!(&object.shape, Shape::Image(image) if image.crop.is_none()));
    }

    #[test]
    fn crop_refuses_a_mask_or_target_size() {
        let (mut editor, image) = editor_with_image();
        let shape = shape(&mut editor, 10.0, 10.0, 40.0, 30.0);
        if let Some(object) = editor.document.object_mut(&image)
            && let Shape::Image(image) = &mut object.shape
        {
            image.mask_invert = true;
        }
        assert!(editor.crop_image_to_shapes(&image, &[shape]).is_err());
    }
}
