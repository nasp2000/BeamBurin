use std::fmt::Write as _;

use crate::document::Document;
use crate::errors::Result;

use super::paths::flatten;
use super::png::Area;

/// HPGL units per millimetre (40 units/mm is the common plotter resolution).
const UNITS_PER_MM: f64 = 40.0;

/// HPGL output settings (§14.1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HpglOptions {
    /// Region whose origin becomes the plot origin; `None` keeps page zero.
    pub area: Option<Area>,
    /// Convert text to outline paths instead of omitting it.
    pub text_as_paths: bool,
    /// Write only these layers; `None` writes every visible layer.
    pub layer_ids: Option<Vec<String>>,
}

/// Export the document as HPGL for a pen plotter.
///
/// Every visible object becomes one `PU`/`PD` movement with the transform baked
/// into the coordinates. Curves are flattened, one pen is selected per layer
/// (cycling through eight) and the plotter is returned to the origin at the end.
/// Coordinates may be negative: the format is position-based, not clipped.
pub fn export_hpgl(document: &Document) -> Result<String> {
    export_hpgl_with_options(document, &HpglOptions::default())
}

/// Export the document as HPGL, optionally shifted so a region starts at 0,0.
///
/// HPGL has no page or clipping box: the region only moves the origin, so a
/// selection export begins at the plotter origin instead of its page position.
#[allow(clippy::cast_possible_truncation)]
pub fn export_hpgl_with_options(document: &Document, options: &HpglOptions) -> Result<String> {
    let area = super::export_area(document, options.area, "HPGL")?;
    let mut out = String::from("IN;PA;SP1;");
    let mut layers: Vec<_> = document.layers.iter().collect();
    layers.sort_by_key(|layer| layer.order);
    let mut pen = 0;
    for layer in layers {
        if !layer.visible || !super::layer_included(&layer.id, &options.layer_ids) {
            continue;
        }
        pen = pen % 8 + 1;
        let _ = write!(out, "SP{pen};");
        for object in document
            .objects
            .iter()
            .filter(|object| object.layer_id == layer.id && object.visible)
        {
            let Some(paths) = super::object_paths(object, options.text_as_paths) else {
                continue;
            };
            for path in &paths {
                let mut points = flatten(path, 16);
                if path.closed && points.len() > 1 {
                    points.push(points[0]);
                }
                if points.len() < 2 {
                    continue;
                }
                let _ = writeln!(out, "PU{};", coord(points[0], area));
                out.push_str("PD");
                for point in &points[1..] {
                    out.push_str(&coord(*point, area));
                    out.push(',');
                }
                out.pop();
                out.push(';');
            }
        }
    }
    out.push_str("SP0;PU0,0;");
    Ok(out)
}

#[allow(clippy::cast_possible_truncation)]
fn coord(point: (f64, f64), area: Area) -> String {
    format!(
        "{},{}",
        ((point.0 - area.x) * UNITS_PER_MM).round() as i64,
        ((point.1 - area.y) * UNITS_PER_MM).round() as i64
    )
}

#[cfg(test)]
mod tests {
    use super::{HpglOptions, export_hpgl, export_hpgl_with_options};
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
    fn an_empty_document_initialises_and_parks_the_pen() {
        let text = export_hpgl(&Document::default()).unwrap();
        assert!(text.starts_with("IN;PA;SP1;"));
        assert!(text.ends_with("SP0;PU0,0;"));
    }

    #[test]
    fn a_rectangle_is_closed_in_plotter_units() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let mut object = rectangle(10.0, 5.0);
        object.layer_id = layer_id;
        document.add_object(object);
        let text = export_hpgl(&document).unwrap();
        assert!(text.contains("PU0,0;"), "{text}");
        // 10 mm is 400 units and 5 mm is 200 units, and the path closes.
        assert!(text.contains("PD400,0,400,200,0,200,0,0;"), "{text}");
    }

    #[test]
    fn an_area_moves_the_region_to_the_plot_origin() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let mut object = rectangle(10.0, 5.0);
        object.layer_id = layer_id;
        document.add_object(object.with_transform(Transform2::translate(5.0, 4.0)));
        let text = export_hpgl_with_options(
            &document,
            &HpglOptions {
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
        assert!(text.contains("PU0,0;"), "{text}");
        assert!(text.contains("PD400,0,400,200,0,200,0,0;"), "{text}");
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
        let plain = export_hpgl(&document).unwrap();
        assert!(!plain.contains("PD"), "{plain}");
        let outlined = export_hpgl_with_options(
            &document,
            &HpglOptions {
                area: None,
                text_as_paths: true,
                layer_ids: None,
            },
        )
        .unwrap();
        assert!(outlined.contains("PD"), "{outlined}");
    }

    #[test]
    fn invisible_objects_are_skipped() {
        let mut document = Document::default();
        let layer_id = document.layers[0].id.clone();
        let mut object = rectangle(10.0, 5.0);
        object.layer_id = layer_id;
        object.visible = false;
        document.add_object(object);
        let text = export_hpgl(&document).unwrap();
        assert!(!text.contains("PD"), "{text}");
    }
}
