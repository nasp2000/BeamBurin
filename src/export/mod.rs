pub mod dxf;
pub mod eps;
pub mod hpgl;
pub mod paths;
pub mod pdf;
pub mod png;
pub mod svg;

pub use dxf::{DxfReport, export_dxf, export_dxf_with_report};
pub use eps::export_eps;
pub use hpgl::export_hpgl;
pub use pdf::export_pdf;
pub use png::{
    Area, PngArea, PngOptions, PngReport, export_png, export_png_cancellable, render_rgba,
    render_rgba_cancellable,
};
pub use svg::{SvgOptions, export_svg, export_svg_with_options};

use crate::document::{Document, Shape};
use crate::geometry::path::Segment;

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

/// Describe the elements the EPS, PDF and HPGL exporters leave out.
///
/// They share the same subset: visible layers, path geometry only, curves
/// flattened. Text and images are named so an export cannot look complete when
/// it is not.
pub fn vector_notes(document: &Document) -> VectorNotes {
    let mut notes = VectorNotes::default();
    for layer in document.layers.iter().filter(|layer| layer.visible) {
        for object in document
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id && object.visible)
        {
            match &object.shape {
                Shape::Text(_) => {
                    notes.text_omitted += 1;
                    notes.warnings.push(format!(
                        "text '{}' was omitted; convert it to paths first",
                        object.name
                    ));
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
                .map(|layer| crate::document::Layer {
                    visible: false,
                    ..layer.clone()
                })
                .collect(),
            ..document.clone()
        };
        assert_eq!(vector_notes(&hidden), VectorNotes::default());
    }
}
