use std::fmt::Write as _;

use crate::document::{Document, Shape};
use crate::errors::Result;
use crate::geometry::{Point2, path::Segment};

use super::paths::{flatten, fmt_num};
use super::png::Area;

/// DXF output settings (§14.2).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DxfOptions {
    /// Region whose origin becomes the drawing origin; `None` keeps page zero.
    pub area: Option<Area>,
    /// Convert text to outline paths instead of omitting it.
    pub text_as_paths: bool,
    /// Write only these layers; `None` writes every visible layer.
    pub layer_ids: Option<Vec<String>>,
    /// Write axis-aligned ellipses as `CIRCLE`/`ELLIPSE` entities.
    pub arcs: bool,
}

/// What the DXF export wrote and what it had to leave out (§14.2).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DxfReport {
    /// Polylines written.
    pub entities: usize,
    /// Text objects that were not written.
    pub text_omitted: usize,
    /// Images that were not written.
    pub images_omitted: usize,
    /// Ellipses written as `CIRCLE` or `ELLIPSE` entities.
    pub arcs_written: usize,
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

/// Export the document with options.
pub fn export_dxf_with_options(document: &Document, options: &DxfOptions) -> Result<String> {
    export_dxf_with_report_options(document, options).map(|(text, _)| text)
}

/// Export the document and describe what the subset could not represent.
pub fn export_dxf_with_report(document: &Document) -> Result<(String, DxfReport)> {
    export_dxf_with_report_options(document, &DxfOptions::default())
}

/// Export the document with options and describe what was left out.
///
/// The optional area only moves the origin: DXF has no page or clipping box,
/// so a selection export starts at 0,0 instead of its page position.
pub fn export_dxf_with_report_options(
    document: &Document,
    options: &DxfOptions,
) -> Result<(String, DxfReport)> {
    let area = super::export_area(document, options.area, "DXF")?;
    let mut out = String::new();
    let mut report = DxfReport::default();
    // Declare millimetres so readers do not have to guess the drawing unit.
    out.push_str("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n");
    out.push_str("0\nSECTION\n2\nENTITIES\n");
    let mut layers: Vec<_> = document.layers.iter().collect();
    layers.sort_by_key(|layer| layer.order);
    for layer in layers {
        if !layer.visible || !super::layer_included(&layer.id, options.layer_ids.as_deref()) {
            continue;
        }
        let (red, green, blue) = layer.rgb();
        for object in document
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id && object.visible)
        {
            if options.arcs {
                if let Shape::Ellipse { rx, ry } = &object.shape {
                    if let Some(curve) = dxf_curve(object, *rx, *ry) {
                        if object.style.fill.is_some() {
                            report.fills_outlined += 1;
                        }
                        write_curve(
                            &mut out,
                            curve,
                            layer.name.as_str(),
                            (red, green, blue),
                            area,
                        );
                        report.arcs_written += 1;
                        report.entities += 1;
                        continue;
                    }
                    report.warnings.push(format!(
                        "ellipse '{}' has a rotated or skewed transform and was flattened",
                        object.name
                    ));
                }
            }
            let paths = match &object.shape {
                Shape::Text(text) => {
                    if !options.text_as_paths {
                        report.text_omitted += 1;
                        report.warnings.push(format!(
                            "text '{}' was omitted; enable text as paths first",
                            object.name
                        ));
                        continue;
                    }
                    match crate::objects::text_to_paths(text) {
                        Ok(paths) => paths
                            .iter()
                            .map(|path| path.transform(&object.transform))
                            .collect(),
                        Err(error) => {
                            report.text_omitted += 1;
                            report.warnings.push(format!(
                                "text '{}' could not be converted to paths: {error}",
                                object.name
                            ));
                            continue;
                        }
                    }
                }
                Shape::Image(_) => {
                    report.images_omitted += 1;
                    report.warnings.push(format!(
                        "image '{}' was omitted; DXF carries vector geometry only",
                        object.name
                    ));
                    continue;
                }
                _ => vec![object.world_path()],
            };
            if object.style.fill.is_some() {
                report.fills_outlined += 1;
            }
            for path in &paths {
                let points = flatten(path, 16);
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
                out.push_str("0\nLWPOLYLINE\n");
                let _ = writeln!(out, "8\n{}", layer.name);
                let _ = writeln!(out, "62\n{}", true_colour(red, green, blue));
                let _ = writeln!(out, "90\n{}", points.len());
                let _ = writeln!(out, "70\n{}", u32::from(path.closed));
                for (x, y) in points {
                    let _ = writeln!(out, "10\n{}", fmt_num(x - area.x));
                    let _ = writeln!(out, "20\n{}", fmt_num(y - area.y));
                }
                report.entities += 1;
            }
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

/// A DXF curve entity derived from an axis-aligned ellipse object.
#[derive(Debug, Clone, Copy)]
enum DxfCurve {
    Circle {
        center: Point2,
        radius: f64,
    },
    Ellipse {
        center: Point2,
        major: Point2,
        ratio: f64,
    },
}

/// Convert an ellipse with an axis-aligned transform, or return `None`.
fn dxf_curve(object: &crate::document::Object, rx: f64, ry: f64) -> Option<DxfCurve> {
    let transform = object.transform;
    if transform.b.abs() > 1e-9 || transform.c.abs() > 1e-9 {
        return None;
    }
    let scale_x = transform.a.abs();
    let scale_y = transform.d.abs();
    if scale_x <= 1e-12 || scale_y <= 1e-12 {
        return None;
    }
    let center = transform.apply(Point2::new(0.0, 0.0));
    let rx = rx.abs() * scale_x;
    let ry = ry.abs() * scale_y;
    if rx <= 1e-12 || ry <= 1e-12 {
        return None;
    }
    let largest = rx.max(ry);
    if (rx - ry).abs() <= largest * 1e-9 {
        return Some(DxfCurve::Circle { center, radius: rx });
    }
    if rx >= ry {
        Some(DxfCurve::Ellipse {
            center,
            major: Point2::new(rx, 0.0),
            ratio: (ry / rx).clamp(0.0, 1.0),
        })
    } else {
        Some(DxfCurve::Ellipse {
            center,
            major: Point2::new(0.0, ry),
            ratio: (rx / ry).clamp(0.0, 1.0),
        })
    }
}

fn write_curve(
    out: &mut String,
    curve: DxfCurve,
    layer_name: &str,
    (red, green, blue): (u8, u8, u8),
    area: Area,
) {
    match curve {
        DxfCurve::Circle { center, radius } => {
            out.push_str("0\nCIRCLE\n");
            let _ = writeln!(out, "8\n{layer_name}");
            let _ = writeln!(out, "62\n{}", true_colour(red, green, blue));
            let _ = writeln!(out, "10\n{}", fmt_num(center.x - area.x));
            let _ = writeln!(out, "20\n{}", fmt_num(center.y - area.y));
            let _ = writeln!(out, "30\n0");
            let _ = writeln!(out, "40\n{}", fmt_num(radius));
        }
        DxfCurve::Ellipse {
            center,
            major,
            ratio,
        } => {
            out.push_str("0\nELLIPSE\n");
            let _ = writeln!(out, "8\n{layer_name}");
            let _ = writeln!(out, "62\n{}", true_colour(red, green, blue));
            let _ = writeln!(out, "10\n{}", fmt_num(center.x - area.x));
            let _ = writeln!(out, "20\n{}", fmt_num(center.y - area.y));
            let _ = writeln!(out, "30\n0");
            let _ = writeln!(out, "11\n{}", fmt_num(major.x));
            let _ = writeln!(out, "21\n{}", fmt_num(major.y));
            let _ = writeln!(out, "31\n0");
            let _ = writeln!(out, "40\n{}", fmt_num(ratio));
            let _ = writeln!(out, "41\n0");
            let _ = writeln!(out, "42\n{}", fmt_num(std::f64::consts::TAU));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{Object, Shape};
    use crate::geometry::Transform2;

    #[test]
    fn empty_document_has_header_and_footer() {
        let text = export_dxf(&Document::default()).unwrap();
        assert!(text.starts_with("0\nSECTION\n2\nHEADER\n"));
        assert!(text.contains("9\n$INSUNITS\n70\n4\n"));
        assert!(text.contains("0\nSECTION\n2\nENTITIES\n"));
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
    fn an_area_moves_the_drawing_to_the_origin() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let object = Object::new(
            layer_id,
            Shape::Rect {
                width: 10.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        )
        .with_transform(Transform2::translate(5.0, 4.0));
        document.add_object(object);
        let text = export_dxf_with_options(
            &document,
            &DxfOptions {
                area: Some(crate::export::Area {
                    x: 5.0,
                    y: 4.0,
                    width: 10.0,
                    height: 5.0,
                }),
                text_as_paths: false,
                layer_ids: None,
                arcs: false,
            },
        )
        .unwrap();
        assert!(text.contains("10\n0\n20\n0\n"), "{text}");
        assert!(text.contains("10\n10\n20\n5\n"), "{text}");
    }

    #[test]
    fn text_can_be_written_as_outline_paths() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id,
            Shape::Text(crate::objects::TextObject {
                content: "A".to_string(),
                x: 10.0,
                y: 10.0,
                ..crate::objects::TextObject::default()
            }),
        ));
        let (plain, plain_report) = export_dxf_with_report_options(
            &document,
            &DxfOptions {
                area: None,
                text_as_paths: false,
                layer_ids: None,
                arcs: false,
            },
        )
        .unwrap();
        assert!(!plain.contains("LWPOLYLINE"), "{plain}");
        assert_eq!(plain_report.text_omitted, 1);
        let (outlined, report) = export_dxf_with_report_options(
            &document,
            &DxfOptions {
                area: None,
                text_as_paths: true,
                layer_ids: None,
                arcs: false,
            },
        )
        .unwrap();
        assert!(outlined.contains("LWPOLYLINE"), "{outlined}");
        assert_eq!(report.text_omitted, 0);
        assert!(report.entities > 0);
    }

    #[test]
    fn ellipses_can_be_written_as_curve_entities() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(
            layer_id.clone(),
            Shape::Ellipse { rx: 5.0, ry: 5.0 },
        ));
        let text = export_dxf_with_options(
            &document,
            &DxfOptions {
                area: None,
                text_as_paths: false,
                layer_ids: None,
                arcs: true,
            },
        )
        .unwrap();
        assert!(text.contains("0\nCIRCLE\n"), "{text}");
        assert!(text.contains("40\n5\n"), "{text}");
        assert!(!text.contains("LWPOLYLINE"), "{text}");

        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        document.add_object(Object::new(layer_id, Shape::Ellipse { rx: 5.0, ry: 2.0 }));
        let text = export_dxf_with_options(
            &document,
            &DxfOptions {
                area: None,
                text_as_paths: false,
                layer_ids: None,
                arcs: true,
            },
        )
        .unwrap();
        assert!(text.contains("0\nELLIPSE\n"), "{text}");
        assert!(text.contains("40\n0.4\n"), "{text}");
    }

    #[test]
    fn a_rotated_ellipse_falls_back_to_a_polyline() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let object = Object::new(layer_id, Shape::Ellipse { rx: 5.0, ry: 2.0 })
            .with_transform(Transform2::rotate(30.0));
        document.add_object(object);
        let (text, report) = export_dxf_with_report_options(
            &document,
            &DxfOptions {
                area: None,
                text_as_paths: false,
                layer_ids: None,
                arcs: true,
            },
        )
        .unwrap();
        assert!(text.contains("LWPOLYLINE"), "{text}");
        assert_eq!(report.arcs_written, 0);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("rotated or skewed")),
            "{:?}",
            report.warnings
        );
    }

    #[test]
    fn only_chosen_layers_are_written() {
        let mut document = Document::default();
        let first = document.layers[0].id.clone();
        document.add_object(Object::new(
            first,
            Shape::Rect {
                width: 1.0,
                height: 1.0,
                corner_radius: 0.0,
            },
        ));
        let second = document.add_layer("Second").id.clone();
        document.add_object(Object::new(
            second.clone(),
            Shape::Rect {
                width: 2.0,
                height: 2.0,
                corner_radius: 0.0,
            },
        ));
        let text = export_dxf_with_options(
            &document,
            &DxfOptions {
                area: None,
                text_as_paths: false,
                layer_ids: Some(vec![second]),
                arcs: false,
            },
        )
        .unwrap();
        assert!(text.contains("8\nSecond\n"), "{text}");
        assert!(!text.contains("8\nLayer 1\n"), "{text}");
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
