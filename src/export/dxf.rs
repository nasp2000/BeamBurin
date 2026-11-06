use std::fmt::Write as _;

use crate::document::{Document, Shape};
use crate::errors::Result;
use crate::geometry::path::Segment;

use super::paths::{flatten, fmt_num};

/// What the DXF export wrote and what it had to leave out (§14.2).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DxfReport {
    /// Polylines written.
    pub entities: usize,
    /// Text objects that were not written.
    pub text_omitted: usize,
    /// Images that were not written.
    pub images_omitted: usize,
    /// Paths that contained curves and were flattened.
    pub curves_flattened: usize,
    /// Filled paths written as outlines only.
    pub fills_outlined: usize,
    /// Human-readable notes for the export report.
    pub warnings: Vec<String>,
}

/// Export the document as DXF, aiming at the widely supported R12 `ENTITIES`
/// subset.
///
/// Each visible object becomes one `LWPOLYLINE` with the transform baked into the
/// vertices. Bézier segments are flattened. Layers are written as entity layer
/// names and colours; no layer table is emitted.
pub fn export_dxf(document: &Document) -> Result<String> {
    export_dxf_with_report(document).map(|(text, _)| text)
}

/// Export the document and describe what the subset could not represent.
pub fn export_dxf_with_report(document: &Document) -> Result<(String, DxfReport)> {
    let mut out = String::new();
    let mut report = DxfReport::default();
    out.push_str("0\nSECTION\n2\nENTITIES\n");
    let mut layers: Vec<_> = document.layers.iter().collect();
    layers.sort_by_key(|layer| layer.order);
    for layer in layers {
        if !layer.visible {
            continue;
        }
        let (red, green, blue) = layer.rgb();
        for object in document
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id && object.visible)
        {
            match &object.shape {
                Shape::Text(_) => {
                    report.text_omitted += 1;
                    report.warnings.push(format!(
                        "text '{}' was omitted; convert it to paths first",
                        object.name
                    ));
                    continue;
                }
                Shape::Image(_) => {
                    report.images_omitted += 1;
                    report.warnings.push(format!(
                        "image '{}' was omitted; DXF carries vector geometry only",
                        object.name
                    ));
                    continue;
                }
                _ => {}
            }
            let path = object.world_path();
            let points = flatten(&path, 16);
            if points.len() < 2 {
                continue;
            }
            if path
                .segments
                .iter()
                .any(|segment| matches!(segment, Segment::Cubic(_)))
            {
                report.curves_flattened += 1;
            }
            if object.style.fill.is_some() {
                report.fills_outlined += 1;
            }
            out.push_str("0\nLWPOLYLINE\n");
            let _ = writeln!(out, "8\n{}", layer.name);
            let _ = writeln!(out, "62\n{}", true_colour(red, green, blue));
            let _ = writeln!(out, "90\n{}", points.len());
            let _ = writeln!(out, "70\n{}", u32::from(path.closed));
            for (x, y) in points {
                let _ = writeln!(out, "10\n{}", fmt_num(x));
                let _ = writeln!(out, "20\n{}", fmt_num(y));
            }
            report.entities += 1;
        }
    }
    if report.curves_flattened > 0 {
        report.warnings.push(format!(
            "{} path(s) contain curves; the DXF uses straight segments",
            report.curves_flattened
        ));
    }
    if report.fills_outlined > 0 {
        report.warnings.push(format!(
            "{} filled shape(s) were written as outlines; this DXF subset has no fill",
            report.fills_outlined
        ));
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    Ok((out, report))
}

/// Pack an RGB colour as a DXF true-colour value.
fn true_colour(red: u8, green: u8, blue: u8) -> u32 {
    (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Object, Shape};
    use crate::geometry::Transform2;

    #[test]
    fn empty_document_has_header_and_footer() {
        let text = export_dxf(&Document::default()).unwrap();
        assert!(text.starts_with("0\nSECTION\n2\nENTITIES\n"));
        assert!(text.ends_with("0\nENDSEC\n0\nEOF\n"));
    }

    #[test]
    fn rectangle_is_closed_polyline_with_colour() {
        let mut document = Document::default();
        document.layers[0].color = "#ff0000".to_string();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Rect {
                width: 10.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        ));
        let text = export_dxf(&document).unwrap();
        assert!(text.contains("0\nLWPOLYLINE\n"));
        assert!(text.contains("90\n4\n"));
        assert!(text.contains("70\n1\n"));
        assert!(text.contains("62\n16711680\n"));
        assert!(text.contains("10\n10\n20\n5\n"));
    }

    #[test]
    fn transform_is_baked() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let object = Object::new(
            layer_id,
            Shape::Rect {
                width: 2.0,
                height: 2.0,
                corner_radius: 0.0,
            },
        )
        .with_transform(Transform2::translate(100.0, 50.0));
        document.add_object(object);
        let text = export_dxf(&document).unwrap();
        assert!(text.contains("10\n100\n20\n50\n"));
    }

    #[test]
    fn hidden_layers_and_objects_are_skipped() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id.clone(),
            Shape::Ellipse { rx: 1.0, ry: 1.0 },
        ));
        document.layer_mut(&layer_id).unwrap().visible = false;
        let text = export_dxf(&document).unwrap();
        assert!(!text.contains("LWPOLYLINE"));
    }

    #[test]
    fn curves_are_flattened_to_several_vertices() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(layer_id, Shape::Ellipse { rx: 4.0, ry: 2.0 }));
        let text = export_dxf(&document).unwrap();
        assert!(text.contains("90\n64\n"));
    }

    #[test]
    fn the_report_names_omitted_text_images_fills_and_curves() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id.clone(),
            Shape::Text(crate::objects::TextObject {
                content: "hello".to_string(),
                ..crate::objects::TextObject::default()
            }),
        ));
        document.add_object(Object::new(
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
        let mut filled = Object::new(layer_id.clone(), Shape::Ellipse { rx: 4.0, ry: 2.0 });
        filled.style.fill = Some("#ff0000".to_string());
        document.add_object(filled);
        let (text, report) = export_dxf_with_report(&document).unwrap();
        assert!(text.contains("LWPOLYLINE"));
        assert_eq!(report.entities, 1);
        assert_eq!(report.text_omitted, 1);
        assert_eq!(report.images_omitted, 1);
        assert_eq!(report.fills_outlined, 1);
        assert_eq!(report.curves_flattened, 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("text 'Text'"))
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("DXF carries vector geometry only"))
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("no fill"))
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("straight segments"))
        );
    }

    #[test]
    fn dxf_round_trip_preserves_a_rectangle() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Rect {
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        ));
        let text = export_dxf(&document).unwrap();
        let imported = crate::import::dxf::import_dxf(&text).unwrap();
        assert_eq!(imported.objects.len(), 1);
        let (min, max) = imported.objects[0].bounds().unwrap();
        assert!((min.x).abs() < 1e-6);
        assert!((max.x - 20.0).abs() < 1e-6);
        assert!((max.y - 10.0).abs() < 1e-6);
    }
}
