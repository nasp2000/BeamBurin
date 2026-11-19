//! Print output: paper sizes, scaling, the monochrome raster and a one-page
//! PDF that embeds it.
//!
//! Printing never changes the document: the raster and the PDF are external
//! files. Text is rasterised through the PNG renderer, so it keeps its look;
//! registration marks stay printed and are skipped by the cut job.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::document::Document;
use crate::errors::{BeamError, Result};
use crate::export::paths::fmt_num;
use crate::export::png::{Area, PngOptions, render_rgba};

/// Layer that holds the registration marks.
pub const MARKS_LAYER_NAME: &str = "Registration marks";

/// Millimetres per PDF point (1 pt = 1/72 inch).
const MM_PER_POINT: f64 = 25.4 / 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintColour {
    BlackOnly,
    KeepColours,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paper {
    A4,
    Letter,
    A3,
    Legal,
    Custom { width_mm: f64, height_mm: f64 },
}

impl Paper {
    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::Letter => "Letter",
            Paper::A3 => "A3",
            Paper::Legal => "Legal",
            Paper::Custom { .. } => "Custom",
        }
    }

    /// Portrait page size in millimetres.
    pub fn size_mm(self) -> Result<(f64, f64)> {
        let (width, height) = match self {
            Paper::A4 => (210.0, 297.0),
            Paper::Letter => (215.9, 279.4),
            Paper::A3 => (297.0, 420.0),
            Paper::Legal => (215.9, 355.6),
            Paper::Custom {
                width_mm,
                height_mm,
            } => {
                if !width_mm.is_finite()
                    || !height_mm.is_finite()
                    || !(10.0..=2000.0).contains(&width_mm)
                    || !(10.0..=2000.0).contains(&height_mm)
                {
                    return Err(BeamError::InvalidInput(
                        "custom paper must be between 10 and 2000 mm on each side".to_string(),
                    ));
                }
                (width_mm, height_mm)
            }
        };
        Ok((width, height))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrintScale {
    Percent(f64),
    Fit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrintOptions {
    pub colour: PrintColour,
    /// Document rectangle to print; `None` prints the whole page.
    pub area: Option<Area>,
    pub paper: Paper,
    pub landscape: bool,
    pub scale: PrintScale,
    /// Top, right, bottom and left margins, in millimetres.
    pub margins_mm: [f64; 4],
    pub dpi: u32,
    pub include_marks: bool,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            colour: PrintColour::BlackOnly,
            area: None,
            paper: Paper::A4,
            landscape: false,
            scale: PrintScale::Percent(100.0),
            margins_mm: [5.0; 4],
            dpi: 300,
            include_marks: true,
        }
    }
}

impl PrintOptions {
    pub fn validate(&self) -> Result<()> {
        self.paper.size_mm()?;
        if !matches!(self.dpi, 150 | 300 | 600) {
            return Err(BeamError::InvalidInput(
                "print resolution must be 150, 300 or 600 dpi".to_string(),
            ));
        }
        for margin in self.margins_mm {
            if !margin.is_finite() || !(0.0..=50.0).contains(&margin) {
                return Err(BeamError::InvalidInput(
                    "print margins must be between 0 and 50 mm".to_string(),
                ));
            }
        }
        if let PrintScale::Percent(percent) = self.scale
            && (!percent.is_finite() || !(1.0..=1000.0).contains(&percent))
        {
            return Err(BeamError::InvalidInput(
                "print scale must be between 1 and 1000 percent".to_string(),
            ));
        }
        if let Some(area) = self.area {
            let finite = area.x.is_finite()
                && area.y.is_finite()
                && area.width.is_finite()
                && area.height.is_finite();
            if !finite || area.width <= 0.0 || area.height <= 0.0 {
                return Err(BeamError::InvalidInput(
                    "the print area must have a positive finite size".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Page size in millimetres after applying the orientation.
    pub fn page_mm(&self) -> Result<(f64, f64)> {
        let (width, height) = self.paper.size_mm()?;
        Ok(if self.landscape {
            (height, width)
        } else {
            (width, height)
        })
    }
}

/// One rendered page ready to embed in a PDF.
#[derive(Debug, Clone)]
pub struct PrintRaster {
    /// RGBA pixels, top row first.
    pub pixels: Vec<u8>,
    pub width_px: u32,
    pub height_px: u32,
    /// Image rectangle on the paper, in millimetres from the bottom left.
    pub placement: Area,
    pub dpi: u32,
    pub scale: f64,
    pub warnings: Vec<String>,
}

/// Render the chosen area at the print resolution.
#[allow(clippy::too_many_lines)]
pub fn render_print_raster(document: &Document, options: &PrintOptions) -> Result<PrintRaster> {
    options.validate()?;
    let (page_width, page_height) = options.page_mm()?;
    let [top, right, bottom, left] = options.margins_mm;
    let printable_width = page_width - left - right;
    let printable_height = page_height - top - bottom;
    if printable_width <= 1.0 || printable_height <= 1.0 {
        return Err(BeamError::InvalidInput(
            "the margins leave no printable area".to_string(),
        ));
    }
    let content = options.area.unwrap_or(Area {
        x: 0.0,
        y: 0.0,
        width: document.width_mm,
        height: document.height_mm,
    });
    if content.width <= 0.0 || content.height <= 0.0 {
        return Err(BeamError::InvalidInput(
            "the print area has no content".to_string(),
        ));
    }
    let scale = match options.scale {
        PrintScale::Percent(percent) => percent / 100.0,
        PrintScale::Fit => (printable_width / content.width)
            .min(printable_height / content.height)
            .min(1.0),
    };
    let placed_width = content.width * scale;
    let placed_height = content.height * scale;
    if placed_width > printable_width + 1e-6 || placed_height > printable_height + 1e-6 {
        return Err(BeamError::InvalidInput(format!(
            "The job needs {placed_width:.1} × {placed_height:.1} mm; {} printable is \
             {printable_width:.1} × {printable_height:.1} mm",
            options.paper.label()
        )));
    }
    let pixel_width = (placed_width / 25.4 * f64::from(options.dpi))
        .round()
        .max(1.0);
    let pixel_height = (placed_height / 25.4 * f64::from(options.dpi))
        .round()
        .max(1.0);
    let pixel_width = u32::try_from(pixel_width as u64)
        .map_err(|_| BeamError::InvalidInput("the print raster is too large".to_string()))?;
    let pixel_height = u32::try_from(pixel_height as u64)
        .map_err(|_| BeamError::InvalidInput("the print raster is too large".to_string()))?;

    let mut source = document.clone();
    if !options.include_marks {
        for layer in &mut source.layers {
            if layer.name == MARKS_LAYER_NAME {
                layer.visible = false;
            }
        }
    }
    let png = PngOptions {
        allow_outside_page: false,
        monochrome: options.colour == PrintColour::BlackOnly,
        dpi: f64::from(options.dpi),
        transparent: false,
        area: Some(content),
        width_px: Some(pixel_width),
        height_px: Some(pixel_height),
        margin_mm: 0.0,
        offset_x_mm: 0.0,
        offset_y_mm: 0.0,
    };
    let (pixels, width_px, height_px, warnings) = render_rgba(&source, &png)?;
    Ok(PrintRaster {
        pixels,
        width_px,
        height_px,
        placement: Area {
            x: left + (printable_width - placed_width) / 2.0,
            y: bottom + (printable_height - placed_height) / 2.0,
            width: placed_width,
            height: placed_height,
        },
        dpi: options.dpi,
        scale,
        warnings,
    })
}

/// Write the raster on a one-page PDF whose page is the chosen paper.
pub fn write_print_pdf(raster: &PrintRaster, options: &PrintOptions, path: &Path) -> Result<()> {
    let (page_width, page_height) = options.page_mm()?;
    let page_width = page_width / MM_PER_POINT;
    let page_height = page_height / MM_PER_POINT;
    let image_width = raster.placement.width / MM_PER_POINT;
    let image_height = raster.placement.height / MM_PER_POINT;
    let image_x = raster.placement.x / MM_PER_POINT;
    let image_y = raster.placement.y / MM_PER_POINT;

    let rgb = rgba_to_rgb(&raster.pixels);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&rgb).map_err(|error| {
        BeamError::InvalidInput(format!("cannot compress the print page: {error}"))
    })?;
    let compressed = encoder.finish().map_err(|error| {
        BeamError::InvalidInput(format!("cannot compress the print page: {error}"))
    })?;

    let content = format!(
        "q\n{} 0 0 {} {} {} cm\n/Im0 Do\nQ\n",
        fmt_num(image_width),
        fmt_num(image_height),
        fmt_num(image_x),
        fmt_num(image_y)
    );
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /XObject << \
             /Im0 5 0 R >> >> /Contents 4 0 R >>",
            fmt_num(page_width),
            fmt_num(page_height)
        )
        .into_bytes(),
        format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        )
        .into_bytes(),
        image_object(&compressed, raster.width_px, raster.height_px),
    ];

    let pdf = assemble_pdf(&objects);
    write_atomically(path, &pdf)
}

fn image_object(compressed: &[u8], width_px: u32, height_px: u32) -> Vec<u8> {
    let mut body = format!(
        "<< /Type /XObject /Subtype /Image /Width {width_px} /Height {height_px} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\n\
         stream\n",
        compressed.len()
    )
    .into_bytes();
    body.extend_from_slice(compressed);
    body.extend_from_slice(b"\nendstream");
    body
}

fn assemble_pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn rgba_to_rgb(pixels: &[u8]) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(pixels.len() / 4 * 3);
    for pixel in pixels.chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
    }
    rgb
}

fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension("pdf.tmp");
    if let Err(error) = std::fs::write(&temporary, bytes) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

/// Open a generated print file with the system handler.
///
/// The file stays on disk whether or not the viewer starts, so a failure is
/// reported without losing the print.
pub fn open_print_file(path: &Path) -> Result<()> {
    let mut command = system_open_command(path);
    match command.spawn() {
        Ok(_) => Ok(()),
        Err(error) => Err(BeamError::InvalidInput(format!(
            "Saved to {}; open it to print ({error})",
            path.display()
        ))),
    }
}

fn system_open_command(path: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    }
    #[cfg(target_os = "linux")]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        let mut command = Command::new("open");
        command.arg(path);
        command
    }
}

/// Default file name for a print PDF beside the project.
pub fn default_print_path(document: &Document, directory: &Path) -> PathBuf {
    let mut name = document.name.trim();
    if name.is_empty() {
        name = "print";
    }
    directory.join(format!("{name}-print.pdf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;
    use crate::document::model::{Object, Shape};

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
            stroke: stroke.then(|| "#ff0000".to_string()),
            fill: fill.map(str::to_string),
            stroke_width_mm: 0.5,
        };
        document.add_object(object);
        document
    }

    fn options() -> PrintOptions {
        PrintOptions {
            paper: Paper::Custom {
                width_mm: 100.0,
                height_mm: 100.0,
            },
            margins_mm: [0.0; 4],
            ..PrintOptions::default()
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("beamburin_print_{}_{name}", std::process::id()))
    }

    fn centre_pixel(raster: &PrintRaster) -> [u8; 4] {
        let x = raster.width_px / 2;
        let y = raster.height_px / 2;
        let index = ((y * raster.width_px + x) * 4) as usize;
        [
            raster.pixels[index],
            raster.pixels[index + 1],
            raster.pixels[index + 2],
            raster.pixels[index + 3],
        ]
    }

    #[test]
    fn a4_portrait_page_size_in_points() {
        let options = PrintOptions::default();
        let (width, height) = options.page_mm().unwrap();
        assert!((width - 210.0).abs() < 1e-9);
        assert!((height - 297.0).abs() < 1e-9);
        assert!((width / MM_PER_POINT - 595.275_590_551).abs() < 1e-6);
        assert!((height / MM_PER_POINT - 841.889_763_780).abs() < 1e-6);
    }

    #[test]
    fn landscape_swaps_width_and_height() {
        let options = PrintOptions {
            landscape: true,
            ..PrintOptions::default()
        };
        let (width, height) = options.page_mm().unwrap();
        assert!((width - 297.0).abs() < 1e-9);
        assert!((height - 210.0).abs() < 1e-9);
    }

    #[test]
    fn custom_paper_is_validated() {
        let ok = Paper::Custom {
            width_mm: 100.0,
            height_mm: 200.0,
        };
        let (width, height) = ok.size_mm().unwrap();
        assert!((width - 100.0).abs() < 1e-9);
        assert!((height - 200.0).abs() < 1e-9);
        let bad = Paper::Custom {
            width_mm: 0.0,
            height_mm: 200.0,
        };
        assert!(bad.size_mm().is_err());
    }

    #[test]
    fn print_raster_at_300_dpi_has_the_right_pixel_size() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(&document, &options()).unwrap();
        assert_eq!((raster.width_px, raster.height_px), (118, 118));
        assert!((raster.scale - 1.0).abs() < 1e-9);
    }

    #[test]
    fn print_raster_600_dpi_doubles_the_pixels() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(
            &document,
            &PrintOptions {
                dpi: 600,
                ..options()
            },
        )
        .unwrap();
        assert_eq!((raster.width_px, raster.height_px), (236, 236));
    }

    #[test]
    fn black_only_renders_vectors_black() {
        let document = document_with_rect(Some("#00ff00"), true);
        let raster = render_print_raster(&document, &options()).unwrap();
        let mut found_ink = false;
        for pixel in raster.pixels.chunks_exact(4) {
            if pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255 {
                found_ink = true;
                assert_eq!(&pixel[..3], &[0, 0, 0]);
            }
        }
        assert!(found_ink);
    }

    #[test]
    fn keep_colours_preserves_the_fill_colour() {
        let document = document_with_rect(Some("#00ff00"), true);
        let raster = render_print_raster(
            &document,
            &PrintOptions {
                colour: PrintColour::KeepColours,
                ..options()
            },
        )
        .unwrap();
        assert_eq!(centre_pixel(&raster), [0, 255, 0, 255]);
    }

    #[test]
    fn black_only_converts_images_to_grayscale() {
        let path = temp_path("red.png");
        let image = image::RgbImage::from_pixel(10, 10, image::Rgb([255, 0, 0]));
        image.save(&path).unwrap();
        let mut document = Document {
            width_mm: 10.0,
            height_mm: 10.0,
            ..Document::default()
        };
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Image(crate::objects::ImageObject::new(
                path.to_string_lossy(),
                10,
                10,
                25.4,
            )),
        ));
        let raster = render_print_raster(&document, &options()).unwrap();
        let pixel = centre_pixel(&raster);
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
        assert!(pixel[0] < 128);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn hidden_layers_are_not_printed() {
        let mut document = document_with_rect(Some("#000000"), false);
        document.layers[0].visible = false;
        let raster = render_print_raster(&document, &options()).unwrap();
        assert!(
            raster
                .pixels
                .chunks_exact(4)
                .all(|pixel| pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255)
        );
    }

    #[test]
    fn registration_marks_can_be_left_out() {
        let mut document = document_with_rect(None, false);
        let marks_id = document.add_layer(MARKS_LAYER_NAME).id.clone();
        let mut mark = Object::new(
            marks_id,
            Shape::Path(crate::geometry::Path::from_points(
                &[(0.0, 0.0), (10.0, 10.0)],
                false,
            )),
        );
        mark.style.stroke = Some("#000000".to_string());
        document.add_object(mark);
        let with_marks = render_print_raster(&document, &options()).unwrap();
        assert!(
            with_marks
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel[..3] != [255, 255, 255])
        );
        let without_marks = render_print_raster(
            &document,
            &PrintOptions {
                include_marks: false,
                ..options()
            },
        )
        .unwrap();
        assert!(
            without_marks
                .pixels
                .chunks_exact(4)
                .all(|pixel| pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255)
        );
    }

    #[test]
    fn percent_scale_changes_the_raster_size_and_placement() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(
            &document,
            &PrintOptions {
                scale: PrintScale::Percent(50.0),
                ..options()
            },
        )
        .unwrap();
        assert_eq!((raster.width_px, raster.height_px), (59, 59));
        assert!((raster.placement.width - 5.0).abs() < 1e-9);
        assert!((raster.placement.x - 47.5).abs() < 1e-9);
    }

    #[test]
    fn fit_only_scales_down() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(
            &document,
            &PrintOptions {
                scale: PrintScale::Fit,
                ..PrintOptions::default()
            },
        )
        .unwrap();
        assert!((raster.scale - 1.0).abs() < 1e-9);
        assert!((raster.placement.width - 10.0).abs() < 1e-9);
    }

    #[test]
    fn margins_centre_the_content_on_the_printable_area() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(
            &document,
            &PrintOptions {
                margins_mm: [10.0, 20.0, 30.0, 40.0],
                ..PrintOptions::default()
            },
        )
        .unwrap();
        assert!((raster.placement.x - 110.0).abs() < 1e-9);
        assert!((raster.placement.y - 153.5).abs() < 1e-9);
    }

    #[test]
    fn paper_too_small_is_an_error_with_measurements() {
        let mut document = Document {
            width_mm: 20.0,
            height_mm: 20.0,
            ..Document::default()
        };
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Rect {
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        ));
        let error = render_print_raster(
            &document,
            &PrintOptions {
                paper: Paper::Custom {
                    width_mm: 5.0,
                    height_mm: 5.0,
                },
                ..options()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("custom paper must be"));
        let error = render_print_raster(
            &document,
            &PrintOptions {
                paper: Paper::Custom {
                    width_mm: 10.0,
                    height_mm: 10.0,
                },
                ..options()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("The job needs"));
    }

    #[test]
    fn huge_print_areas_are_refused() {
        let mut document = Document {
            width_mm: 500.0,
            height_mm: 500.0,
            ..Document::default()
        };
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Rect {
                width: 500.0,
                height: 500.0,
                corner_radius: 0.0,
            },
        ));
        let error = render_print_raster(
            &document,
            &PrintOptions {
                paper: Paper::Custom {
                    width_mm: 2000.0,
                    height_mm: 2000.0,
                },
                dpi: 600,
                ..options()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("limit"));
    }

    #[test]
    fn invalid_dpi_and_margins_are_refused() {
        let document = document_with_rect(None, true);
        assert!(
            render_print_raster(
                &document,
                &PrintOptions {
                    dpi: 200,
                    ..options()
                }
            )
            .is_err()
        );
        assert!(
            render_print_raster(
                &document,
                &PrintOptions {
                    margins_mm: [-1.0, 0.0, 0.0, 0.0],
                    ..PrintOptions::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn printing_does_not_change_the_document() {
        let document = document_with_rect(None, true);
        let before = serde_json::to_string(&document).unwrap();
        let _ = render_print_raster(&document, &options()).unwrap();
        assert_eq!(serde_json::to_string(&document).unwrap(), before);
    }

    #[test]
    fn pdf_has_one_page_and_an_embedded_image() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(&document, &options()).unwrap();
        let path = temp_path("one_page.pdf");
        write_print_pdf(&raster, &options(), &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4\n"));
        assert!(text.ends_with("%%EOF\n"));
        assert!(text.contains("/Count 1"));
        assert!(text.contains("/Subtype /Image"));
        assert!(text.contains("/Filter /FlateDecode"));
        assert!(text.contains("/Width 118 /Height 118"));
        assert!(!text.contains("/Font"));
        assert!(text.contains(&format!(
            "/MediaBox [0 0 {} {}]",
            fmt_num(100.0 / MM_PER_POINT),
            fmt_num(100.0 / MM_PER_POINT)
        )));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a4_pdf_page_matches_the_paper() {
        let document = document_with_rect(None, true);
        let options = PrintOptions::default();
        let raster = render_print_raster(&document, &options).unwrap();
        let path = temp_path("a4.pdf");
        write_print_pdf(&raster, &options, &path).unwrap();
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).to_string();
        assert!(
            text.contains("/MediaBox [0 0 595.275591 841.889764]"),
            "{text}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn saved_pdf_leaves_no_temporary_file() {
        let document = document_with_rect(None, true);
        let raster = render_print_raster(&document, &options()).unwrap();
        let path = temp_path("atomic.pdf");
        write_print_pdf(&raster, &options(), &path).unwrap();
        assert!(path.is_file());
        assert!(!path.with_extension("pdf.tmp").exists());
        let _ = std::fs::remove_file(path);
    }
}
