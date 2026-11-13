#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::Path;

use crate::document::shape::Shape;
use crate::document::{Document, Object};
use crate::errors::Result;
use crate::geometry::Point2;

/// Raster output settings (§14.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PngOptions {
    /// Output resolution in dots per inch.
    pub dpi: f64,
    /// Transparent background instead of white.
    pub transparent: bool,
    /// Page region to render in millimetres; `None` renders the whole page.
    pub area: Option<PngArea>,
    /// Explicit output width in pixels; when only one side is set the other
    /// follows the region aspect ratio.
    pub width_px: Option<u32>,
    /// Explicit output height in pixels.
    pub height_px: Option<u32>,
    /// Blank margin around the chosen region, in millimetres.
    pub margin_mm: f64,
    /// Horizontal shift of the captured region, in millimetres.
    pub offset_x_mm: f64,
    /// Vertical shift of the captured region, in millimetres.
    pub offset_y_mm: f64,
}

/// A rectangle of the page, in millimetres from the bottom-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Earlier name of [`Area`], kept for the PNG call sites.
pub type PngArea = Area;

impl Default for PngOptions {
    fn default() -> Self {
        Self {
            dpi: 96.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        }
    }
}

impl PngOptions {
    pub fn validate(&self) -> Result<()> {
        if !self.dpi.is_finite() || !(10.0..=2400.0).contains(&self.dpi) {
            return Err(crate::errors::BeamError::InvalidInput(
                "PNG resolution must be between 10 and 2400 dpi".to_string(),
            ));
        }
        if self.width_px == Some(0) || self.height_px == Some(0) {
            return Err(crate::errors::BeamError::InvalidInput(
                "the PNG size must be at least 1 pixel".to_string(),
            ));
        }
        if !self.margin_mm.is_finite() || self.margin_mm < 0.0 {
            return Err(crate::errors::BeamError::InvalidInput(
                "the PNG margin must be a finite value of zero or more".to_string(),
            ));
        }
        if !self.offset_x_mm.is_finite() || !self.offset_y_mm.is_finite() {
            return Err(crate::errors::BeamError::InvalidInput(
                "the PNG offset must be finite".to_string(),
            ));
        }
        if let Some(area) = self.area {
            let finite = area.x.is_finite()
                && area.y.is_finite()
                && area.width.is_finite()
                && area.height.is_finite();
            if !finite || area.width <= 0.0 || area.height <= 0.0 {
                return Err(crate::errors::BeamError::InvalidInput(
                    "the PNG area must have a positive finite size".to_string(),
                ));
            }
        }
        Ok(())
    }
}

/// Summary of a PNG export.
#[derive(Debug, Clone, PartialEq)]
pub struct PngReport {
    pub width_px: u32,
    pub height_px: u32,
    pub warnings: Vec<String>,
}

/// Largest PNG accepted by the raster export, in pixels.
///
/// A 600 dpi A3 page is about 69 million pixels, so this leaves room for
/// normal high-resolution exports without trying to allocate a picture no
/// machine in the target range can hold.
pub const MAX_PNG_PIXELS: u64 = 100_000_000;

struct Canvas {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    scale_x: f64,
    scale_y: f64,
    /// Left edge and bottom edge of the rendered region, in millimetres.
    origin: (f64, f64),
    height_mm: f64,
    /// `true` when explicit dimensions change the region's aspect ratio.
    stretched: bool,
}

impl Canvas {
    fn new(document: &Document, options: &PngOptions) -> Result<Self> {
        let resolution = options.dpi / 25.4;
        let (origin_x, origin_y, width_mm, height_mm) = match options.area {
            Some(area) => {
                let epsilon = 1e-6;
                if area.x < -epsilon
                    || area.y < -epsilon
                    || area.x + area.width > document.width_mm + epsilon
                    || area.y + area.height > document.height_mm + epsilon
                {
                    return Err(crate::errors::BeamError::InvalidInput(
                        "the PNG area must lie inside the page".to_string(),
                    ));
                }
                (area.x, area.y, area.width, area.height)
            }
            None => (0.0, 0.0, document.width_mm, document.height_mm),
        };
        let margin = options.margin_mm;
        let origin_x = origin_x - margin + options.offset_x_mm;
        let origin_y = origin_y - margin + options.offset_y_mm;
        let width_mm = width_mm + margin * 2.0;
        let height_mm = height_mm + margin * 2.0;
        let (width, height) = match (options.width_px, options.height_px) {
            (Some(width), Some(height)) => (f64::from(width), f64::from(height)),
            (Some(width), None) => (
                f64::from(width),
                (f64::from(width) * height_mm / width_mm).round().max(1.0),
            ),
            (None, Some(height)) => (
                (f64::from(height) * width_mm / height_mm).round().max(1.0),
                f64::from(height),
            ),
            (None, None) => (
                ((width_mm * resolution).round()).max(1.0),
                ((height_mm * resolution).round()).max(1.0),
            ),
        };
        if !width.is_finite()
            || !height.is_finite()
            || width > f64::from(u32::MAX)
            || height > f64::from(u32::MAX)
        {
            return Err(crate::errors::BeamError::InvalidInput(
                "the PNG is too large for the requested resolution".to_string(),
            ));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width_px = width as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let height_px = height as u32;
        let total = u128::from(width_px) * u128::from(height_px);
        if total > u128::from(MAX_PNG_PIXELS) {
            return Err(crate::errors::BeamError::InvalidInput(format!(
                "the PNG would have {width_px} x {height_px} pixels; the limit is {MAX_PNG_PIXELS}; lower the resolution"
            )));
        }
        let width = usize::try_from(width_px).unwrap_or(1);
        let height = usize::try_from(height_px).unwrap_or(1);
        let scale_x = width as f64 / width_mm;
        let scale_y = height as f64 / height_mm;
        // A size asked for in whole pixels rounds each axis by up to half a
        // pixel. That alone is not a stretched picture: only warn when the two
        // axes disagree by more than a pixel's worth over the smaller side.
        let aspect_error_px = (scale_x - scale_y).abs() * width_mm.min(height_mm);
        let stretched = aspect_error_px > 1.0;
        let background = if options.transparent {
            [0, 0, 0, 0]
        } else {
            [255, 255, 255, 255]
        };
        let mut pixels = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            pixels.extend_from_slice(&background);
        }
        Ok(Self {
            width,
            height,
            pixels,
            scale_x,
            scale_y,
            origin: (origin_x, origin_y),
            height_mm,
            stretched,
        })
    }

    /// Document millimetres to pixel coordinates (y axis flipped).
    fn px(&self, point: Point2) -> (f64, f64) {
        (
            (point.x - self.origin.0) * self.scale_x,
            (self.origin.1 + self.height_mm - point.y) * self.scale_y,
        )
    }

    fn stroke_scale(&self) -> f64 {
        f64::midpoint(self.scale_x, self.scale_y)
    }

    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn blend(&mut self, x: i64, y: i64, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let index = (y as usize * self.width + x as usize) * 4;
        let alpha = u16::from(color[3]);
        if alpha == 255 || self.pixels[index + 3] == 0 {
            self.pixels[index..index + 4].copy_from_slice(&color);
            return;
        }
        for (channel, value) in color.iter().take(3).enumerate() {
            let dst = u16::from(self.pixels[index + channel]);
            let src = u16::from(*value);
            #[allow(clippy::cast_possible_truncation)]
            let value = ((src * alpha + dst * (255 - alpha)) / 255) as u8;
            self.pixels[index + channel] = value;
        }
        self.pixels[index + 3] = 255;
    }

    fn fill_polygon_with_holes(
        &mut self,
        points: &[Point2],
        holes: &[Vec<Point2>],
        color: [u8; 4],
    ) {
        if points.len() < 3 {
            return;
        }
        let pixels: Vec<(f64, f64)> = points.iter().map(|point| self.px(*point)).collect();
        let hole_pixels: Vec<Vec<(f64, f64)>> = holes
            .iter()
            .filter(|hole| hole.len() >= 3)
            .map(|hole| hole.iter().map(|point| self.px(*point)).collect())
            .collect();
        let min_y = pixels
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0);
        let max_y = pixels
            .iter()
            .map(|(_, y)| *y)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min((self.height - 1) as f64);
        let mut row = min_y as i64;
        while f64::from(row as i32) <= max_y {
            let scan = row as f64 + 0.5;
            let mut crossings: Vec<f64> = Vec::new();
            for contour in std::iter::once(&pixels).chain(hole_pixels.iter()) {
                for index in 0..contour.len() {
                    let (x0, y0) = contour[index];
                    let (x1, y1) = contour[(index + 1) % contour.len()];
                    if (y0 <= scan) != (y1 <= scan) {
                        let t = (scan - y0) / (y1 - y0);
                        crossings.push(x0 + t * (x1 - x0));
                    }
                }
            }
            crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for pair in crossings.chunks_exact(2) {
                let start = pair[0].ceil() as i64;
                let end = pair[1].floor() as i64;
                for x in start..=end {
                    self.blend(x, row, color);
                }
            }
            row += 1;
        }
    }

    fn stroke_polyline(&mut self, points: &[Point2], width_px: f64, color: [u8; 4]) {
        if points.len() < 2 {
            return;
        }
        let radius = (width_px / 2.0).max(0.5);
        let pixels: Vec<(f64, f64)> = points.iter().map(|point| self.px(*point)).collect();
        for window in pixels.windows(2) {
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            let steps = ((x1 - x0).hypot(y1 - y0)).ceil().max(1.0) as usize;
            for step in 0..=steps {
                let t = step as f64 / steps as f64;
                self.stamp(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, radius, color);
            }
        }
    }

    fn stamp(&mut self, x: f64, y: f64, radius: f64, color: [u8; 4]) {
        let extent = radius.ceil() as i64;
        let cx = x.round() as i64;
        let cy = y.round() as i64;
        for dy in -extent..=extent {
            for dx in -extent..=extent {
                if f64::from(dx as i32).hypot(f64::from(dy as i32)) <= radius {
                    self.blend(cx + dx, cy + dy, color);
                }
            }
        }
    }
}

/// Render the document to an RGBA buffer.
pub fn render_rgba(
    document: &Document,
    options: &PngOptions,
) -> Result<(Vec<u8>, u32, u32, Vec<String>)> {
    render_rgba_cancellable(document, options, &|| false)
}

fn png_cancelled() -> crate::errors::BeamError {
    crate::errors::BeamError::InvalidInput("cancelled".to_string())
}

/// Render the document, checking `cancel` between layers and objects.
pub fn render_rgba_cancellable(
    document: &Document,
    options: &PngOptions,
    cancel: &dyn Fn() -> bool,
) -> Result<(Vec<u8>, u32, u32, Vec<String>)> {
    options.validate()?;
    document
        .validate()
        .map_err(crate::errors::BeamError::InvalidInput)?;
    let mut canvas = Canvas::new(document, options)?;
    let mut warnings = Vec::new();
    if canvas.stretched {
        warnings.push(
            "the output size does not match the area aspect ratio; the picture is stretched"
                .to_string(),
        );
    }
    let mut layers: Vec<_> = document.layers.iter().collect();
    layers.sort_by_key(|layer| layer.order);
    for layer in layers {
        if cancel() {
            return Err(png_cancelled());
        }
        if !layer.visible {
            continue;
        }
        let (lr, lg, lb) = layer.rgb();
        for object in document
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id && object.visible)
        {
            if cancel() {
                return Err(png_cancelled());
            }
            let holes: Vec<Vec<Point2>> = if object.hole_of.is_some() {
                Vec::new()
            } else {
                document
                    .objects
                    .iter()
                    .filter(|candidate| candidate.hole_of.as_deref() == Some(object.id.as_str()))
                    .map(|candidate| candidate.world_path().flatten(0.1))
                    .filter(|points| points.len() >= 3)
                    .collect()
            };
            draw_object(&mut canvas, object, (lr, lg, lb), &holes, &mut warnings);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    let width = canvas.width as u32;
    #[allow(clippy::cast_possible_truncation)]
    let height = canvas.height as u32;
    Ok((canvas.pixels, width, height, warnings))
}

#[allow(clippy::too_many_lines)]
fn draw_object(
    canvas: &mut Canvas,
    object: &Object,
    layer_rgb: (u8, u8, u8),
    holes: &[Vec<Point2>],
    warnings: &mut Vec<String>,
) {
    let stroke_fallback = [layer_rgb.0, layer_rgb.1, layer_rgb.2];
    let stroke = object
        .style
        .stroke
        .as_deref()
        .and_then(crate::color::parse_hex_color)
        .map_or(stroke_fallback, |(r, g, b)| [r, g, b]);
    let fill = object
        .style
        .fill
        .as_deref()
        .and_then(crate::color::parse_hex_color);

    match &object.shape {
        Shape::Text(text) => {
            let paths = match crate::objects::text_to_paths(text) {
                Ok(paths) if !paths.is_empty() => paths,
                Ok(_) => {
                    warnings.push(format!(
                        "text '{}' has no printable characters and was omitted from the PNG",
                        object.name
                    ));
                    return;
                }
                Err(error) => {
                    warnings.push(format!(
                        "text '{}' could not be outlined and was omitted from the PNG: {error}",
                        object.name
                    ));
                    return;
                }
            };
            let parents = crate::geometry::path::contour_parents(&paths);
            let text_fill = fill.unwrap_or((stroke[0], stroke[1], stroke[2]));
            for (index, path) in paths.iter().enumerate() {
                let world = path.transform(&object.transform);
                let points = world.flatten(0.1);
                if points.len() < 2 {
                    continue;
                }
                if parents[index].is_none() {
                    let holes: Vec<Vec<Point2>> = (0..paths.len())
                        .filter(|&other| parents[other] == Some(index))
                        .map(|other| paths[other].transform(&object.transform).flatten(0.1))
                        .filter(|hole| hole.len() >= 3)
                        .collect();
                    canvas.fill_polygon_with_holes(
                        &points,
                        &holes,
                        [text_fill.0, text_fill.1, text_fill.2, 255],
                    );
                }
                if object.style.stroke.is_some() {
                    canvas.stroke_polyline(
                        &points,
                        object.style.stroke_width_mm * canvas.stroke_scale(),
                        [stroke[0], stroke[1], stroke[2], 255],
                    );
                }
            }
        }
        Shape::Image(image) => {
            let path = Path::new(&image.path);
            match image::open(path) {
                Ok(source) => {
                    if object.transform.b.abs() > 1e-9 || object.transform.c.abs() > 1e-9 {
                        warnings.push(format!(
                            "image '{}' is rotated; the PNG uses its bounding box",
                            object.name
                        ));
                    }
                    let (min, max) = object.bounds().unwrap_or_else(|| image.bounds());
                    let (x0, y0) = canvas.px(Point2::new(min.x, max.y));
                    let (x1, y1) = canvas.px(Point2::new(max.x, min.y));
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let width = ((x1 - x0).abs().round() as u32).max(1);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let height = ((y1 - y0).abs().round() as u32).max(1);
                    let resized =
                        source.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
                    let rgba = resized.to_rgba8();
                    #[allow(clippy::cast_possible_truncation)]
                    let start_x = x0.round() as i64;
                    #[allow(clippy::cast_possible_truncation)]
                    let start_y = y0.round() as i64;
                    for (px, py, pixel) in rgba.enumerate_pixels() {
                        canvas.blend(start_x + i64::from(px), start_y + i64::from(py), pixel.0);
                    }
                }
                Err(error) => warnings.push(format!(
                    "image '{}' could not be read: {error}",
                    object.name
                )),
            }
        }
        _ => {
            let path = object.world_path();
            let points = path.flatten(0.1);
            if let Some((r, g, b)) = fill
                && object.hole_of.is_none()
            {
                canvas.fill_polygon_with_holes(&points, holes, [r, g, b, 255]);
            }
            if object.style.stroke.is_some() {
                canvas.stroke_polyline(
                    &points,
                    object.style.stroke_width_mm * canvas.stroke_scale(),
                    [stroke[0], stroke[1], stroke[2], 255],
                );
            }
        }
    }
}

/// Write the document as a PNG file, atomically.
pub fn export_png(document: &Document, path: &Path, options: &PngOptions) -> Result<PngReport> {
    export_png_cancellable(document, path, options, &|| false)
}

/// Write a PNG atomically, checking `cancel` before touching the file.
pub fn export_png_cancellable(
    document: &Document,
    path: &Path,
    options: &PngOptions,
    cancel: &dyn Fn() -> bool,
) -> Result<PngReport> {
    let (pixels, width, height, warnings) = render_rgba_cancellable(document, options, cancel)?;
    if cancel() {
        return Err(png_cancelled());
    }
    let buffer = image::RgbaImage::from_raw(width, height, pixels)
        .ok_or_else(|| crate::errors::BeamError::InvalidInput("invalid PNG buffer".to_string()))?;
    let temporary = path.with_extension("png.tmp");
    buffer
        .save_with_format(&temporary, image::ImageFormat::Png)
        .map_err(|error| {
            crate::errors::BeamError::InvalidInput(format!("cannot write PNG: {error}"))
        })?;
    std::fs::rename(&temporary, path)?;
    Ok(PngReport {
        width_px: width,
        height_px: height,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;
    use crate::document::model::{Object, Shape};
    use crate::geometry::Transform2;

    fn document_with_rect(fill: Option<&str>, stroke: bool) -> Document {
        let mut document = Document {
            width_mm: 10.0,
            height_mm: 10.0,
            ..Document::default()
        };
        let layer_id = document.layers[0].id.clone();
        let mut object = Object::new(
            layer_id,
            Shape::Rect {
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        object.style = Style {
            stroke: stroke.then(|| "#000000".to_string()),
            fill: fill.map(str::to_string),
            stroke_width_mm: 0.5,
        };
        document.add_object(object);
        document
    }

    #[test]
    fn filled_rectangle_covers_the_page() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (4, 4));
        assert_eq!(warnings.len(), 0);
        let index = (2 * 4 + 2) * 4;
        assert_eq!(&pixels[index..index + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn a_filled_shape_keeps_its_hole_empty() {
        let mut document = Document {
            width_mm: 10.0,
            height_mm: 10.0,
            ..Document::default()
        };
        let layer_id = document.layers[0].id.clone();
        let style = Style {
            stroke: None,
            fill: Some("#000000".to_string()),
            stroke_width_mm: 0.0,
        };
        let mut outer = Object::new(
            layer_id.clone(),
            Shape::Rect {
                width: 8.0,
                height: 8.0,
                corner_radius: 0.0,
            },
        );
        outer.transform = Transform2::translate(1.0, 1.0);
        outer.style = style.clone();
        document.add_object(outer);
        let outer_id = document.objects[0].id.clone();
        let mut hole = Object::new(
            layer_id,
            Shape::Rect {
                width: 4.0,
                height: 4.0,
                corner_radius: 0.0,
            },
        );
        hole.transform = Transform2::translate(3.0, 3.0);
        hole.style = style;
        hole.hole_of = Some(outer_id);
        document.add_object(hole);

        let options = PngOptions {
            dpi: 25.4,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (10, 10));
        assert_eq!(warnings.len(), 0);
        let pixel = |x: usize, y: usize| {
            let index = (y * width as usize + x) * 4;
            &pixels[index..index + 4]
        };
        assert_eq!(pixel(2, 5), &[0, 0, 0, 255], "the outer band is not filled");
        assert_eq!(pixel(5, 5), &[255, 255, 255, 255], "the hole was filled");
    }

    #[test]
    fn transparent_background_keeps_alpha_zero() {
        let document = document_with_rect(None, false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, ..) = render_rgba(&document, &options).unwrap();
        assert_eq!(pixels[3], 0);
    }

    #[test]
    fn explicit_dimensions_set_the_output_size() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: Some(20),
            height_px: Some(10),
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (_, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (20, 10));
        assert!(
            warnings.iter().any(|warning| warning.contains("stretched")),
            "{warnings:?}"
        );
    }

    #[test]
    fn rounding_the_output_size_is_not_a_stretched_picture() {
        // 126.0 x 95.7 mm at 96 dpi is 476.0 x 361.7 pixels; rounding the
        // height to 362 must not warn about a stretched picture.
        let document = Document {
            width_mm: 126.0,
            height_mm: 95.7,
            ..Document::default()
        };
        let options = PngOptions {
            dpi: 96.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (_, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (476, 362));
        assert!(
            !warnings.iter().any(|warning| warning.contains("stretched")),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_deliberate_stretch_still_warns() {
        let document = Document {
            width_mm: 126.0,
            height_mm: 95.7,
            ..Document::default()
        };
        let options = PngOptions {
            dpi: 96.0,
            transparent: false,
            width_px: Some(476),
            height_px: Some(400),
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (_, _, _, warnings) = render_rgba(&document, &options).unwrap();
        assert!(
            warnings.iter().any(|warning| warning.contains("stretched")),
            "{warnings:?}"
        );
    }

    #[test]
    fn one_explicit_side_keeps_the_aspect_ratio() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: Some(20),
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (_, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (20, 20));
        assert!(
            !warnings.iter().any(|warning| warning.contains("stretched")),
            "{warnings:?}"
        );
    }

    #[test]
    fn a_zero_explicit_size_is_rejected() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: Some(0),
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let error = render_rgba(&document, &options).unwrap_err();
        assert!(error.to_string().contains("at least 1 pixel"), "{error}");
    }

    #[test]
    fn a_margin_adds_blank_space_around_the_page() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 25.4,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 1.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (12, 12));
        assert_eq!(pixels[3], 0, "the corner margin is not blank");
        let inside = (width as usize + 1) * 4;
        assert_eq!(&pixels[inside..inside + 4], &[255, 0, 0, 255]);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn an_offset_moves_the_region() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 25.4,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: -2.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, width, _height, _warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!(pixels[3], 0, "the left edge should be blank");
        let first_red = (width as usize + 2) * 4;
        assert_eq!(&pixels[first_red..first_red + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn a_non_finite_offset_is_rejected() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: f64::NAN,
            offset_y_mm: 0.0,
            area: None,
        };
        let error = render_rgba(&document, &options).unwrap_err();
        assert!(error.to_string().contains("offset"), "{error}");
    }

    #[test]
    fn a_negative_margin_is_rejected() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: -1.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let error = render_rgba(&document, &options).unwrap_err();
        assert!(error.to_string().contains("margin"), "{error}");
    }

    #[test]
    fn hidden_layer_is_not_drawn() {
        let mut document = document_with_rect(Some("#000000"), false);
        document.layers[0].visible = false;
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, ..) = render_rgba(&document, &options).unwrap();
        assert_eq!(pixels[3], 0);
    }

    #[test]
    fn stroke_is_drawn_without_fill() {
        let document = document_with_rect(None, true);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, ..) = render_rgba(&document, &options).unwrap();
        // The corner pixel belongs to the stroke.
        assert_eq!(pixels[3], 255);
    }

    #[test]
    fn invalid_options_are_rejected() {
        let document = Document::default();
        let options = PngOptions {
            dpi: 0.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        assert!(render_rgba(&document, &options).is_err());
    }

    #[test]
    fn transform_is_applied() {
        let mut document = document_with_rect(Some("#ff0000"), false);
        document.objects[0].transform = Transform2::translate(5.0, 0.0);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: None,
        };
        let (pixels, width, ..) = render_rgba(&document, &options).unwrap();
        // The 10 mm rectangle shifted by 5 mm starts at about pixel 2.
        assert_eq!(pixels[(2 * width as usize) * 4 + 3], 0);
        assert_eq!(pixels[(2 * width as usize + 3) * 4 + 3], 255);
    }

    #[test]
    fn an_area_renders_only_that_region() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: Some(PngArea {
                x: 2.0,
                y: 2.0,
                width: 4.0,
                height: 4.0,
            }),
        };
        let (pixels, width, height, warnings) = render_rgba(&document, &options).unwrap();
        assert_eq!((width, height), (2, 2));
        let _ = warnings;
        assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn an_area_uses_the_region_origin_for_geometry() {
        let mut document = document_with_rect(Some("#ff0000"), false);
        document.objects[0].shape = Shape::Rect {
            width: 2.0,
            height: 2.0,
            corner_radius: 0.0,
        };
        document.objects[0].transform = Transform2::translate(5.0, 5.0);
        let inside = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: Some(PngArea {
                x: 5.0,
                y: 5.0,
                width: 3.0,
                height: 3.0,
            }),
        };
        let (pixels, ..) = render_rgba(&document, &inside).unwrap();
        assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
        let outside = PngOptions {
            dpi: 10.0,
            transparent: true,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: Some(PngArea {
                x: 0.0,
                y: 0.0,
                width: 3.0,
                height: 3.0,
            }),
        };
        let (pixels, ..) = render_rgba(&document, &outside).unwrap();
        assert_eq!(pixels[3], 0, "the region included a rectangle outside it");
    }

    #[test]
    fn an_area_outside_the_page_is_refused() {
        let document = document_with_rect(Some("#ff0000"), false);
        let options = PngOptions {
            dpi: 10.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: Some(PngArea {
                x: 8.0,
                y: 8.0,
                width: 4.0,
                height: 4.0,
            }),
        };
        let error = render_rgba(&document, &options).unwrap_err();
        assert!(error.to_string().contains("inside the page"), "{error}");
        let empty = PngOptions {
            dpi: 10.0,
            transparent: false,
            width_px: None,
            height_px: None,
            margin_mm: 0.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            area: Some(PngArea {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 4.0,
            }),
        };
        assert!(render_rgba(&document, &empty).is_err());
    }

    #[test]
    fn png_rendering_can_be_cancelled() {
        let document = document_with_rect(Some("#ff0000"), false);
        assert!(render_rgba_cancellable(&document, &PngOptions::default(), &|| false).is_ok());
        assert!(render_rgba_cancellable(&document, &PngOptions::default(), &|| true).is_err());
        let path = std::env::temp_dir().join("bw_cancelled.png");
        let _ = std::fs::remove_file(&path);
        let error =
            export_png_cancellable(&document, &path, &PngOptions::default(), &|| true).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert!(!path.exists());
    }

    #[test]
    fn refuses_a_canvas_that_is_too_large() {
        let document = Document {
            width_mm: 200_000.0,
            height_mm: 200_000.0,
            ..Document::default()
        };
        let error = render_rgba(&document, &PngOptions::default()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("pixels"), "{message}");
        assert!(message.contains("limit is"), "{message}");

        let document = Document {
            width_mm: 1e12,
            height_mm: 1e12,
            ..Document::default()
        };
        let error = render_rgba(&document, &PngOptions::default()).unwrap_err();
        assert!(error.to_string().contains("too large"), "{error}");
    }

    #[test]
    fn text_is_outlined_in_the_png() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Text(crate::objects::TextObject {
                content: "hi".to_string(),
                font: "sans".to_string(),
                size: 8.0,
                x: 10.0,
                y: 20.0,
                ..crate::objects::TextObject::default()
            }),
        ));
        let (pixels, _, _, warnings) = render_rgba(&document, &PngOptions::default()).unwrap();
        assert_eq!(warnings.len(), 0);
        let ink = pixels
            .chunks_exact(4)
            .filter(|pixel| pixel[0] < 250 || pixel[1] < 250 || pixel[2] < 250)
            .count();
        assert!(ink > 0, "the text left no pixels in the PNG");
    }

    #[test]
    fn rendered_images_save_as_bmp_tiff_and_webp() {
        let document = document_with_rect(Some("#00ff00"), false);
        let (data, width, height, _) = render_rgba(&document, &PngOptions::default()).unwrap();
        let picture = image::RgbaImage::from_raw(width, height, data).unwrap();
        for extension in ["png", "bmp", "tif", "webp"] {
            let path = std::env::temp_dir().join(format!("beamburin_image_export.{extension}"));
            let _ = std::fs::remove_file(&path);
            picture.save(&path).unwrap();
            let decoded = image::open(&path).unwrap();
            assert_eq!((decoded.width(), decoded.height()), (width, height));
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn writes_a_readable_png() {
        let document = document_with_rect(Some("#00ff00"), false);
        let path = std::env::temp_dir().join("beamburin_export_test.png");
        let _ = std::fs::remove_file(&path);
        let report = export_png(&document, &path, &PngOptions::default()).unwrap();
        assert!(report.width_px > 0);
        let decoded = image::open(&path).unwrap();
        assert_eq!(decoded.width(), report.width_px);
        assert!(!path.with_extension("png.tmp").exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_a4_page_exports_at_the_requested_dpi() {
        let document = Document {
            width_mm: 210.0,
            height_mm: 297.0,
            ..Document::default()
        };
        for (dpi, expected_width, expected_height) in [(96.0, 794u32, 1123u32), (300.0, 2480, 3508)]
        {
            let options = PngOptions {
                dpi,
                transparent: false,
                width_px: None,
                height_px: None,
                margin_mm: 0.0,
                offset_x_mm: 0.0,
                offset_y_mm: 0.0,
                area: None,
            };
            let (_, width, height, _) = render_rgba(&document, &options).unwrap();
            assert_eq!(
                (width, height),
                (expected_width, expected_height),
                "at {dpi} dpi"
            );
        }
    }
}
