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
    /// Trace only the tones inside a brightness band, optionally inverted.
    ///
    /// The two ends are the lowest and the highest brightness to include, so a
    /// mid-grey drawing can be traced without the white paper or the black
    /// shadows around it.
    Custom {
        low: u8,
        high: u8,
        invert: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceOptions {
    pub mode: TraceMode,
    pub threshold: ThresholdSource,
    pub transparency: bool,
    pub tolerance_mm: f64,
    pub corner_degrees: f64,
    pub min_area_mm2: f64,
    /// Drop holes smaller than this, in square millimetres.
    pub min_hole_mm2: f64,
    /// Turn the traced points into curves instead of straight segments.
    pub smooth: bool,
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
            min_hole_mm2: 0.0,
            smooth: true,
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
        if !(0.0..=1000.0).contains(&self.min_hole_mm2) {
            return Err("the minimum hole area must be between 0 and 1000 mm2".to_string());
        }
        if let ThresholdSource::Custom { low, high, .. } = self.threshold
            && low > high
        {
            return Err("the lowest brightness must not be above the highest".to_string());
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
        let outer_path = outline_path(outer_points, options);
        let mut outer = object(
            &outer_path,
            placement,
            format!("{} contour {}", placement.stem, contour_count),
        );
        let outer_id = outer.id.clone();
        outer.hole_of = None;
        objects.push(outer);
        for (index, hole_index) in group.iter().skip(1).enumerate() {
            let hole_points = &contours[*hole_index];
            let hole_area = crate::geometry::boolean::polygon_area(
                &hole_points
                    .iter()
                    .map(|point| (point.x, point.y))
                    .collect::<Vec<_>>(),
            )
            .abs();
            if hole_area < options.min_hole_mm2 {
                continue;
            }
            let path = outline_path(hole_points, options);
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
        let path = if options.smooth {
            crate::centerline::to_curved_path(line, options.corner_degrees)
        } else {
            crate::centerline::to_path(line)
        };
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

/// Contour path: curves when smoothing is on, straight segments otherwise.
fn outline_path(points: &[Point2], options: &TraceOptions) -> Path {
    if options.smooth {
        crate::vectorize::to_curved_path(points, options.corner_degrees)
    } else {
        crate::vectorize::to_path(points)
    }
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
    #![allow(clippy::cast_precision_loss)]

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
        assert_ne!(outcome.objects.len(), 0);
        assert!(
            outcome
                .objects
                .iter()
                .all(|object| !object.shape.to_path().closed)
        );
    }

    #[test]
    fn smoothing_turns_an_outline_into_curves_and_can_be_switched_off() {
        // A ring: one outer contour with many points.
        let raster = bitmap(48, 48, |x, y| {
            let dx = x as f64 - 24.0;
            let dy = y as f64 - 24.0;
            let radius = dx.hypot(dy);
            (10.0..20.0).contains(&radius)
        });
        let curved =
            trace_bitmap(&raster, 48.0, 48.0, &placement(), &TraceOptions::default()).unwrap();
        assert!(
            curved.objects[0]
                .shape
                .to_path()
                .segments
                .iter()
                .any(|segment| matches!(segment, crate::geometry::path::Segment::Cubic(_))),
            "smoothing did not produce curves"
        );
        let straight_options = TraceOptions {
            smooth: false,
            ..TraceOptions::default()
        };
        let straight = trace_bitmap(&raster, 48.0, 48.0, &placement(), &straight_options).unwrap();
        assert!(
            straight.objects[0]
                .shape
                .to_path()
                .segments
                .iter()
                .all(|segment| matches!(segment, crate::geometry::path::Segment::Line { .. })),
            "smoothing was not switched off"
        );
    }

    #[test]
    fn tiny_holes_can_be_filled_in() {
        // A square with a big hole and a single-pixel pinhole.
        let raster = bitmap(64, 64, |x, y| {
            let inside = (8..56).contains(&x) && (8..56).contains(&y);
            let big_hole = (20..44).contains(&x) && (20..44).contains(&y);
            let pinhole = x == 50 && y == 50;
            inside && !big_hole && !pinhole
        });
        let keep_holes =
            trace_bitmap(&raster, 64.0, 64.0, &placement(), &TraceOptions::default()).unwrap();
        assert_eq!(keep_holes.holes, 2, "both holes should be kept");
        let filled_options = TraceOptions {
            min_hole_mm2: 4.0,
            ..TraceOptions::default()
        };
        let filled = trace_bitmap(&raster, 64.0, 64.0, &placement(), &filled_options).unwrap();
        assert_eq!(filled.holes, 1, "the pinhole was not filled in");
    }

    #[test]
    fn a_reversed_brightness_band_is_rejected() {
        let options = TraceOptions {
            threshold: ThresholdSource::Custom {
                low: 200,
                high: 20,
                invert: false,
            },
            ..TraceOptions::default()
        };
        assert!(options.validate().is_err());
        assert!(
            TraceOptions {
                min_hole_mm2: -1.0,
                ..TraceOptions::default()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn centreline_smoothing_follows_an_open_stroke() {
        // A wavy horizontal stroke: a single open chain with many points.
        let raster = bitmap(64, 48, |x, y| {
            let wave = 24.0 + 8.0 * ((x as f64 / 63.0) * std::f64::consts::TAU).sin();
            (y as f64 - wave).abs() < 2.0
        });
        let options = TraceOptions {
            mode: TraceMode::Centerline,
            smooth: true,
            ..TraceOptions::default()
        };
        let outcome = trace_bitmap(&raster, 64.0, 48.0, &placement(), &options).unwrap();
        assert!(!outcome.objects.is_empty());
        let path = outcome.objects[0].shape.to_path();
        assert!(!path.closed, "an open stroke became a closed path");
        assert!(
            path.segments
                .iter()
                .any(|segment| matches!(segment, crate::geometry::path::Segment::Cubic(_))),
            "centreline smoothing did not produce curves"
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
