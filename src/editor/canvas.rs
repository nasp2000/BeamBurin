use crate::geometry::Point2;

/// Canvas view transform.
///
/// Document coordinates are millimetres with the origin at the bottom-left
/// (`y` up). Screen coordinates are egui points with `y` down. `pan_x`/`pan_y`
/// are the screen position of the document origin relative to the canvas
/// rectangle's top-left corner, and `zoom` is pixels per millimetre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasState {
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub grid_visible: bool,
    pub grid_spacing_mm: f64,
}

pub const MIN_ZOOM: f64 = 0.05;
pub const MAX_ZOOM: f64 = 64.0;

/// Zoom change per scroll point, before the step preference is applied.
///
/// The wheel is continuous: a notch arrives as several small scroll events, so
/// every event only nudges the zoom instead of jumping a fixed step.
const WHEEL_ZOOM_RATE: f64 = 0.0022;

/// The factor the sensitivity preference refers to (the historic default).
pub const DEFAULT_SCROLL_ZOOM_STEP: f64 = 1.15;

/// Largest zoom change one wheel event may apply.
const WHEEL_ZOOM_FACTOR_LIMIT: f64 = 1.35;

/// Zoom factor for one wheel event with `scroll_points` of vertical scroll.
///
/// `step` is the sensitivity the preferences expose; it only scales the rate,
/// so the default stays the familiar one and each event remains a small nudge.
/// A zero or non-finite scroll leaves the zoom untouched.
pub fn wheel_zoom_factor(scroll_points: f64, step: f64) -> f64 {
    if !scroll_points.is_finite() || scroll_points == 0.0 {
        return 1.0;
    }
    let step = if step.is_finite() && step > 1.0 {
        step
    } else {
        DEFAULT_SCROLL_ZOOM_STEP
    };
    // Express the multiplicative preference as a relative rate.
    let sensitivity = step.ln() / DEFAULT_SCROLL_ZOOM_STEP.ln();
    let rate = WHEEL_ZOOM_RATE * sensitivity.clamp(0.25, 3.0);
    (scroll_points * rate)
        .exp()
        .clamp(1.0 / WHEEL_ZOOM_FACTOR_LIMIT, WHEEL_ZOOM_FACTOR_LIMIT)
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            grid_visible: true,
            grid_spacing_mm: 10.0,
        }
    }
}

impl CanvasState {
    pub fn zoom_in(&mut self) {
        self.zoom = (self.zoom * 1.25).min(MAX_ZOOM);
    }

    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.25).max(MIN_ZOOM);
    }

    pub fn reset_zoom(&mut self) {
        self.zoom = 1.0;
    }

    /// Set the zoom from a toolbar percentage, clamped to the canvas limits.
    pub fn set_zoom_percent(&mut self, percent: f64) {
        if percent.is_finite() {
            self.zoom = (percent / 100.0).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }

    /// Set the percentage while keeping the document point under
    /// `(anchor_x, anchor_y)` in place.
    pub fn set_zoom_percent_at(&mut self, percent: f64, anchor_x: f64, anchor_y: f64) {
        if percent.is_finite() {
            let target = (percent / 100.0).clamp(MIN_ZOOM, MAX_ZOOM);
            let factor = target / self.zoom.max(f64::MIN_POSITIVE);
            self.zoom_at(anchor_x, anchor_y, factor);
        }
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    pub fn reset_pan(&mut self) {
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    /// Zoom while keeping the document point under `(anchor_x, anchor_y)`
    /// fixed on screen.
    pub fn zoom_at(&mut self, anchor_x: f64, anchor_y: f64, factor: f64) {
        let (doc_x, doc_y) = self.screen_to_doc(anchor_x, anchor_y);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan_x = anchor_x - doc_x * self.zoom;
        self.pan_y = anchor_y + doc_y * self.zoom;
    }

    /// Fit a document page into a viewport, leaving a margin in pixels.
    pub fn fit(
        &mut self,
        width_mm: f64,
        height_mm: f64,
        viewport_width: f64,
        viewport_height: f64,
        margin: f64,
    ) {
        let width_mm = width_mm.max(0.001);
        let height_mm = height_mm.max(0.001);
        let available_width = (viewport_width - 2.0 * margin).max(1.0);
        let available_height = (viewport_height - 2.0 * margin).max(1.0);
        self.zoom = (available_width / width_mm)
            .min(available_height / height_mm)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan_x = (viewport_width - width_mm * self.zoom) / 2.0;
        self.pan_y = f64::midpoint(viewport_height, height_mm * self.zoom);
    }

    /// Fit a document rectangle into the viewport, centred.
    pub fn fit_bounds(
        &mut self,
        min: Point2,
        max: Point2,
        viewport_width: f64,
        viewport_height: f64,
        margin: f64,
    ) {
        let width = (max.x - min.x).max(0.001);
        let height = (max.y - min.y).max(0.001);
        let available_width = (viewport_width - 2.0 * margin).max(1.0);
        let available_height = (viewport_height - 2.0 * margin).max(1.0);
        self.zoom = (available_width / width)
            .min(available_height / height)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        let mid_x = f64::midpoint(min.x, max.x);
        let mid_y = f64::midpoint(min.y, max.y);
        self.pan_x = f64::midpoint(viewport_width, 0.0) - mid_x * self.zoom;
        self.pan_y = f64::midpoint(viewport_height, 0.0) + mid_y * self.zoom;
    }

    pub fn screen_to_doc(&self, screen_x: f64, screen_y: f64) -> (f64, f64) {
        (
            (screen_x - self.pan_x) / self.zoom,
            (self.pan_y - screen_y) / self.zoom,
        )
    }

    pub fn doc_to_screen(&self, doc_x: f64, doc_y: f64) -> (f64, f64) {
        (
            self.pan_x + doc_x * self.zoom,
            self.pan_y - doc_y * self.zoom,
        )
    }

    /// Grid spacing in millimetres that stays legible at the current zoom.
    pub fn effective_grid_spacing(&self) -> f64 {
        let base = if self.grid_spacing_mm > 0.0 {
            self.grid_spacing_mm
        } else {
            10.0
        };
        let mut spacing = base;
        while spacing * self.zoom < 8.0 {
            spacing *= 2.0;
        }
        while spacing * self.zoom > 80.0 {
            spacing /= 2.0;
        }
        spacing
    }
}

#[cfg(test)]
mod tests {
    use super::CanvasState;

    const EPS: f64 = 1e-9;

    #[test]
    fn round_trip_coordinates() {
        let canvas = CanvasState {
            zoom: 2.0,
            pan_x: 40.0,
            pan_y: 300.0,
            ..CanvasState::default()
        };
        let (sx, sy) = canvas.doc_to_screen(10.0, 20.0);
        let (dx, dy) = canvas.screen_to_doc(sx, sy);
        assert!((dx - 10.0).abs() < EPS);
        assert!((dy - 20.0).abs() < EPS);
    }

    #[test]
    fn y_axis_points_up() {
        let canvas = CanvasState {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 100.0,
            ..CanvasState::default()
        };
        let (_, low) = canvas.doc_to_screen(0.0, 10.0);
        let (_, high) = canvas.doc_to_screen(0.0, 50.0);
        assert!(high < low);
    }

    #[test]
    fn fit_centres_page() {
        let mut canvas = CanvasState::default();
        canvas.fit(100.0, 50.0, 400.0, 300.0, 20.0);
        assert!((canvas.zoom - 3.6).abs() < EPS);
        assert!((canvas.pan_x - 20.0).abs() < EPS);
        assert!((canvas.pan_y - 240.0).abs() < EPS);
    }

    #[test]
    fn zoom_at_keeps_anchor_fixed() {
        let mut canvas = CanvasState {
            pan_x: 30.0,
            pan_y: 200.0,
            zoom: 2.0,
            ..CanvasState::default()
        };
        let (before_x, before_y) = canvas.screen_to_doc(100.0, 120.0);
        canvas.zoom_at(100.0, 120.0, 1.5);
        let (after_x, after_y) = canvas.screen_to_doc(100.0, 120.0);
        assert!((after_x - before_x).abs() < 1e-9);
        assert!((after_y - before_y).abs() < 1e-9);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut canvas = CanvasState::default();
        for _ in 0..100 {
            canvas.zoom_in();
        }
        assert!(canvas.zoom <= super::MAX_ZOOM);
        for _ in 0..200 {
            canvas.zoom_out();
        }
        assert!(canvas.zoom >= super::MIN_ZOOM);
    }

    #[test]
    fn wheel_zoom_is_gradual_and_symmetric() {
        let step = super::DEFAULT_SCROLL_ZOOM_STEP;
        // A wheel notch is a handful of points, never a fixed 15% jump.
        let notch = super::wheel_zoom_factor(50.0, step);
        assert!(
            (1.02..1.12).contains(&notch),
            "one notch should nudge the zoom, got {notch}"
        );
        // Small deltas nudge proportionally; scrolling back undoes the zoom.
        let small = super::wheel_zoom_factor(5.0, step);
        assert!(small > 1.0 && small < notch, "small delta: {small}");
        let there = super::wheel_zoom_factor(37.0, step);
        let back = super::wheel_zoom_factor(-37.0, step);
        assert!((there * back - 1.0).abs() < 1e-9, "{there} * {back}");
        // Nothing to scroll, nothing to change.
        assert!((super::wheel_zoom_factor(0.0, step) - 1.0).abs() < 1e-12);
        assert!((super::wheel_zoom_factor(f64::NAN, step) - 1.0).abs() < 1e-12);
        // A huge delta in one event still stays a nudge.
        let huge = super::wheel_zoom_factor(10_000.0, step);
        assert!(huge <= 1.35 + 1e-9, "a single event jumped: {huge}");
        // The preference scales sensitivity instead of replacing the step.
        let fast = super::wheel_zoom_factor(50.0, 1.4);
        assert!(fast > notch, "higher sensitivity must react more: {fast}");
    }

    #[test]
    fn toolbar_percentage_sets_and_clamps_the_zoom() {
        let mut canvas = CanvasState::default();
        canvas.set_zoom_percent(250.0);
        assert!((canvas.zoom - 2.5).abs() < EPS);
        canvas.set_zoom_percent(1.0);
        assert!((canvas.zoom - super::MIN_ZOOM).abs() < EPS);
        canvas.set_zoom_percent(100_000.0);
        assert!((canvas.zoom - super::MAX_ZOOM).abs() < EPS);
        canvas.set_zoom_percent(f64::NAN);
        assert!((canvas.zoom - super::MAX_ZOOM).abs() < EPS);
    }

    #[test]
    fn toolbar_percentage_keeps_the_anchor() {
        let mut canvas = CanvasState {
            pan_x: 30.0,
            pan_y: 200.0,
            zoom: 1.0,
            ..CanvasState::default()
        };
        let (before_x, before_y) = canvas.screen_to_doc(140.0, 90.0);
        canvas.set_zoom_percent_at(300.0, 140.0, 90.0);
        assert!((canvas.zoom - 3.0).abs() < EPS);
        let (after_x, after_y) = canvas.screen_to_doc(140.0, 90.0);
        assert!((after_x - before_x).abs() < 1e-9, "{before_x} -> {after_x}");
        assert!((after_y - before_y).abs() < 1e-9, "{before_y} -> {after_y}");
    }

    #[test]
    fn grid_spacing_adapts_to_zoom() {
        let mut canvas = CanvasState {
            zoom: 0.1,
            ..CanvasState::default()
        };
        assert!(canvas.effective_grid_spacing() * canvas.zoom >= 8.0);
        canvas.zoom = 20.0;
        assert!(canvas.effective_grid_spacing() * canvas.zoom <= 80.0);
    }
}
