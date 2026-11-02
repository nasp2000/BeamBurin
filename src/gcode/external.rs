#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

//! Read an existing G-code file for inspection.
//!
//! This is a viewer, not a simulator: it understands the common motion, unit,
//! positioning, feed, power and laser-mode words, reports what it cannot draw
//! and never writes, sends or runs anything. Nothing here talks to a machine.

use std::collections::HashSet;
use std::f64::consts::{PI, TAU};

use crate::geometry::Point2;

/// Largest file accepted by [`parse_file`], in bytes.
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;

/// Largest angle between two samples of a previewed arc, in radians.
const ARC_STEP_RAD: f64 = 0.05;

/// Upper bound on the samples generated for one arc.
const MAX_ARC_SAMPLES: usize = 2048;

/// Largest mismatch, in millimetres, between the two ends of an arc's radius.
const ARC_RADIUS_TOLERANCE_MM: f64 = 0.05;

/// What the file contains, in millimetres.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Program {
    /// Polylines drawn with the laser on.
    pub cuts: Vec<Vec<Point2>>,
    /// Polylines drawn with the laser off.
    pub travels: Vec<Vec<Point2>>,
    pub cut_length_mm: f64,
    pub travel_length_mm: f64,
    /// Estimate from the feed rates in the file; `None` when no move had one.
    pub estimated_seconds: Option<f64>,
    pub power_min: Option<f64>,
    pub power_max: Option<f64>,
    pub feed_min: Option<f64>,
    pub feed_max: Option<f64>,
    pub lines: usize,
    pub warnings: Vec<String>,
}

impl Program {
    /// Bounding box of every point, in millimetres.
    pub fn bounds(&self) -> Option<(Point2, Point2)> {
        let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut seen = false;
        for point in self.cuts.iter().chain(&self.travels).flatten() {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            seen = true;
        }
        seen.then_some((min, max))
    }

    pub fn cut_points(&self) -> usize {
        self.cuts.iter().map(Vec::len).sum()
    }
}

/// Parse a G-code file's text. Never fails: problems become warnings.
pub fn parse(text: &str) -> Program {
    let mut parser = Parser::default();
    for line in text.lines() {
        parser.line = parser.line.saturating_add(1);
        let stripped = strip_comments(line);
        if stripped.trim().is_empty() {
            continue;
        }
        parser.read_line(&stripped);
    }
    parser.finish()
}

/// Read and parse a G-code file.
pub fn parse_file(path: &std::path::Path) -> Result<Program, String> {
    let metadata = std::fs::metadata(path).map_err(|error| format!("cannot read file: {error}"))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!(
            "the file is larger than the {} MiB limit",
            MAX_FILE_BYTES / (1024 * 1024)
        ));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read the file as text: {error}"))?;
    let mut program = parse(&text);
    if program.lines == 0 {
        program.warnings.push("the file is empty".to_string());
    }
    Ok(program)
}

/// Drop `( ... )` and `; ...` comments.
fn strip_comments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_comment = false;
    for character in line.chars() {
        if in_comment {
            if character == ')' {
                in_comment = false;
            }
            continue;
        }
        match character {
            ';' => break,
            '(' => in_comment = true,
            _ => out.push(character),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Cut,
    Travel,
}

struct Parser {
    program: Program,
    position: Point2,
    absolute: bool,
    /// Millimetres per program unit.
    scale: f64,
    feed: Option<f64>,
    power: Option<f64>,
    laser_on: bool,
    mode: Option<Mode>,
    current: Vec<Point2>,
    missing_feed: bool,
    rated: HashSet<String>,
    warned: HashSet<String>,
    line: usize,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            program: Program::default(),
            position: Point2::origin(),
            absolute: true,
            scale: 1.0,
            feed: None,
            power: None,
            laser_on: false,
            mode: None,
            current: Vec::new(),
            missing_feed: false,
            rated: HashSet::new(),
            warned: HashSet::new(),
            line: 0,
        }
    }
}

impl Parser {
    fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        if self.warned.insert(message.clone()) {
            self.program.warnings.push(message);
        }
    }

    fn read_line(&mut self, line: &str) {
        let words = tokens(line);
        if words.is_empty() {
            return;
        }
        let mut target = self.position;
        let mut has_axis = false;
        let mut motion: Option<u32> = None;
        let mut arc_i: Option<f64> = None;
        let mut arc_j: Option<f64> = None;
        let mut arc_r: Option<f64> = None;
        for (letter, value) in &words {
            match letter {
                'G' => {
                    let code = *value as u32;
                    match code {
                        0..=3 => motion = Some(code),
                        4 | 17 | 40 | 49 => {}
                        20 => self.scale = 25.4,
                        21 => self.scale = 1.0,
                        90 => self.absolute = true,
                        91 => self.absolute = false,
                        54..=59 => self.warn(
                            "work offsets (G54–G59) are not applied; coordinates are read as written",
                        ),
                        92 => self.warn("G92 offsets are not applied"),
                        _ => self.warn(format!("G{code} is not interpreted")),
                    }
                }
                'M' => match *value as u32 {
                    3 | 4 => self.laser_on = true,
                    5 => self.laser_on = false,
                    8 | 9 => {}
                    code => self.warn(format!("M{code} is ignored")),
                },
                'F' => {
                    if *value > 0.0 {
                        self.feed = Some(*value * self.scale);
                        let feed = *value * self.scale;
                        self.program.feed_min =
                            Some(self.program.feed_min.map_or(feed, |min| min.min(feed)));
                        self.program.feed_max =
                            Some(self.program.feed_max.map_or(feed, |max| max.max(feed)));
                    }
                }
                'S' => self.power = Some(*value),
                'X' => {
                    target.x = self.axis(*value, self.position.x);
                    has_axis = true;
                }
                'Y' => {
                    target.y = self.axis(*value, self.position.y);
                    has_axis = true;
                }
                'I' => arc_i = Some(*value * self.scale),
                'J' => arc_j = Some(*value * self.scale),
                'R' => arc_r = Some(*value * self.scale),
                'Z' | 'K' | 'P' | 'Q' | 'N' | 'O' => {}
                other => self.warn(format!("parameter {other} is ignored")),
            }
        }
        let Some(code) = motion else {
            return;
        };
        if code == 2 || code == 3 {
            self.arc(code == 2, target, arc_i, arc_j, arc_r);
            return;
        }
        if !has_axis {
            return;
        }
        let cutting = code == 1 && self.laser_on;
        let mode = if cutting { Mode::Cut } else { Mode::Travel };
        self.add_move(target, mode);
    }

    /// Draw a `G2`/`G3` arc as a polyline.
    ///
    /// `I`/`J` are centre offsets from the start, which is the common output;
    /// a signed `R` is accepted too. A malformed arc is reported and drawn as
    /// a straight move instead of being dropped, so the preview never hides
    /// geometry that the file asked for.
    fn arc(
        &mut self,
        clockwise: bool,
        target: Point2,
        i: Option<f64>,
        j: Option<f64>,
        r: Option<f64>,
    ) {
        let start = self.position;
        let mode = if self.laser_on {
            Mode::Cut
        } else {
            Mode::Travel
        };
        let (center, radius) = if i.is_some() || j.is_some() {
            let center = Point2::new(start.x + i.unwrap_or(0.0), start.y + j.unwrap_or(0.0));
            let radius = start.distance(center);
            (center, radius)
        } else if let Some(radius) = r {
            let Some(center) = center_from_radius(start, target, radius, clockwise) else {
                self.warn(
                    "an arc radius cannot reach its end point; it is drawn as a straight move",
                );
                self.add_move(target, mode);
                return;
            };
            (center, radius.abs())
        } else {
            self.warn("an arc without I, J or R is drawn as a straight move");
            self.add_move(target, mode);
            return;
        };
        if radius <= 1e-9 {
            self.warn("an arc with a zero radius is drawn as a straight move");
            self.add_move(target, mode);
            return;
        }
        if (target.distance(center) - radius).abs() > ARC_RADIUS_TOLERANCE_MM {
            self.warn("an arc end point is not on its circle; it is drawn as a straight move");
            self.add_move(target, mode);
            return;
        }
        let start_angle = (start.y - center.y).atan2(start.x - center.x);
        let mut sweep = sweep_between(start, target, center, clockwise);
        if start.distance(target) <= 1e-9 {
            // A full circle: start and end are the same point.
            sweep = TAU;
        }
        if !sweep.is_finite() || sweep <= 0.0 {
            self.warn("an arc has no usable sweep; it is drawn as a straight move");
            self.add_move(target, mode);
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((sweep / ARC_STEP_RAD).ceil() as usize).clamp(1, MAX_ARC_SAMPLES);
        let direction = if clockwise { -1.0 } else { 1.0 };
        for step in 1..=steps {
            if step == steps {
                self.add_move(target, mode);
            } else {
                #[allow(clippy::cast_precision_loss)]
                let fraction = step as f64 / steps as f64;
                let angle = start_angle + direction * sweep * fraction;
                let point = Point2::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                );
                self.add_move(point, mode);
            }
        }
    }

    fn axis(&self, value: f64, current: f64) -> f64 {
        if self.absolute {
            value * self.scale
        } else {
            current + value * self.scale
        }
    }

    fn add_move(&mut self, to: Point2, mode: Mode) {
        let from = self.position;
        self.position = to;
        let length = from.distance(to);
        if length <= 1e-12 && self.current.is_empty() {
            return;
        }
        if self.mode != Some(mode) {
            self.flush();
            self.mode = Some(mode);
            self.current.push(from);
        }
        self.current.push(to);
        match mode {
            Mode::Cut => {
                self.program.cut_length_mm += length;
                if let Some(power) = self.power {
                    self.program.power_min = Some(
                        self.program
                            .power_min
                            .map_or(power, |minimum| minimum.min(power)),
                    );
                    self.program.power_max = Some(
                        self.program
                            .power_max
                            .map_or(power, |maximum| maximum.max(power)),
                    );
                }
            }
            Mode::Travel => self.program.travel_length_mm += length,
        }
        let seconds = match self.feed {
            Some(feed) if feed > 0.0 => length * 60.0 / feed,
            _ => {
                self.missing_feed = true;
                0.0
            }
        };
        *self.program.estimated_seconds.get_or_insert(0.0) += seconds;
    }

    fn flush(&mut self) {
        if self.current.len() < 2 {
            self.current.clear();
            return;
        }
        let points = std::mem::take(&mut self.current);
        match self.mode {
            Some(Mode::Cut) => self.program.cuts.push(points),
            Some(Mode::Travel) => self.program.travels.push(points),
            None => {}
        }
    }

    fn finish(mut self) -> Program {
        self.flush();
        if self.missing_feed {
            self.program.warnings.push(
                "some moves have no feed rate; the time estimate leaves them out".to_string(),
            );
        }
        if self.program.cuts.is_empty() && self.program.travels.is_empty() {
            self.program
                .warnings
                .push("no motion was found in the file".to_string());
        }
        if self.program.estimated_seconds == Some(0.0) {
            self.program.estimated_seconds = None;
        }
        self.program
    }
}

/// Angle travelled from `start` to `end` around `center`, in `(0, 2π]`.
fn sweep_between(start: Point2, end: Point2, center: Point2, clockwise: bool) -> f64 {
    let start_angle = (start.y - center.y).atan2(start.x - center.x);
    let end_angle = (end.y - center.y).atan2(end.x - center.x);
    let mut sweep = if clockwise {
        start_angle - end_angle
    } else {
        end_angle - start_angle
    };
    while sweep <= 0.0 {
        sweep += TAU;
    }
    sweep
}

/// Centre of a radius-format arc, choosing the minor arc for a positive `R`
/// and the major arc for a negative one.
fn center_from_radius(start: Point2, end: Point2, radius: f64, clockwise: bool) -> Option<Point2> {
    let r = radius.abs();
    let chord = start.distance(end);
    if chord <= 1e-9 || chord > 2.0 * r + 1e-9 {
        return None;
    }
    let middle = Point2::new(f64::midpoint(start.x, end.x), f64::midpoint(start.y, end.y));
    let height = (r * r - (chord * 0.5).powi(2)).max(0.0).sqrt();
    let dx = (end.x - start.x) / chord;
    let dy = (end.y - start.y) / chord;
    let candidates = [
        Point2::new(middle.x - dy * height, middle.y + dx * height),
        Point2::new(middle.x + dy * height, middle.y - dx * height),
    ];
    let wanted_minor = radius > 0.0;
    let mut fallback = None;
    for center in candidates {
        let sweep = sweep_between(start, end, center, clockwise);
        if (sweep <= PI + 1e-9) == wanted_minor {
            return Some(center);
        }
        fallback = Some(center);
    }
    fallback
}

/// Split a line into letter/number words, tolerating missing spaces.
fn tokens(line: &str) -> Vec<(char, f64)> {
    let mut words = Vec::new();
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if !character.is_ascii_alphabetic() {
            continue;
        }
        let letter = character.to_ascii_uppercase();
        let mut number = String::new();
        while let Some(next) = chars.peek() {
            if next.is_ascii_digit() || matches!(next, '.' | '-' | '+') {
                number.push(*next);
                chars.next();
            } else {
                break;
            }
        }
        if let Ok(value) = number.parse::<f64>() {
            if value.is_finite() {
                words.push((letter, value));
            }
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::{Program, parse, tokens};
    use crate::geometry::Point2;

    fn close(value: f64, expected: f64) -> bool {
        (value - expected).abs() < 1e-9
    }

    #[test]
    fn a_simple_cut_keeps_its_length_time_and_power() {
        let program = parse("(header)\nG21\nG90\nG0 X0 Y0\nM3\nS500\nF1000\nG1 X10 Y0\nM5\n");
        assert_eq!(program.cuts.len(), 1);
        assert_eq!(program.cuts[0].len(), 2);
        assert!(close(program.cut_length_mm, 10.0));
        assert_eq!(program.power_min, Some(500.0));
        assert_eq!(program.power_max, Some(500.0));
        assert_eq!(program.feed_min, Some(1000.0));
        // 10 mm at 1000 mm/min is 0.6 s.
        assert!((program.estimated_seconds.unwrap() - 0.6).abs() < 1e-9);
    }

    #[test]
    fn inches_are_converted_to_millimetres() {
        let program = parse("G20\nG90\nM3\nG1 X1 Y0\nM5\n");
        assert!(close(program.cut_length_mm, 25.4));
        let (min, max) = program.bounds().unwrap();
        assert!(close(min.x, 0.0));
        assert!(close(max.x, 25.4));
    }

    #[test]
    fn relative_positioning_accumulates() {
        let program = parse("G91\nM3\nG1 X10 Y0\nG1 X0 Y5\nM5\n");
        assert_eq!(program.cuts.len(), 1);
        let last = *program.cuts[0].last().unwrap();
        assert!(close(last.x, 10.0));
        assert!(close(last.y, 5.0));
        assert!(close(program.cut_length_mm, 15.0));
    }

    #[test]
    fn laser_off_moves_are_travels() {
        let program = parse("G21\nG90\nG0 X0 Y0\nG1 X10 Y0\nM3\nS100\nG1 X10 Y10\nM5\n");
        assert_eq!(program.cuts.len(), 1);
        assert_eq!(program.travels.len(), 1);
        assert!(close(program.cut_length_mm, 10.0));
        assert!(close(program.travel_length_mm, 10.0));
    }

    #[test]
    fn bounds_cover_cuts_and_travels() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG1 X10 Y5\nM5\nG0 X20 Y0\n");
        let (min, max) = program.bounds().unwrap();
        assert!(close(min.x, 0.0));
        assert!(close(min.y, 0.0));
        assert!(close(max.x, 20.0));
        assert!(close(max.y, 5.0));
    }

    #[test]
    fn comments_and_compact_words_are_understood() {
        let program = parse("G21G90M3S200F500G1X2.5Y-1.5; trailing\n(end)M5\n");
        assert_eq!(program.cuts.len(), 1);
        let last = *program.cuts[0].last().unwrap();
        assert!(close(last.x, 2.5));
        assert!(close(last.y, -1.5));
    }

    #[test]
    fn a_clockwise_arc_is_drawn() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG2 X10 Y0 I5 J0\nM5\n");
        assert_eq!(program.cuts.len(), 1);
        assert!((program.cut_length_mm - std::f64::consts::PI * 5.0).abs() < 0.05);
        let (min, max) = program.bounds().unwrap();
        assert!(min.x.abs() < 1e-9 && min.y.abs() < 1e-9);
        assert!((max.x - 10.0).abs() < 1e-9);
        assert!((max.y - 5.0).abs() < 0.01);
    }

    #[test]
    fn a_radius_arc_is_drawn() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG3 X10 Y0 R5\nM5\n");
        assert_eq!(program.cuts.len(), 1);
        assert!((program.cut_length_mm - std::f64::consts::PI * 5.0).abs() < 0.05);
        let (min, _) = program.bounds().unwrap();
        // The counter-clockwise minor arc dips below the chord.
        assert!((min.y + 5.0).abs() < 0.01);
    }

    #[test]
    fn a_full_circle_is_drawn() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG2 I5 J0\nM5\n");
        assert_eq!(program.cuts.len(), 1);
        assert!((program.cut_length_mm - std::f64::consts::TAU * 5.0).abs() < 0.05);
        let (min, max) = program.bounds().unwrap();
        assert!((max.x - 10.0).abs() < 0.01);
        assert!((max.y - 5.0).abs() < 0.01);
        assert!((min.y + 5.0).abs() < 0.01);
    }

    #[test]
    fn an_arc_with_the_laser_off_is_a_travel() {
        let program = parse("G21\nG0 X0 Y0\nG2 X10 Y0 I5 J0\nM5\n");
        assert_eq!(program.cuts.len(), 0);
        assert_eq!(program.travels.len(), 1);
        assert!((program.travel_length_mm - std::f64::consts::PI * 5.0).abs() < 0.05);
    }

    #[test]
    fn an_arc_without_offsets_is_a_straight_move() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG2 X10 Y0\nM5\n");
        assert!(close(program.cut_length_mm, 10.0));
        assert!(
            program
                .warnings
                .iter()
                .any(|warning| warning.contains("without I, J or R"))
        );
    }

    #[test]
    fn an_arc_with_an_impossible_radius_falls_back_to_a_line() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG3 X10 Y0 R1\nM5\n");
        assert!(close(program.cut_length_mm, 10.0));
        assert!(
            program
                .warnings
                .iter()
                .any(|warning| warning.contains("cannot reach"))
        );
    }

    #[test]
    fn an_arc_end_point_off_the_circle_falls_back_to_a_line() {
        let program = parse("G21\nG0 X0 Y0\nM3\nG2 X10 Y0 I1 J0\nM5\n");
        assert!(close(program.cut_length_mm, 10.0));
        assert!(
            program
                .warnings
                .iter()
                .any(|warning| warning.contains("not on its circle"))
        );
    }

    #[test]
    fn unknown_commands_are_reported() {
        let program = parse("G21\nG1 X1 Y1\nG28\nM5\n");
        assert!(
            program
                .warnings
                .iter()
                .any(|warning| warning.contains("G28"))
        );
    }

    #[test]
    fn an_empty_file_is_reported() {
        let program = parse("\n\n(only a comment)\n");
        assert!(
            program
                .warnings
                .iter()
                .any(|warning| warning.contains("no motion"))
        );
        assert_eq!(program.estimated_seconds, None);
    }

    #[test]
    fn warnings_are_not_repeated() {
        let program = parse("G28\nG28\nG28\nG1 X1 Y1\n");
        assert_eq!(
            program
                .warnings
                .iter()
                .filter(|warning| warning.contains("G28"))
                .count(),
            1
        );
    }

    #[test]
    fn tokens_split_run_together_words() {
        let words = tokens("G1X10.5Y-2F1000");
        assert_eq!(
            words,
            vec![('G', 1.0), ('X', 10.5), ('Y', -2.0), ('F', 1000.0)]
        );
    }

    #[test]
    fn a_program_without_points_has_no_bounds() {
        let program = Program::default();
        assert!(program.bounds().is_none());
        assert!(parse("G21\nG90\n").bounds().is_none());
    }

    #[test]
    fn points_and_origin_are_exact() {
        let program = parse("G21\nM3\nG1 X1 Y0\nG1 X0 Y0\nM5\n");
        assert_eq!(program.cuts[0][0], Point2::new(0.0, 0.0));
        assert_eq!(*program.cuts[0].last().unwrap(), Point2::new(0.0, 0.0));
        assert_eq!(program.cut_points(), 3);
    }
}
