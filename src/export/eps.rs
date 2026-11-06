use std::fmt::Write as _;

use crate::document::Document;
use crate::errors::Result;

use super::paths::{flatten, fmt_num};
use super::png::Area;

/// Millimetres per PostScript point.
const MM_PER_POINT: f64 = 25.4 / 72.0;

/// EPS output settings (§14.1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EpsOptions {
    /// Page region to write; `None` writes the whole page.
    pub area: Option<Area>,
    /// Convert text to outline paths instead of omitting it.
    pub text_as_paths: bool,
    /// Write only these layers; `None` writes every visible layer.
    pub layer_ids: Option<Vec<String>>,
}

/// Export the document as an Encapsulated PostScript file.
///
/// The bounding box is the work area in points, the origin is the bottom left
/// and every visible object becomes a stroked subpath in its layer colour. Curves
/// are flattened; text and images are not written, like the other vector
/// exporters.
pub fn export_eps(document: &Document) -> Result<String> {
    export_eps_with_options(document, &EpsOptions::default())
}

/// Export the document as EPS, optionally cropped to a page region.
///
/// The `BoundingBox` becomes the region and every coordinate is shifted by its
/// origin.
#[allow(clippy::cast_possible_truncation)]
pub fn export_eps_with_options(document: &Document, options: &EpsOptions) -> Result<String> {
    let area = super::export_area(document, options.area, "EPS")?;
    let width = (area.width / MM_PER_POINT).ceil().max(1.0) as i64;
    let height = (area.height / MM_PER_POINT).ceil().max(1.0) as i64;
    let mut out = String::new();
    out.push_str("%!PS-Adobe-3.0 EPSF-3.0\n");
    out.push_str("%%Creator: BeamBurin\n");
    let _ = writeln!(out, "%%BoundingBox: 0 0 {width} {height}");
    out.push_str("%%Pages: 1\n%%EndComments\n");

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
            let Some(paths) = super::object_paths(object, options.text_as_paths) else {
                continue;
            };
            let _ = writeln!(
                out,
                "{:.3} {:.3} {:.3} setrgbcolor",
                f64::from(red) / 255.0,
                f64::from(green) / 255.0,
                f64::from(blue) / 255.0
            );
            out.push_str("0.25 setlinewidth\n");
            for path in &paths {
                let points = flatten(path, 16);
                if points.len() < 2 {
                    continue;
                }
                let first = points[0];
                let _ = writeln!(
                    out,
                    "newpath {} {} moveto",
                    point(first.0 - area.x),
                    point(first.1 - area.y)
                );
                for entry in &points[1..] {
                    let _ = writeln!(
                        out,
                        "{} {} lineto",
                        point(entry.0 - area.x),
                        point(entry.1 - area.y)
                    );
                }
                if path.closed {
                    out.push_str("closepath\n");
                }
                out.push_str("stroke\n");
            }
        }
    }
    out.push_str("%%EOF\n");
    Ok(out)
}

fn point(mm: f64) -> String {
    fmt_num(mm / MM_PER_POINT)
}

#[cfg(test)]
mod tests {
    use super::{EpsOptions, export_eps, export_eps_with_options};
    use crate::document::Document;
    use crate::document::model::{Object, Shape};
    use crate::export::Area;
    use crate::geometry::Transform2;

    fn rectangle(width: f64, height: f64) -> Object {
        Object::new(
            "layer",
            Shape::Rect {
                width,
                height,
                corner_radius: 0.0,
            },
        )
    }

    #[test]
    fn the_header_has_the_page_bounding_box() {
        let text = export_eps(&Document::default()).unwrap();
        assert!(text.starts_with("%!PS-Adobe-3.0 EPSF-3.0"));
        // 300 x 200 mm in points, rounded up.
        assert!(text.contains("%%BoundingBox: 0 0 851 567"), "{text}");
        assert!(text.ends_with("%%EOF\n"));
    }

    #[test]
    fn an_area_changes_the_bounding_box_and_shifts_the_path() {
        let mut document = Document {
            width_mm: 20.0,
            height_mm: 10.0,
            ..Document::default()
        };
        let layer_id = document.layers[0].id.clone();
        let mut object = rectangle(10.0, 5.0);
        object.layer_id = layer_id;
        document.add_object(object.with_transform(Transform2::translate(5.0, 4.0)));
        let text = export_eps_with_options(
            &document,
            &EpsOptions {
                area: Some(Area {
                    x: 5.0,
                    y: 4.0,
                    width: 10.0,
                    height: 5.0,
                }),
                text_as_paths: false,
                layer_ids: None,
            },
        )
        .unwrap();
        assert!(text.contains("%%BoundingBox: 0 0 29 15"), "{text}");
        assert!(text.contains("newpath 0 0 moveto"), "{text}");
        assert!(text.contains("28.346457 0 lineto"), "{text}");
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
        let plain = export_eps(&document).unwrap();
        assert!(!plain.contains("moveto"), "{plain}");
        let outlined = export_eps_with_options(
            &document,
            &EpsOptions {
                area: None,
                text_as_paths: true,
                layer_ids: None,
            },
        )
        .unwrap();
        assert!(outlined.contains("moveto"), "{outlined}");
    }

    #[test]
    fn a_rectangle_is_a_closed_stroked_path() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let mut object = rectangle(10.0, 5.0);
        object.layer_id = layer_id;
        document.add_object(object);
        let text = export_eps(&document).unwrap();
        assert!(text.contains("newpath 0 0 moveto"), "{text}");
        assert!(text.contains("28.346457 0 lineto"), "{text}");
        assert!(text.contains("closepath\nstroke\n"), "{text}");
        assert!(text.contains("0.000 0.000 0.000 setrgbcolor"), "{text}");
    }

    #[test]
    fn invisible_objects_are_skipped() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let mut object = rectangle(10.0, 5.0);
        object.layer_id = layer_id;
        object.visible = false;
        document.add_object(object);
        let text = export_eps(&document).unwrap();
        assert!(!text.contains("moveto"), "{text}");
    }
}
