pub mod dxf;
pub mod eps;
pub mod hpgl;
pub mod paths;
pub mod pdf;
pub mod png;
pub mod svg;

pub use dxf::{
    DxfOptions, DxfReport, DxfUnit, export_dxf, export_dxf_with_options, export_dxf_with_report,
    export_dxf_with_report_options,
};
pub use eps::{EpsOptions, export_eps, export_eps_with_options};
pub use hpgl::{HpglOptions, export_hpgl, export_hpgl_with_options};
pub use pdf::{PdfOptions, export_pdf, export_pdf_with_options};
pub use png::{
    Area, PngArea, PngOptions, PngReport, export_png, export_png_cancellable, render_rgba,
    render_rgba_cancellable,
};
pub use svg::{SvgOptions, export_svg, export_svg_with_options};

use crate::document::{Document, Object, Shape};
use crate::errors::{BeamError, Result};
use crate::geometry::Path;
use crate::geometry::path::Segment;

/// Validate an optional export region and return the rectangle to write.
///
/// `None` means the whole page. A region must be finite, positive and inside
/// the document; `label` names the format in the error message.
pub(crate) fn export_area(document: &Document, area: Option<Area>, label: &str) -> Result<Area> {
    let area = area.unwrap_or(Area {
        x: 0.0,
        y: 0.0,
        width: document.width_mm,
        height: document.height_mm,
    });
    let finite = area.x.is_finite()
        && area.y.is_finite()
        && area.width.is_finite()
        && area.height.is_finite();
    if !finite || area.width <= 0.0 || area.height <= 0.0 {
        return Err(BeamError::InvalidInput(format!(
            "the {label} area must have a positive finite size"
        )));
    }
    let epsilon = 1e-6;
    if area.x < -epsilon
        || area.y < -epsilon
        || area.x + area.width > document.width_mm + epsilon
        || area.y + area.height > document.height_mm + epsilon
    {
        return Err(BeamError::InvalidInput(format!(
            "the {label} area must lie inside the page"
        )));
    }
    Ok(area)
}

/// Add a margin around an export region, limited to the page.
///
/// Vector exports use the region as their frame: the margin becomes blank space
/// on every side. A region that already lies outside the page is returned
/// unchanged, so the exporter still reports its own error instead of silently
/// framing different content. A non-finite or non-positive margin is ignored.
pub fn expand_area(area: Area, margin_mm: f64, page_width_mm: f64, page_height_mm: f64) -> Area {
    if !margin_mm.is_finite() || margin_mm <= 0.0 {
        return area;
    }
    let epsilon = 1e-6;
    let inside = area.x >= -epsilon
        && area.y >= -epsilon
        && area.x + area.width <= page_width_mm + epsilon
        && area.y + area.height <= page_height_mm + epsilon;
    if !inside {
        return area;
    }
    let min_x = (area.x - margin_mm).max(0.0);
    let min_y = (area.y - margin_mm).max(0.0);
    let max_x = (area.x + area.width + margin_mm).min(page_width_mm);
    let max_y = (area.y + area.height + margin_mm).min(page_height_mm);
    Area {
        x: min_x,
        y: min_y,
        width: (max_x - min_x).max(0.01),
        height: (max_y - min_y).max(0.01),
    }
}

/// What the path-only vector exporters (EPS, PDF, HPGL) cannot represent.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VectorNotes {
    /// Visible text objects that are not written.
    pub text_omitted: usize,
    /// Visible images that are not written.
    pub images_omitted: usize,
    /// Visible paths whose curves become straight segments.
    pub curves_flattened: usize,
    /// Ready-to-show notes for the export report.
    pub warnings: Vec<String>,
}

/// Whether a layer passes the optional export layer filter.
pub(crate) fn layer_included(layer_id: &str, ids: Option<&[String]>) -> bool {
    ids.is_none_or(|ids| ids.iter().any(|id| id == layer_id))
}

/// Paths to write for one object in a path-only vector exporter.
///
/// Text is outlined when `text_as_paths` is set. Text that cannot be outlined
/// and images return `None`; the caller skips them and [`vector_notes`] says
/// why.
pub(crate) fn object_paths(object: &Object, text_as_paths: bool) -> Option<Vec<Path>> {
    match &object.shape {
        Shape::Text(text) if text_as_paths => {
            crate::objects::text_to_paths(text).ok().map(|paths| {
                paths
                    .iter()
                    .map(|path| path.transform(&object.transform))
                    .collect()
            })
        }
        Shape::Text(_) | Shape::Image(_) => None,
        _ => Some(vec![object.world_path()]),
    }
}

/// Describe the elements the EPS, PDF and HPGL exporters leave out.
///
/// They share the same subset: visible layers, path geometry only, curves
/// flattened. Text and images are named so an export cannot look complete when
/// it is not. When `text_as_paths` is set, text is only reported if outlining
/// fails.
pub fn vector_notes(document: &Document) -> VectorNotes {
    vector_notes_with_text(document, false)
}

/// Describe omitted elements, taking the text-as-paths setting into account.
pub fn vector_notes_with_text(document: &Document, text_as_paths: bool) -> VectorNotes {
    let mut notes = VectorNotes::default();
    for layer in document.layers.iter().filter(|layer| layer.visible) {
        for object in document
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id && object.visible)
        {
            match &object.shape {
                Shape::Text(text) => {
                    let outlined = text_as_paths && crate::objects::text_to_paths(text).is_ok();
                    if !outlined {
                        notes.text_omitted += 1;
                        if text_as_paths {
                            notes.warnings.push(format!(
                                "text '{}' could not be converted to paths",
                                object.name
                            ));
                        } else {
                            notes.warnings.push(format!(
                                "text '{}' was omitted; enable text as paths first",
                                object.name
                            ));
                        }
                    }
                }
                Shape::Image(_) => {
                    notes.images_omitted += 1;
                    notes.warnings.push(format!(
                        "image '{}' was omitted; this format carries vector geometry only",
                        object.name
                    ));
                }
                _ => {
                    if object
                        .world_path()
                        .segments
                        .iter()
                        .any(|segment| matches!(segment, Segment::Cubic(_)))
                    {
                        notes.curves_flattened += 1;
                    }
                }
            }
        }
    }
    if notes.curves_flattened > 0 {
        notes.warnings.push(format!(
            "{} path(s) contain curves; the export uses straight segments",
            notes.curves_flattened
        ));
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_margin_expands_the_region_on_every_side() {
        let area = Area {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
        };
        let framed = expand_area(area, 5.0, 200.0, 100.0);
        assert!((framed.x - 5.0).abs() < 1e-9);
        assert!((framed.y - 15.0).abs() < 1e-9);
        assert!((framed.width - 40.0).abs() < 1e-9);
        assert!((framed.height - 50.0).abs() < 1e-9);
    }

    #[test]
    fn a_margin_stops_at_the_page_edge() {
        let area = Area {
            x: 0.0,
            y: 0.0,
            width: 190.0,
            height: 95.0,
        };
        let framed = expand_area(area, 20.0, 200.0, 100.0);
        assert!((framed.x).abs() < 1e-9);
        assert!((framed.y).abs() < 1e-9);
        assert!((framed.width - 200.0).abs() < 1e-9);
        assert!((framed.height - 100.0).abs() < 1e-9);
    }

    #[test]
    fn a_margin_keeps_an_outside_region_for_the_exporter_to_reject() {
        let area = Area {
            x: 180.0,
            y: 0.0,
            width: 40.0,
            height: 40.0,
        };
        let framed = expand_area(area, 5.0, 200.0, 100.0);
        assert!((framed.x - area.x).abs() < 1e-9);
        assert!((framed.width - area.width).abs() < 1e-9);
        assert_eq!(expand_area(area, 0.0, 200.0, 100.0), Area { ..area });
        assert_eq!(expand_area(area, f64::NAN, 200.0, 100.0), Area { ..area });
    }

    #[test]
    fn vector_notes_name_text_images_and_curves() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(crate::document::Object::new(
            layer_id.clone(),
            Shape::Text(crate::objects::TextObject {
                content: "hi".to_string(),
                ..crate::objects::TextObject::default()
            }),
        ));
        document.add_object(crate::document::Object::new(
            layer_id.clone(),
            Shape::Image(crate::objects::ImageObject {
                path: "missing.png".to_string(),
                width_px: 1,
                height_px: 1,
                dpi: 96.0,
                settings: crate::objects::ImageSettings::default(),
                crop: None,
                mask_colours: Vec::new(),
                mask_tolerance: 24,
                mask_paths: Vec::new(),
                mask_invert: false,
                mask_alpha: false,
                mask_alpha_threshold: 128,
            }),
        ));
        document.add_object(crate::document::Object::new(
            layer_id.clone(),
            Shape::Ellipse { rx: 2.0, ry: 1.0 },
        ));
        let notes = vector_notes(&document);
        assert_eq!(notes.text_omitted, 1);
        assert_eq!(notes.images_omitted, 1);
        assert_eq!(notes.curves_flattened, 1);
        assert!(
            notes
                .warnings
                .iter()
                .any(|warning| warning.contains("text 'Text'"))
        );
        assert!(
            notes
                .warnings
                .iter()
                .any(|warning| warning.contains("vector geometry only"))
        );
        assert!(
            notes
                .warnings
                .iter()
                .any(|warning| warning.contains("straight segments"))
        );
        let hidden = Document {
            layers: document
                .layers
                .iter()
                .map(|layer| {
                    let mut hidden = layer.clone();
                    hidden.visible = false;
                    hidden
                })
                .collect(),
            ..document.clone()
        };
        assert_eq!(vector_notes(&hidden), VectorNotes::default());
        let outlined = vector_notes_with_text(&document, true);
        assert_eq!(outlined.text_omitted, 0);
        assert_eq!(outlined.images_omitted, 1);
        assert!(
            outlined
                .warnings
                .iter()
                .all(|warning| !warning.contains("text 'Text'"))
        );
    }
}
