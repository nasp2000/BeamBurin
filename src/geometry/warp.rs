//! Non-affine deformation kernels: quadrilateral, 4×4 lattice and taper.

use crate::geometry::path::Segment;
use crate::geometry::{CubicBezier, Path, Point2};

pub const WARP_TOLERANCE_MM: f64 = 0.02;
pub const MAX_WARP_SEGMENTS: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadWarp {
    /// Bottom-left, bottom-right, top-left, top-right.
    pub corners: [Point2; 4],
}

impl QuadWarp {
    pub fn new(corners: [Point2; 4]) -> Option<Self> {
        if corners
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            return None;
        }
        let (bl, br, tl, tr) = (corners[0], corners[1], corners[2], corners[3]);
        let area = 0.5
            * ((bl.x * br.y - br.x * bl.y)
                + (br.x * tr.y - tr.x * br.y)
                + (tr.x * tl.y - tl.x * tr.y)
                + (tl.x * bl.y - bl.x * tl.y));
        (area.abs() >= 1e-6).then_some(Self { corners })
    }

    /// Bilinear map on the unit square.
    pub fn map(&self, u: f64, v: f64) -> Point2 {
        let [bl, br, tl, tr] = self.corners;
        let top = Point2::new(tl.x + (tr.x - tl.x) * u, tl.y + (tr.y - tl.y) * u);
        let bottom = Point2::new(bl.x + (br.x - bl.x) * u, bl.y + (br.y - bl.y) * u);
        Point2::new(
            bottom.x + (top.x - bottom.x) * v,
            bottom.y + (top.y - bottom.y) * v,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatticeWarp {
    /// `[row][column]`, with row = v from bottom to top.
    pub nodes: [[Point2; 4]; 4],
}

impl LatticeWarp {
    pub fn new(nodes: [[Point2; 4]; 4]) -> Option<Self> {
        if nodes
            .iter()
            .flatten()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            return None;
        }
        let area = (nodes[3][3].x - nodes[0][0].x).abs() * (nodes[3][3].y - nodes[0][0].y).abs();
        (area >= 1e-6).then_some(Self { nodes })
    }

    pub fn map(&self, u: f64, v: f64) -> Point2 {
        let mut xs = [0.0; 4];
        let mut ys = [0.0; 4];
        for row in 0..4 {
            let cols = self.nodes[row];
            xs[row] = interp4([cols[0].x, cols[1].x, cols[2].x, cols[3].x], u);
            ys[row] = interp4([cols[0].y, cols[1].y, cols[2].y, cols[3].y], u);
        }
        Point2::new(interp4(xs, v), interp4(ys, v))
    }
}

/// Catmull-Rom through four uniformly spaced values, with secant end tangents.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn interp4(values: [f64; 4], t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let scaled = t * 3.0;
    let index = (scaled.floor() as usize).min(2);
    let local = scaled - index as f64;
    let p0 = if index == 0 {
        values[1] - values[0]
    } else {
        values[index] - values[index - 1]
    };
    let p1 = values[index];
    let p2 = values[index + 1];
    let p3 = if index + 2 < 4 {
        values[index + 2] - values[index + 1]
    } else {
        values[3] - values[2]
    };
    let m1 = if index == 0 {
        p0
    } else {
        (p2 - values[index - 1]) * 0.5
    };
    let m2 = if index + 2 >= 4 {
        p3
    } else {
        (values[index + 2] - p1) * 0.5
    };
    let t2 = local * local;
    let t3 = t2 * local;
    (2.0 * t3 - 3.0 * t2 + 1.0) * p1
        + (t3 - 2.0 * t2 + local) * m1
        + (-2.0 * t3 + 3.0 * t2) * p2
        + (t3 - t2) * m2
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaperOrientation {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaperSpec {
    pub top_width: f64,
    pub bottom_width: f64,
    pub length: f64,
    pub orientation: TaperOrientation,
    pub diameters: bool,
    pub wrap_fraction: f64,
}

impl TaperSpec {
    pub fn quad(&self, domain: (Point2, Point2)) -> Option<QuadWarp> {
        if !self.top_width.is_finite()
            || !self.bottom_width.is_finite()
            || !self.length.is_finite()
            || self.top_width <= 0.0
            || self.bottom_width <= 0.0
            || self.length <= 0.0
            || !(1.0..=100.0).contains(&self.wrap_fraction)
        {
            return None;
        }
        let factor = if self.diameters {
            std::f64::consts::PI * self.wrap_fraction / 100.0
        } else {
            1.0
        };
        let top = self.top_width * factor;
        let bottom = self.bottom_width * factor;
        let cx = f64::midpoint(domain.0.x, domain.1.x);
        let cy = f64::midpoint(domain.0.y, domain.1.y);
        let half_l = self.length / 2.0;
        let mut corners = [
            Point2::new(cx - bottom / 2.0, cy - half_l),
            Point2::new(cx + bottom / 2.0, cy - half_l),
            Point2::new(cx - top / 2.0, cy + half_l),
            Point2::new(cx + top / 2.0, cy + half_l),
        ];
        let turns = match self.orientation {
            TaperOrientation::Top => 0,
            TaperOrientation::Bottom => 2,
            TaperOrientation::Left => 1,
            TaperOrientation::Right => 3,
        };
        for _ in 0..turns {
            corners = [
                Point2::new(2.0 * cx - corners[2].x, 2.0 * cy - corners[2].y),
                Point2::new(2.0 * cx - corners[3].x, 2.0 * cy - corners[3].y),
                Point2::new(2.0 * cx - corners[0].x, 2.0 * cy - corners[0].y),
                Point2::new(2.0 * cx - corners[1].x, 2.0 * cy - corners[1].y),
            ];
        }
        QuadWarp::new(corners)
    }
}

/// Warp a path through a unit-square map, baking the result into document coordinates.
pub fn warp_path<M: Fn(f64, f64) -> Point2>(
    path: &Path,
    domain: (Point2, Point2),
    map: M,
) -> Result<Path, String> {
    let width = (domain.1.x - domain.0.x).max(1.0);
    let height = (domain.1.y - domain.0.y).max(1.0);
    let normalize = |point: Point2| {
        (
            (point.x - domain.0.x) / width,
            (point.y - domain.0.y) / height,
        )
    };
    let mut output = Path::new();
    output.closed = path.closed;
    for segment in &path.segments {
        let cubic = match *segment {
            Segment::Line { from, to } => CubicBezier {
                p0: from,
                p1: Point2::new(
                    from.x + (to.x - from.x) / 3.0,
                    from.y + (to.y - from.y) / 3.0,
                ),
                p2: Point2::new(
                    from.x + 2.0 * (to.x - from.x) / 3.0,
                    from.y + 2.0 * (to.y - from.y) / 3.0,
                ),
                p3: to,
            },
            Segment::Cubic(cubic) => cubic,
        };
        subdivide(&cubic, &normalize, &map, 0, &mut output)?;
        if output.segments.len() > MAX_WARP_SEGMENTS {
            return Err(format!(
                "The warp exceeds the {MAX_WARP_SEGMENTS} segment limit; simplify the selection first"
            ));
        }
    }
    Ok(output)
}

fn subdivide<M: Fn(f64, f64) -> Point2>(
    cubic: &CubicBezier,
    normalize: &impl Fn(Point2) -> (f64, f64),
    map: &M,
    depth: usize,
    output: &mut Path,
) -> Result<(), String> {
    let mapped = |point: Point2| {
        let (u, v) = normalize(point);
        map(u, v)
    };
    let p0 = mapped(cubic.p0);
    let p1 = mapped(cubic.p1);
    let p2 = mapped(cubic.p2);
    let p3 = mapped(cubic.p3);
    let candidate = CubicBezier { p0, p1, p2, p3 };
    let error = [0.25, 0.5, 0.75]
        .iter()
        .map(|t| {
            let original = cubic.eval(*t);
            let exact = mapped(original);
            let approximated = candidate.eval(*t);
            (exact.x - approximated.x).hypot(exact.y - approximated.y)
        })
        .fold(0.0_f64, f64::max);
    if error <= WARP_TOLERANCE_MM || depth >= 12 {
        if !p0.x.is_finite() || !p0.y.is_finite() || !p3.x.is_finite() || !p3.y.is_finite() {
            return Err("The warp produced non-finite geometry".to_string());
        }
        let straight = (p1.x - p0.x) * (p3.y - p0.y) - (p1.y - p0.y) * (p3.x - p0.x);
        let straight = straight.abs() < 1e-6
            && (p2.x - p0.x) * (p3.y - p0.y) - (p2.y - p0.y) * (p3.x - p0.x) < 1e-6;
        if straight {
            output.push(Segment::Line { from: p0, to: p3 });
        } else {
            output.push(Segment::Cubic(candidate));
        }
        return Ok(());
    }
    let (first, second) = cubic.split(0.5);
    subdivide(&first, normalize, map, depth + 1, output)?;
    subdivide(&second, normalize, map, depth + 1, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain() -> (Point2, Point2) {
        (Point2::new(0.0, 0.0), Point2::new(10.0, 10.0))
    }

    #[test]
    fn identity_quad_maps_points_unchanged() {
        let quad = QuadWarp::new([
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(0.0, 10.0),
            Point2::new(10.0, 10.0),
        ])
        .unwrap();
        for index in 0..100 {
            let u = f64::from(index % 10) / 10.0;
            let v = f64::from(index / 10) / 10.0;
            let point = quad.map(u, v);
            assert!((point.x - u * 10.0).abs() < 1e-9);
            assert!((point.y - v * 10.0).abs() < 1e-9);
        }
    }

    #[test]
    fn quad_corners_map_to_corners() {
        let quad = QuadWarp::new([
            Point2::new(1.0, 2.0),
            Point2::new(11.0, 2.0),
            Point2::new(2.0, 12.0),
            Point2::new(12.0, 12.0),
        ])
        .unwrap();
        assert_eq!(quad.map(0.0, 0.0), Point2::new(1.0, 2.0));
        assert_eq!(quad.map(1.0, 0.0), Point2::new(11.0, 2.0));
        assert_eq!(quad.map(0.0, 1.0), Point2::new(2.0, 12.0));
        assert_eq!(quad.map(1.0, 1.0), Point2::new(12.0, 12.0));
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn lattice_identity_and_nodes() {
        let mut nodes = [[Point2::origin(); 4]; 4];
        for row in 0..4 {
            for col in 0..4 {
                nodes[row][col] = Point2::new(col as f64 * 10.0 / 3.0, row as f64 * 10.0 / 3.0);
            }
        }
        let lattice = LatticeWarp::new(nodes).unwrap();
        for row in 0..4 {
            for col in 0..4 {
                let point = lattice.map(col as f64 / 3.0, row as f64 / 3.0);
                assert!((point.x - nodes[row][col].x).abs() < 1e-9);
                assert!((point.y - nodes[row][col].y).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn taper_builds_a_trapezoid() {
        let spec = TaperSpec {
            top_width: 20.0,
            bottom_width: 40.0,
            length: 50.0,
            orientation: TaperOrientation::Top,
            diameters: false,
            wrap_fraction: 100.0,
        };
        let quad = spec.quad(domain()).unwrap();
        assert!((quad.corners[0].x - 30.0).abs() < 1e-9);
        assert!((quad.corners[1].x - 50.0).abs() < 1e-9);
        assert!((quad.corners[2].x - 40.0).abs() < 1e-9);
        assert!((quad.corners[3].x - 60.0).abs() < 1e-9);
    }

    #[test]
    fn warp_keeps_a_closed_path_closed() {
        let path = Path::from_points(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], true);
        let quad = QuadWarp::new([
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 1.0),
            Point2::new(0.0, 10.0),
            Point2::new(10.0, 11.0),
        ])
        .unwrap();
        let warped = warp_path(&path, domain(), |u, v| quad.map(u, v)).unwrap();
        assert!(warped.closed);
        assert_eq!(warped.to_points().first(), warped.to_points().last());
    }
}
