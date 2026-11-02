//! Fit circular arcs to runs of cut moves.
//!
//! Toolpaths are generated as dense polylines: curves are flattened with a
//! 0.2 mm tolerance before the writer sees them. When the user enables arc
//! output, a run of cut moves that lies on one circle is replaced by a single
//! `G2`/`G3` command. The fit is deliberately conservative: it never crosses a
//! power change and it refuses anything that deviates from the circle by more
//! than [`ARC_TOLERANCE_MM`], so enabling the option cannot move the tool
//! outside the geometry the document already describes.
//!
//! The tolerance stays below the radius check that common GRBL-based senders
//! apply to `I`/`J` arcs (0.005 mm), because `I` and `J` are rounded to the
//! configured number of coordinate decimals when written.

use std::f64::consts::{PI, TAU};

/// Largest radial deviation, in millimetres, accepted between a point and the
/// circle it is fitted to.
pub const ARC_TOLERANCE_MM: f64 = 0.004;

/// Smallest and largest circle radius that is worth writing as an arc.
const MIN_RADIUS_MM: f64 = 0.2;
const MAX_RADIUS_MM: f64 = 100_000.0;

/// Smallest number of points (including the start) that may define an arc.
///
/// Three points always define a circle, so a longer run is required before the
/// fit is trusted; this keeps a sharp corner from being rounded off.
pub const MIN_POINTS: usize = 5;

/// Largest angle between two consecutive moves, in radians.
const MAX_STEP_RAD: f64 = 0.35;

/// Smallest total angle that is worth a dedicated arc command.
const MIN_SPAN_RAD: f64 = 0.05;

/// A little under a full turn: the closing move of a circle stays a `G1`, which
/// keeps the output valid for senders that reject full-circle `I`/`J` arcs.
const MAX_SPAN_RAD: f64 = TAU - 0.02;

/// A circular arc that replaces a run of cut moves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FittedArc {
    /// `G2` when true, `G3` when false.
    pub clockwise: bool,
    /// End point of the arc.
    pub x: f64,
    pub y: f64,
    /// Centre offset from the start point.
    pub i: f64,
    pub j: f64,
    /// Number of moves consumed, starting at the first point after the start.
    pub moves: usize,
}

/// Fit a circle through `points`, where the first point is the current head
/// position and the remaining ones are consecutive cut targets.
///
/// Returns `None` when the run is too short, is straight or has a corner, turns
/// more than a full circle, or leaves the circle by more than
/// [`ARC_TOLERANCE_MM`].
pub fn fit(points: &[(f64, f64)]) -> Option<FittedArc> {
    if points.len() < MIN_POINTS {
        return None;
    }
    let first = points[0];
    let middle = points[points.len() / 2];
    let last = points[points.len() - 1];
    let (center_x, center_y, radius) = circumcircle(first, middle, last)?;
    if !(MIN_RADIUS_MM..=MAX_RADIUS_MM).contains(&radius) {
        return None;
    }
    for point in points {
        let distance = distance(*point, (center_x, center_y));
        if (distance - radius).abs() > ARC_TOLERANCE_MM {
            return None;
        }
    }

    let mut total = 0.0;
    let mut previous = angle(first, (center_x, center_y));
    let mut direction: Option<bool> = None;
    for point in &points[1..] {
        let current = angle(*point, (center_x, center_y));
        let mut delta = current - previous;
        while delta > PI {
            delta -= TAU;
        }
        while delta < -PI {
            delta += TAU;
        }
        if delta.abs() < 1e-9 || delta.abs() > MAX_STEP_RAD {
            return None;
        }
        let positive = delta > 0.0;
        match direction {
            None => direction = Some(positive),
            Some(previous_direction) if previous_direction != positive => return None,
            Some(_) => {}
        }
        total += delta;
        previous = current;
    }
    if total.abs() < MIN_SPAN_RAD || total.abs() > MAX_SPAN_RAD {
        return None;
    }

    Some(FittedArc {
        clockwise: total < 0.0,
        x: last.0,
        y: last.1,
        i: center_x - first.0,
        j: center_y - first.1,
        moves: points.len() - 1,
    })
}

/// Circle through three points, or `None` when they are collinear.
fn circumcircle(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> Option<(f64, f64, f64)> {
    let determinant = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if !determinant.is_finite() {
        return None;
    }
    let longest = distance(a, b).max(distance(b, c)).max(distance(c, a));
    // Relative test: a nearly collinear triple has a determinant that is tiny
    // compared with the square of its longest side.
    if longest < 1e-6 || determinant.abs() < 1e-7 * longest * longest {
        return None;
    }
    let a2 = a.0 * a.0 + a.1 * a.1;
    let b2 = b.0 * b.0 + b.1 * b.1;
    let c2 = c.0 * c.0 + c.1 * c.1;
    let center_x = (a2 * (b.1 - c.1) + b2 * (c.1 - a.1) + c2 * (a.1 - b.1)) / determinant;
    let center_y = (a2 * (c.0 - b.0) + b2 * (a.0 - c.0) + c2 * (b.0 - a.0)) / determinant;
    let radius = distance(a, (center_x, center_y));
    if !center_x.is_finite() || !center_y.is_finite() || !radius.is_finite() {
        return None;
    }
    Some((center_x, center_y, radius))
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn angle(point: (f64, f64), center: (f64, f64)) -> f64 {
    (point.1 - center.1).atan2(point.0 - center.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_points(
        radius: f64,
        start: f64,
        end: f64,
        count: usize,
        clockwise: bool,
    ) -> Vec<(f64, f64)> {
        (0..=count)
            .map(|index| {
                #[allow(clippy::cast_precision_loss)]
                let t = index as f64 / count as f64;
                let angle = start + (end - start) * t;
                let angle = if clockwise { -angle } else { angle };
                (radius * angle.cos(), radius * angle.sin())
            })
            .collect()
    }

    #[test]
    fn a_quarter_circle_is_fitted_counter_clockwise() {
        let points = arc_points(10.0, 0.0, std::f64::consts::FRAC_PI_2, 20, false);
        let arc = fit(&points).expect("quarter circle");
        assert!(!arc.clockwise);
        assert!((arc.x - 0.0).abs() < 1e-6);
        assert!((arc.y - 10.0).abs() < 1e-6);
        assert!((arc.i + 10.0).abs() < 1e-6);
        assert!(arc.j.abs() < 1e-6);
        assert_eq!(arc.moves, 20);
    }

    #[test]
    fn a_clockwise_circle_is_fitted_clockwise() {
        let points = arc_points(5.0, 0.0, std::f64::consts::PI, 24, true);
        let arc = fit(&points).expect("clockwise arc");
        assert!(arc.clockwise);
    }

    #[test]
    fn a_straight_run_is_not_an_arc() {
        let points: Vec<(f64, f64)> = (0..12).map(|index| (f64::from(index), 0.0)).collect();
        assert!(fit(&points).is_none());
    }

    #[test]
    fn a_sharp_corner_is_not_an_arc() {
        let mut points = Vec::new();
        for index in 0..8 {
            points.push((f64::from(index), 0.0));
        }
        for index in 1..8 {
            points.push((7.0, f64::from(index)));
        }
        assert!(fit(&points).is_none());
    }

    #[test]
    fn a_point_off_the_circle_breaks_the_fit() {
        let mut points = arc_points(10.0, 0.0, std::f64::consts::FRAC_PI_2, 20, false);
        points[10].0 += 0.1;
        assert!(fit(&points).is_none());
    }

    #[test]
    fn duplicate_points_break_the_fit() {
        let mut points = arc_points(10.0, 0.0, 1.0, 20, false);
        points[7] = points[6];
        assert!(fit(&points).is_none());
    }

    #[test]
    fn a_full_circle_stops_short_of_the_closing_point() {
        let points = arc_points(10.0, 0.0, TAU, 120, false);
        // The complete run makes a full turn and is refused so the last move
        // can stay a straight closing move.
        assert!(fit(&points).is_none());
        let without_closing = &points[..points.len() - 1];
        let arc = fit(without_closing).expect("almost full circle");
        assert!(!arc.clockwise);
        assert_eq!(arc.moves, without_closing.len() - 1);
    }

    #[test]
    fn a_short_run_is_not_trusted() {
        let points = arc_points(10.0, 0.0, 0.3, 3, false);
        assert_eq!(points.len(), 4);
        assert!(fit(&points).is_none());
    }

    #[test]
    fn a_tiny_circle_is_not_written_as_an_arc() {
        let points = arc_points(0.05, 0.0, std::f64::consts::FRAC_PI_2, 20, false);
        assert!(fit(&points).is_none());
    }
}
