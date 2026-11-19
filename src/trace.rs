//! Shared image tracing core: options, outline and centerline.

use crate::document::{Object, Shape};
use crate::geometry::{Path, Point2, Transform2};
use crate::raster::RasterBitmap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMode {
    Outline,
    Centerline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThresholdSource {
    ImageSettings,
    Custom { value: u8, invert: bool },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceOptions {
    pub mode: TraceMode,
    pub threshold: ThresholdSource,
    pub transparency: bool,
    pub tolerance_mm: f64,
    pub corner_degrees: f64,
    pub min_area_mm2: f64,
}

impl Default for TraceOptions {
    fn default() -> Self {
        Self {
            mode: TraceMode::Outline,
            threshold: ThresholdSource::ImageSettings,
            transparency: false,
            tolerance_mm: 0.2,
            corner_degrees: 40.0,
            min_area_mm2: 0.0,
        }
    }
}

impl TraceOptions {
    pub fn validate(&self) -> Result<(), String> {
        if !(0.02..=2.0).contains(&self.tolerance_mm) {
            return Err("tolerance must be between 0.02 and 2 mm".to_string());
        }
        if !(10.0..=90.0).contains(&self.corner_degrees) {
            return Err("the corner angle must be between 10 and 90 degrees".to_string());
        }
        if !(0.0..=1000.0).contains(&self.min_area_mm2) {
            return Err("the minimum area must be between 0 and 1000 mm2".to_string());
        }
        Ok(())
    }
}

pub struct TracePlacement {
    pub layer_id: String,
    pub operation_id: Option<String>,
    pub origin: Point2,
    pub stem: String,
    pub group_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct TraceOutcome {
    pub objects: Vec<Object>,
    pub contours: usize,
    pub holes: usize,
    pub warnings: Vec<String>,
}

/// Trace one already-prepared raster bitmap into document objects.
pub fn trace_bitmap(
    bitmap: &RasterBitmap,
    width_mm: f64,
    height_mm: f64,
    placement: &TracePlacement,
    options: &TraceOptions,
) -> Result<TraceOutcome, String> {
    options.validate()?;
    if bitmap.width == 0 || bitmap.height == 0 {
        return Err("The image has no burnable areas to trace".to_string());
    }
    match options.mode {
        TraceMode::Outline => trace_outline(bitmap, width_mm, height_mm, placement, options),
        TraceMode::Centerline => trace_centerline(bitmap, width_mm, height_mm, placement, options),
    }
}

fn trace_outline(
    bitmap: &RasterBitmap,
    width_mm: f64,
    height_mm: f64,
    placement: &TracePlacement,
    options: &TraceOptions,
) -> Result<TraceOutcome, String> {
    let contours = crate::vectorize::trace(bitmap, width_mm, height_mm, options.tolerance_mm);
    if contours.is_empty() {
        return Err("The image has no burnable areas to trace".to_string());
    }
    let groups = crate::vectorize::group_contours(&contours);
    let mut objects = Vec::new();
    let mut holes = 0;
    let mut contour_count = 0;
    for group in groups {
        let Some(outer_index) = group.first().copied() else {
            continue;
        };
        let outer_points = &contours[outer_index];
        let area = crate::geometry::boolean::polygon_area(
            &outer_points
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        )
        .abs();
        if area < options.min_area_mm2 {
            continue;
        }
        contour_count += 1;
        let outer_path = crate::vectorize::to_curved_path(outer_points, options.corner_degrees);
        let mut outer = object(
            &outer_path,
            placement,
            format!("{} contour {}", placement.stem, contour_count),
        );
        let outer_id = outer.id.clone();
        outer.hole_of = None;
        objects.push(outer);
        for (index, hole_index) in group.iter().skip(1).enumerate() {
            let path =
                crate::vectorize::to_curved_path(&contours[*hole_index], options.corner_degrees);
            let mut hole = object(
                &path,
                placement,
                format!("{} hole {}", placement.stem, index + 1),
            );
            hole.hole_of = Some(outer_id.clone());
            objects.push(hole);
            holes += 1;
            if objects.len() > 20_000 {
                return Err("The trace would create more than 20000 objects".to_string());
            }
        }
    }
    if objects.is_empty() {
        return Err("The image has no burnable areas to trace".to_string());
    }
    Ok(TraceOutcome {
        objects,
        contours: contour_count,
        holes,
        warnings: Vec::new(),
    })
}

fn trace_centerline(
    bitmap: &RasterBitmap,
    width_mm: f64,
    height_mm: f64,
    placement: &TracePlacement,
    options: &TraceOptions,
) -> Result<TraceOutcome, String> {
    let lines = crate::centerline::trace(bitmap, width_mm, height_mm, options.tolerance_mm);
    if lines.is_empty() {
        return Err("The image has no burnable areas to trace".to_string());
    }
    let mut objects = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let path = crate::centerline::to_path(line);
        objects.push(object(
            &path,
            placement,
            format!("{} centerline {}", placement.stem, index + 1),
        ));
    }
    Ok(TraceOutcome {
        objects,
        contours: lines.len(),
        holes: 0,
        warnings: Vec::new(),
    })
}

fn object(path: &Path, placement: &TracePlacement, name: String) -> Object {
    let mut object = Object::new(placement.layer_id.clone(), Shape::Path(path.clone()));
    object.name = name;
    object.transform = Transform2::translate(placement.origin.x, placement.origin.y);
    object.operation_id.clone_from(&placement.operation_id);
    object.group_id = Some(placement.group_id.clone());
    object.style.stroke = Some("#000000".to_string());
    object.style.fill = None;
    object
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raster::RasterBitmap;

    fn bitmap(width: usize, height: usize, burn: impl Fn(usize, usize) -> bool) -> RasterBitmap {
        RasterBitmap {
            width,
            height,
            power: (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| if burn(x, y) { 255 } else { 0 })
                .collect(),
        }
    }

    fn placement() -> TracePlacement {
        TracePlacement {
            layer_id: "layer".to_string(),
            operation_id: None,
            origin: Point2::origin(),
            stem: "square".to_string(),
            group_id: "group".to_string(),
        }
    }

    #[test]
    fn outline_of_a_square_creates_one_contour() {
        let raster = bitmap(32, 32, |x, y| (8..24).contains(&x) && (8..24).contains(&y));
        let outcome =
            trace_bitmap(&raster, 32.0, 32.0, &placement(), &TraceOptions::default()).unwrap();
        assert_eq!(outcome.contours, 1);
        assert!(outcome.objects[0].shape.to_path().closed);
        assert_eq!(outcome.objects[0].group_id.as_deref(), Some("group"));
    }

    #[test]
    fn nested_ring_links_a_hole() {
        let raster = bitmap(32, 32, |x, y| {
            (6..26).contains(&x)
                && (6..26).contains(&y)
                && !((12..20).contains(&x) && (12..20).contains(&y))
        });
        let outcome =
            trace_bitmap(&raster, 32.0, 32.0, &placement(), &TraceOptions::default()).unwrap();
        assert!(outcome.objects.len() >= 2);
        assert!(
            outcome
                .objects
                .iter()
                .any(|object| object.hole_of.is_some())
        );
    }

    #[test]
    fn centerline_returns_open_paths() {
        let raster = bitmap(32, 32, |_x, y| (15..17).contains(&y));
        let options = TraceOptions {
            mode: TraceMode::Centerline,
            ..TraceOptions::default()
        };
        let outcome = trace_bitmap(&raster, 32.0, 32.0, &placement(), &options).unwrap();
        assert!(!outcome.objects.is_empty());
        assert!(
            outcome
                .objects
                .iter()
                .all(|object| !object.shape.to_path().closed)
        );
    }

    #[test]
    fn minimum_area_filters_small_regions() {
        let raster = bitmap(64, 32, |x, y| {
            ((2..10).contains(&x) && (2..10).contains(&y))
                || ((30..34).contains(&x) && (2..6).contains(&y))
        });
        let options = TraceOptions {
            min_area_mm2: 20.0,
            ..TraceOptions::default()
        };
        let outcome = trace_bitmap(&raster, 64.0, 32.0, &placement(), &options).unwrap();
        assert_eq!(outcome.contours, 1);
    }
}
