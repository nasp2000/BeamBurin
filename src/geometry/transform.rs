use serde::{Deserialize, Serialize};

use super::Point2;

/// 2D affine transform (matrix + translation).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2 {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for Transform2 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform2 {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn translate(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx,
            ty,
        }
    }
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn scale_uniform(s: f64) -> Self {
        Self::scale(s, s)
    }
    pub fn scale_around(sx: f64, sy: f64, cx: f64, cy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: cx * (1.0 - sx),
            ty: cy * (1.0 - sy),
        }
    }
    pub fn rotate_around(angle_rad: f64, cx: f64, cy: f64) -> Self {
        let t = Self::rotate(angle_rad);
        // translate to origin, rotate, translate back
        Self {
            a: t.a,
            b: t.b,
            c: t.c,
            d: t.d,
            tx: cx - t.a * cx - t.c * cy,
            ty: cy - t.b * cx - t.d * cy,
        }
    }
    pub fn rotate(angle_rad: f64) -> Self {
        let (s, c) = (angle_rad.sin(), angle_rad.cos());
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn mirror_x() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn mirror_y() -> Self {
        Self {
            a: -1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }
    pub fn compose(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }
    /// Reflection across the line through `a` and `b`.
    ///
    /// Returns `None` for coincident or non-finite points.
    pub fn reflect_across_line(a: Point2, b: Point2) -> Option<Self> {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let length = dx.hypot(dy);
        if !length.is_finite() || length < 1e-12 {
            return None;
        }
        let nx = dx / length;
        let ny = dy / length;
        let a11 = 2.0 * nx * nx - 1.0;
        let a12 = 2.0 * nx * ny;
        let a22 = 2.0 * ny * ny - 1.0;
        let transform = Self {
            a: a11,
            b: a12,
            c: a12,
            d: a22,
            tx: a.x - (a11 * a.x + a12 * a.y),
            ty: a.y - (a12 * a.x + a22 * a.y),
        };
        transform.is_finite().then_some(transform)
    }

    /// Uniform scale and rotation around `pivot`.
    pub fn similarity_about(pivot: Point2, angle_rad: f64, scale: f64) -> Option<Self> {
        if !angle_rad.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
            || !pivot.x.is_finite()
            || !pivot.y.is_finite()
        {
            return None;
        }
        let transform = Self::translate(pivot.x, pivot.y)
            .compose(&Self::rotate(angle_rad).compose(&Self::scale_uniform(scale)))
            .compose(&Self::translate(-pivot.x, -pivot.y));
        transform.is_finite().then_some(transform)
    }

    /// Linear determinant; negative after a reflection.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn invert(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return None;
        }
        Some(Self {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            tx: (self.c * self.ty - self.d * self.tx) / det,
            ty: (self.b * self.tx - self.a * self.ty) / det,
        })
    }
    pub fn apply_to_points(&self, pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
        pts.iter()
            .map(|(x, y)| {
                let p = self.apply(Point2::new(*x, *y));
                (p.x, p.y)
            })
            .collect()
    }
    pub fn apply(&self, p: Point2) -> Point2 {
        Point2 {
            x: self.a * p.x + self.c * p.y + self.tx,
            y: self.b * p.x + self.d * p.y + self.ty,
        }
    }
    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|v| v.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::{Point2, Transform2};

    const EPS: f64 = 1e-9;

    #[test]
    fn identity() {
        let t = Transform2::identity();
        let p = Point2::new(3.0, 4.0);
        let r = t.apply(p);
        assert!((r.x - 3.0).abs() < EPS);
        assert!((r.y - 4.0).abs() < EPS);
    }

    #[test]
    fn translate() {
        let t = Transform2::translate(10.0, -5.0);
        let p = Point2::new(1.0, 2.0);
        let r = t.apply(p);
        assert!((r.x - 11.0).abs() < EPS);
        assert!((r.y - (-3.0)).abs() < EPS);
    }

    #[test]
    fn scale_via_matrix() {
        let t = Transform2 {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            tx: 0.0,
            ty: 0.0,
        };
        let r = t.apply(Point2::new(3.0, 4.0));
        assert!((r.x - 6.0).abs() < EPS);
        assert!((r.y - 8.0).abs() < EPS);
    }

    #[test]
    fn scale_around() {
        let t = Transform2::scale_around(2.0, 2.0, 5.0, 5.0);
        let r = t.apply(Point2::new(5.0, 5.0));
        assert!((r.x - 5.0).abs() < 1e-9);
        assert!((r.y - 5.0).abs() < 1e-9);
        let r2 = t.apply(Point2::new(6.0, 5.0));
        assert!((r2.x - 7.0).abs() < 1e-9);
    }

    #[test]
    fn rotate_around() {
        let t = Transform2::rotate_around(std::f64::consts::FRAC_PI_2, 0.0, 0.0);
        let r = t.apply(Point2::new(1.0, 0.0));
        assert!(r.x.abs() < 1e-9);
        assert!((r.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn scale_uniform() {
        let t = Transform2::scale_uniform(2.0);
        let r = t.apply(Point2::new(3.0, 4.0));
        assert!((r.x - 6.0).abs() < 1e-9);
        assert!((r.y - 8.0).abs() < 1e-9);
    }

    #[test]
    fn scale_factory() {
        let t = Transform2::scale(2.0, 3.0);
        let r = t.apply(Point2::new(1.0, 1.0));
        assert!((r.x - 2.0).abs() < EPS);
        assert!((r.y - 3.0).abs() < EPS);
    }

    #[test]
    fn rotate_factory() {
        let t = Transform2::rotate(std::f64::consts::FRAC_PI_2);
        let r = t.apply(Point2::new(1.0, 0.0));
        assert!(r.x.abs() < 1e-9);
        assert!((r.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn mirror_x() {
        let t = Transform2::mirror_x();
        let r = t.apply(Point2::new(1.0, 2.0));
        assert!((r.x - 1.0).abs() < 1e-9);
        assert!((r.y + 2.0).abs() < 1e-9);
    }

    #[test]
    fn mirror_y() {
        let t = Transform2::mirror_y();
        let r = t.apply(Point2::new(1.0, 2.0));
        assert!((r.x + 1.0).abs() < 1e-9);
        assert!((r.y - 2.0).abs() < 1e-9);
    }

    #[test]
    fn invert_identity() {
        let t = Transform2::identity();
        let inv = t.invert().unwrap();
        let r = inv.apply(Point2::new(3.0, 4.0));
        assert!((r.x - 3.0).abs() < 1e-9);
        assert!((r.y - 4.0).abs() < 1e-9);
    }

    #[test]
    fn invert_singular() {
        let t = Transform2 {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            tx: 0.0,
            ty: 0.0,
        };
        assert!(t.invert().is_none());
    }

    #[test]
    fn apply_to_points() {
        let t = Transform2::translate(1.0, 2.0);
        let r = t.apply_to_points(&[(0.0, 0.0), (1.0, 1.0)]);
        assert_eq!(r, vec![(1.0, 2.0), (2.0, 3.0)]);
    }

    #[test]
    fn compose_translate() {
        let t = Transform2::translate(1.0, 2.0).compose(&Transform2::translate(3.0, 4.0));
        let r = t.apply(Point2::origin());
        assert!((r.x - 4.0).abs() < 1e-9);
        assert!((r.y - 6.0).abs() < 1e-9);
    }
}

/// Mirror a list of points across a vertical axis x = mx.
pub fn mirror_points_vertical(pts: &[(f64, f64)], mx: f64) -> Vec<(f64, f64)> {
    pts.iter().map(|(x, y)| (2.0 * mx - x, *y)).collect()
}

/// Mirror a list of points across a horizontal axis y = my.
pub fn mirror_points_horizontal(pts: &[(f64, f64)], my: f64) -> Vec<(f64, f64)> {
    pts.iter().map(|(x, y)| (*x, 2.0 * my - y)).collect()
}

#[cfg(test)]
mod mirror_tests {
    use super::*;

    #[test]
    fn mirror_vertical() {
        let pts = [(0.0, 0.0), (2.0, 3.0)];
        let r = mirror_points_vertical(&pts, 1.0);
        assert_eq!(r, vec![(2.0, 0.0), (0.0, 3.0)]);
    }

    #[test]
    fn mirror_horizontal() {
        let pts = [(0.0, 0.0), (2.0, 3.0)];
        let r = mirror_points_horizontal(&pts, 1.0);
        assert_eq!(r, vec![(0.0, 2.0), (2.0, -1.0)]);
    }

    #[test]
    fn mirror_empty() {
        assert_eq!(mirror_points_vertical(&[], 0.0), [] as [(f64, f64); 0]);
        assert_eq!(mirror_points_horizontal(&[], 0.0), [] as [(f64, f64); 0]);
    }
}

#[cfg(test)]
mod mirror_tests {
    use super::*;

    fn point(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn reflect_across_vertical_line() {
        let t = Transform2::reflect_across_line(point(5.0, 0.0), point(5.0, 10.0)).unwrap();
        let p = t.apply(point(2.0, 3.0));
        assert!((p.x - 8.0).abs() < 1e-9 && (p.y - 3.0).abs() < 1e-9);
    }

    #[test]
    fn reflect_across_horizontal_line() {
        let t = Transform2::reflect_across_line(point(0.0, 2.0), point(10.0, 2.0)).unwrap();
        let p = t.apply(point(2.0, 3.0));
        assert!((p.x - 2.0).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn reflection_is_an_involution_and_has_determinant_minus_one() {
        let t = Transform2::reflect_across_line(point(1.0, 1.0), point(4.0, 5.0)).unwrap();
        let p = point(7.0, -2.0);
        let again = t.apply(t.apply(p));
        assert!((again.x - p.x).abs() < 1e-9 && (again.y - p.y).abs() < 1e-9);
        assert!((t.determinant() + 1.0).abs() < 1e-9);
    }

    #[test]
    fn reflection_rejects_coincident_points() {
        assert!(Transform2::reflect_across_line(point(1.0, 1.0), point(1.0, 1.0)).is_none());
    }

    #[test]
    fn similarity_fixes_the_pivot_and_scales_by_squared_scale() {
        let pivot = point(3.0, 4.0);
        let t = Transform2::similarity_about(pivot, 0.7, 2.0).unwrap();
        let fixed = t.apply(pivot);
        assert!((fixed.x - pivot.x).abs() < 1e-9 && (fixed.y - pivot.y).abs() < 1e-9);
        assert!((t.determinant() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn similarity_maps_the_reference_vector_to_the_target() {
        let pivot = point(1.0, 2.0);
        let from = point(3.0, 2.0);
        let to = point(1.0, 5.0);
        let angle =
            (to.y - pivot.y).atan2(to.x - pivot.x) - (from.y - pivot.y).atan2(from.x - pivot.x);
        let scale =
            ((to.x - pivot.x).hypot(to.y - pivot.y)) / ((from.x - pivot.x).hypot(from.y - pivot.y));
        let t = Transform2::similarity_about(pivot, angle, scale).unwrap();
        let mapped = t.apply(from);
        assert!((mapped.x - to.x).abs() < 1e-9 && (mapped.y - to.y).abs() < 1e-9);
    }
}
