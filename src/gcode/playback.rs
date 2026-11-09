use crate::gcode::toolpath::{MoveKind, Toolpath};
use crate::geometry::Point2;

/// One straight movement in the simulated job.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaybackSegment {
    pub to: [f32; 2],
    pub kind: MoveKind,
    /// Length in millimetres.
    pub length: f32,
    /// Feed rate in millimetres per minute (travel speed for rapids).
    pub speed: f32,
    /// Laser power in the writer's `S` range (0 for rapids).
    pub power: f32,
    /// One-based pass number.
    pub pass: u16,
}

impl PlaybackSegment {
    pub fn to_point(&self) -> Point2 {
        Point2::new(f64::from(self.to[0]), f64::from(self.to[1]))
    }
}

/// Laser position and settings at a point in the simulation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaybackPosition {
    pub point: Point2,
    pub kind: MoveKind,
    pub power: f64,
    pub pass: u32,
}

/// Flattened, timed view of a job used by the preview window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Playback {
    pub segments: Vec<PlaybackSegment>,
    /// Start of each discontinuous run, keyed by its first segment. Most
    /// segments start where the previous one ended, so only toolpath and pass
    /// boundaries need an entry.
    starts: Vec<(usize, [f32; 2])>,
    pub total_length: f64,
    pub duration_seconds: f64,
}

impl Playback {
    /// Flatten every toolpath, repeating its moves for each pass.
    pub fn from_toolpaths(toolpaths: &[Toolpath], travel_speed_mm_min: f64) -> Self {
        let mut playback = Self::default();
        for toolpath in toolpaths {
            playback.append_toolpath(toolpath, travel_speed_mm_min);
        }
        playback
    }

    /// Append one toolpath to an existing playback.
    ///
    /// Used by the preview to build segments from streamed raster bands without
    /// keeping the whole move list alive.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    pub fn append_toolpath(&mut self, toolpath: &Toolpath, travel_speed_mm_min: f64) {
        let travel_speed = if travel_speed_mm_min > 0.0 {
            travel_speed_mm_min
        } else {
            3000.0
        };
        let passes = toolpath.passes.max(1);
        for pass in 1..=passes {
            let mut previous: Option<Point2> = None;
            let mut first_segment = true;
            for movement in &toolpath.moves {
                let to = Point2::new(movement.x, movement.y);
                if let Some(from) = previous {
                    let length = from.distance(to);
                    let (speed, power) = match movement.kind {
                        MoveKind::Cut => (toolpath.speed, toolpath.power),
                        MoveKind::Travel => (travel_speed, 0.0),
                    };
                    if length > 0.0 {
                        self.total_length += length;
                        if speed > 0.0 {
                            self.duration_seconds += length / speed * 60.0;
                        }
                        if first_segment {
                            self.starts
                                .push((self.segments.len(), [from.x as f32, from.y as f32]));
                            first_segment = false;
                        }
                        self.segments.push(PlaybackSegment {
                            to: [to.x as f32, to.y as f32],
                            kind: movement.kind,
                            length: length as f32,
                            speed: speed as f32,
                            power: power as f32,
                            pass: pass as u16,
                        });
                    }
                }
                previous = Some(to);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Start of the segment at `index`.
    ///
    /// The start is stored only at a toolpath or pass boundary; otherwise it is
    /// the end of the previous segment.
    pub fn from_point(&self, index: usize) -> Option<Point2> {
        if self.segments.is_empty() {
            return None;
        }
        let position = self.starts.partition_point(|(start, _)| *start <= index);
        if let Some((start, point)) = self.starts.get(position.checked_sub(1)?)
            && *start == index
        {
            return Some(Point2::new(f64::from(point[0]), f64::from(point[1])));
        }
        if index == 0 {
            return None;
        }
        self.segments.get(index - 1).map(PlaybackSegment::to_point)
    }

    /// Laser state after travelling `length` millimetres of the job.
    pub fn position_at(&self, length: f64) -> Option<PlaybackPosition> {
        let length = length.clamp(0.0, self.total_length);
        let mut travelled = 0.0;
        let last = self.segments.len().saturating_sub(1);
        for (index, segment) in self.segments.iter().enumerate() {
            if length <= travelled + f64::from(segment.length) || index == last {
                let ratio = if segment.length > 0.0 {
                    ((length - travelled) / f64::from(segment.length)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let from = self.from_point(index)?;
                let to = segment.to_point();
                return Some(PlaybackPosition {
                    point: Point2::new(
                        from.x + (to.x - from.x) * ratio,
                        from.y + (to.y - from.y) * ratio,
                    ),
                    kind: segment.kind,
                    power: f64::from(segment.power),
                    pass: u32::from(segment.pass),
                });
            }
            travelled += f64::from(segment.length);
        }
        None
    }

    /// How far the job has travelled after `seconds` of cutting time.
    pub fn length_at_seconds(&self, seconds: f64) -> f64 {
        if seconds <= 0.0 || self.duration_seconds <= 0.0 {
            return 0.0;
        }
        if seconds >= self.duration_seconds {
            return self.total_length;
        }
        let mut elapsed = 0.0;
        let mut travelled = 0.0;
        for segment in &self.segments {
            let duration = if segment.speed > 0.0 {
                f64::from(segment.length) / f64::from(segment.speed) * 60.0
            } else {
                0.0
            };
            if elapsed + duration >= seconds {
                let ratio = if duration > 0.0 {
                    (seconds - elapsed) / duration
                } else {
                    1.0
                };
                return travelled + f64::from(segment.length) * ratio;
            }
            elapsed += duration;
            travelled += f64::from(segment.length);
        }
        self.total_length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toolpath(passes: u32) -> Toolpath {
        let mut path = Toolpath::new("a", 600.0, 250.0, passes);
        path.push(MoveKind::Travel, 0.0, 0.0);
        path.push(MoveKind::Cut, 10.0, 0.0);
        path.push(MoveKind::Travel, 10.0, 10.0);
        path.push(MoveKind::Cut, 20.0, 10.0);
        path
    }

    #[test]
    fn segments_record_length_kind_and_power() {
        let playback = Playback::from_toolpaths(&[toolpath(1)], 3000.0);
        assert_eq!(playback.segments.len(), 3);
        assert_eq!(playback.segments[0].kind, MoveKind::Cut);
        assert!((playback.segments[0].length - 10.0).abs() < 1e-9);
        assert!((playback.segments[0].power - 250.0).abs() < 1e-9);
        assert_eq!(playback.segments[1].kind, MoveKind::Travel);
        assert!((playback.segments[1].power).abs() < 1e-9);
        assert!((playback.total_length - 30.0).abs() < 1e-9);
    }

    #[test]
    fn a_new_toolpath_keeps_its_own_start_point() {
        let mut second = Toolpath::new("b", 600.0, 250.0, 1);
        second.push(MoveKind::Travel, 50.0, 50.0);
        second.push(MoveKind::Cut, 60.0, 50.0);
        let playback = Playback::from_toolpaths(&[toolpath(1), second], 3000.0);
        assert_eq!(playback.from_point(0), Some(Point2::new(0.0, 0.0)));
        assert_eq!(playback.from_point(3), Some(Point2::new(50.0, 50.0)));
        assert_eq!(playback.from_point(2), Some(Point2::new(10.0, 10.0)));
    }

    #[test]
    fn passes_repeat_the_path_and_duration() {
        let playback = Playback::from_toolpaths(&[toolpath(2)], 3000.0);
        assert_eq!(playback.segments.len(), 6);
        assert!((playback.total_length - 60.0).abs() < 1e-9);
        assert_eq!(playback.segments[5].pass, 2);
        // 20 mm of cutting at 600 mm/min plus 10 mm of rapids at 3000 mm/min.
        let expected = (20.0 / 600.0 + 10.0 / 3000.0) * 60.0 * 2.0;
        assert!((playback.duration_seconds - expected).abs() < 1e-9);
    }

    #[test]
    fn position_follows_the_travelled_length() {
        let playback = Playback::from_toolpaths(&[toolpath(1)], 3000.0);
        let start = playback.position_at(0.0).unwrap();
        assert!((start.point.x).abs() < 1e-9);
        assert_eq!(start.kind, MoveKind::Cut);
        let middle = playback.position_at(5.0).unwrap();
        assert!((middle.point.x - 5.0).abs() < 1e-9);
        assert_eq!(middle.power, 250.0);
        let end = playback.position_at(playback.total_length).unwrap();
        assert!((end.point.x - 20.0).abs() < 1e-9);
        assert!((end.point.y - 10.0).abs() < 1e-9);
    }

    #[test]
    fn travel_segments_report_no_power() {
        let playback = Playback::from_toolpaths(&[toolpath(1)], 3000.0);
        let travel = playback.position_at(10.5).unwrap();
        assert_eq!(travel.kind, MoveKind::Travel);
        assert!((travel.power).abs() < 1e-9);
    }

    #[test]
    fn length_at_seconds_matches_the_segments() {
        let playback = Playback::from_toolpaths(&[toolpath(1)], 3000.0);
        assert!((playback.length_at_seconds(0.0)).abs() < 1e-9);
        assert!((playback.length_at_seconds(1.0) - 10.0).abs() < 1e-9);
        // 10 mm of cutting takes 1 s, the 10 mm rapid takes 0.2 s.
        assert!((playback.length_at_seconds(1.1) - 15.0).abs() < 1e-9);
        assert!((playback.length_at_seconds(999.0) - playback.total_length).abs() < 1e-9);
    }

    #[test]
    fn empty_job_has_no_position() {
        let playback = Playback::from_toolpaths(&[], 3000.0);
        assert!(playback.is_empty());
        assert_eq!(playback.position_at(0.0), None);
        assert!((playback.total_length).abs() < 1e-9);
    }
}
