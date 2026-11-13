//! Optional frame timing, enabled with `--frame-log`.
//!
//! One line per frame on stderr with the wait since the previous frame and the
//! work the frame did. On a small board it separates a slow frame (high
//! `work`) from a frame that was never asked for (high `gap`), which are fixed
//! in completely different places.

use std::time::Instant;

/// Frame timing trace. Disabled unless `--frame-log` is passed.
#[derive(Default)]
pub(super) struct FrameTrace {
    enabled: bool,
    frame_start: Option<Instant>,
    last_end: Option<Instant>,
    index: u64,
}

impl FrameTrace {
    pub(super) fn enable(&mut self) {
        self.enabled = true;
    }

    /// Mark the start of a frame, before the application work.
    pub(super) fn begin(&mut self) {
        if self.enabled {
            self.frame_start = Some(Instant::now());
        }
    }

    /// Print the frame line and remember when it ended.
    pub(super) fn end(&mut self, events: usize, causes: &[String]) {
        if !self.enabled {
            return;
        }
        let Some(start) = self.frame_start.take() else {
            return;
        };
        let now = Instant::now();
        let gap = self
            .last_end
            .map_or(0.0, |last| (start - last).as_secs_f64() * 1000.0);
        let work = (now - start).as_secs_f64() * 1000.0;
        self.index += 1;
        if causes.is_empty() {
            eprintln!(
                "frame {} gap={gap:.1}ms work={work:.1}ms events={events}",
                self.index
            );
        } else {
            eprintln!(
                "frame {} gap={gap:.1}ms work={work:.1}ms events={events} causes=[{}]",
                self.index,
                causes.join("; ")
            );
        }
        self.last_end = Some(now);
    }
}
