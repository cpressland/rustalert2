//! The game: the simulation stepped at game speed and drawn as it runs.

use std::time::Duration;

/// Turns wall time into whole simulation frames at a fixed rate, keeping
/// the remainder so drawing can blend between the last two frames.
#[derive(Debug, Clone)]
pub struct FixedStep {
    tick: Duration,
    carried: Duration,
    /// The most frames one call will run, so a stall doesn't snowball.
    max_catch_up: u32,
}

impl FixedStep {
    /// Steps at `frames_per_second` (as the game speed gives it).
    pub fn new(frames_per_second: u32) -> FixedStep {
        FixedStep {
            tick: Duration::from_secs(1) / frames_per_second.max(1),
            carried: Duration::ZERO,
            max_catch_up: 5,
        }
    }

    /// Add `elapsed` wall time; returns how many frames to run now.
    pub fn advance(&mut self, elapsed: Duration) -> u32 {
        self.carried += elapsed;
        let mut frames = 0;
        while self.carried >= self.tick {
            self.carried -= self.tick;
            frames += 1;
            if frames == self.max_catch_up {
                // Too far behind: drop the rest rather than run faster.
                self.carried = Duration::ZERO;
                break;
            }
        }
        frames
    }

    /// How far into the next frame wall time is, 0 to 1: the blend
    /// between the last frame's positions and the current ones.
    pub fn blend(&self) -> f32 {
        self.carried.as_secs_f32() / self.tick.as_secs_f32()
    }
}

/// A point between `a` and `b` at `t` (0 to 1), rounded.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn mix(a: i32, b: i32, t: f32) -> i32 {
    (a as f32 + (b - a) as f32 * t.clamp(0.0, 1.0)).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_frames_run_and_the_rest_carries() {
        let mut s = FixedStep::new(15);
        assert_eq!(s.advance(Duration::from_millis(50)), 0);
        assert!((s.blend() - 0.75).abs() < 0.01);
        assert_eq!(s.advance(Duration::from_millis(50)), 1);
        assert_eq!(s.advance(Duration::from_millis(200)), 3);
    }

    #[test]
    fn a_stall_runs_at_most_five_frames() {
        let mut s = FixedStep::new(30);
        assert_eq!(s.advance(Duration::from_secs(2)), 5);
        assert!(s.blend() < 0.01);
    }

    #[test]
    fn mixing_blends_positions() {
        assert_eq!(mix(100, 200, 0.25), 125);
        assert_eq!(mix(100, 200, 2.0), 200);
    }
}
