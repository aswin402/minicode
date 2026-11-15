//! Demand-driven adaptive TUI frame scheduler with EWMA backpressure and DEC Mode 2026 synchronized output.
//!
//! Eliminates fixed-rate idle polling (`0.0%` CPU at rest) and prevents 100+ FPS render storms
//! during high-throughput KV-cached SSE streaming by pacing redraws proportionally to measured
//! terminal draw + flush latency.

use ratatui::backend::CrosstermBackend;
use ratatui::{Frame, Terminal};
use std::io::Write;
use std::time::{Duration, Instant};

/// Bitmask describing which regions of the TUI require a redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirtyFlags(u8);

impl DirtyFlags {
    pub const NONE: Self = Self(0);
    /// Streaming markdown/thought tail or active tool spinner updated.
    pub const TIMELINE_TAIL: Self = Self(1 << 0);
    /// Structural timeline mutation (scroll, resize, new entry, session clear).
    pub const TIMELINE_LAYOUT: Self = Self(1 << 1);
    /// Bottom status bar telemetry (tokens, cost, model, background task count).
    pub const STATUS: Self = Self(1 << 2);
    /// Floating overlays (modals, toasts, slash palette, drawers).
    pub const OVERLAY: Self = Self(1 << 3);
    /// Periodic animation tick (activity spinner & `.1s` elapsed timer).
    pub const ANIMATION: Self = Self(1 << 4);
    /// Direct user interaction (keyboard, mouse, paste, terminal resize) — bypasses coalescing delay.
    pub const URGENT: Self = Self(1 << 5);

    pub const ALL: Self = Self(
        Self::TIMELINE_TAIL.0
            | Self::TIMELINE_LAYOUT.0
            | Self::STATUS.0
            | Self::OVERLAY.0
            | Self::ANIMATION.0,
    );

    #[inline]
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[inline]
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    #[inline]
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl std::ops::BitOr for DirtyFlags {
    type Output = Self;

    #[inline]
    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for DirtyFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// DEC Private Mode 2026 escape sequences for tear-free atomic terminal frame updates.
const DEC_SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const DEC_SYNC_END: &[u8] = b"\x1b[?2026l";

/// Minimum coalescing floor for non-urgent streaming updates (~60 Hz display refresh ceiling).
const STREAM_FLOOR_US: u64 = 16_667;
/// Maximum coalescing ceiling under heavy terminal backpressure (10 Hz, matching `.1s` timer precision).
const STREAM_CEILING_US: u64 = 100_000;

/// Demand-driven TUI frame scheduler using EWMA render-cost telemetry.
#[derive(Debug)]
pub struct FrameScheduler {
    dirty: DirtyFlags,
    last_draw_at: Option<Instant>,
    /// Exponentially weighted moving average (`alpha = 1/4`) of `terminal.draw` + flush duration in microseconds.
    ewma_draw_us: u64,
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameScheduler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            // Ensure the initial welcome / hydrated screen renders immediately on startup.
            dirty: DirtyFlags::ALL | DirtyFlags::URGENT,
            last_draw_at: None,
            ewma_draw_us: 2_000,
        }
    }

    /// Marks the given UI region(s) dirty so the scheduler plans a frame draw.
    #[inline]
    pub fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.dirty.insert(flags);
    }

    /// Marks an urgent user-driven change (key, mouse, paste, resize) for immediate 0ms-latency rendering.
    #[inline]
    pub fn mark_urgent(&mut self, flags: DirtyFlags) {
        self.dirty.insert(flags | DirtyFlags::URGENT);
    }

    /// Returns the currently accumulated dirty flags.
    #[inline]
    #[must_use]
    pub const fn dirty_flags(&self) -> DirtyFlags {
        self.dirty
    }

    /// Returns the measured EWMA frame render time in microseconds.
    #[inline]
    #[must_use]
    pub const fn ewma_draw_micros(&self) -> u64 {
        self.ewma_draw_us
    }

    /// Computes the adaptive minimum inter-frame interval for the pending `dirty` state.
    ///
    /// - `URGENT` user input renders immediately (`Duration::ZERO`).
    /// - Streaming / status / overlay updates pace at `3 * ewma_draw_us` clamped to `[16.67ms, 100ms]`,
    ///   automatically capping TUI CPU share to <= 33% of a single core even on slow SSH/GPU terminals.
    /// - Pure `ANIMATION` ticks pace at `max(SPINNER_FRAME_MS, 3 * ewma_draw_us)` (`80ms..120ms`).
    #[must_use]
    pub fn min_frame_interval(&self) -> Duration {
        if self.dirty.contains(DirtyFlags::URGENT) {
            return Duration::ZERO;
        }
        let adaptive_us = self.ewma_draw_us.saturating_mul(3);
        let content_flags = DirtyFlags::TIMELINE_TAIL
            | DirtyFlags::TIMELINE_LAYOUT
            | DirtyFlags::STATUS
            | DirtyFlags::OVERLAY;
        if self.dirty.intersects(content_flags) {
            Duration::from_micros(adaptive_us.clamp(STREAM_FLOOR_US, STREAM_CEILING_US))
        } else {
            let anim_floor_us = crate::constants::SPINNER_FRAME_MS.saturating_mul(1_000);
            Duration::from_micros(adaptive_us.clamp(anim_floor_us, 125_000))
        }
    }

    /// Returns `true` if a frame should be drawn right now at `now`.
    #[must_use]
    pub fn should_draw_now(&self, now: Instant) -> bool {
        if self.dirty.is_empty() {
            return false;
        }
        let Some(last) = self.last_draw_at else {
            return true;
        };
        if self.dirty.contains(DirtyFlags::URGENT) {
            return true;
        }
        now.saturating_duration_since(last) >= self.min_frame_interval()
    }

    /// Computes the exact next monotonic wakeup deadline, or `None` when completely idle.
    ///
    /// Returning `None` enables `App::run` to park on `std::future::pending()`, achieving
    /// **0.00% CPU usage and zero timer syscalls** whenever the agent is idle and no toasts
    /// or live process modals are active.
    #[must_use]
    pub fn next_deadline(
        &self,
        now: Instant,
        is_animating: bool,
        next_toast_expiry: Option<Instant>,
        next_modal_poll: Option<Instant>,
    ) -> Option<Instant> {
        let mut earliest: Option<Instant> = None;
        let mut fold_min = |candidate: Instant| {
            earliest = Some(match earliest {
                Some(cur) if cur <= candidate => cur,
                _ => candidate,
            });
        };

        if !self.dirty.is_empty() {
            let due = match self.last_draw_at {
                Some(last) => (last + self.min_frame_interval()).max(now),
                None => now,
            };
            fold_min(due);
        } else if is_animating {
            let anim_interval = Duration::from_micros(
                (self.ewma_draw_us.saturating_mul(3))
                    .clamp(crate::constants::SPINNER_FRAME_MS * 1_000, 125_000),
            );
            let due = match self.last_draw_at {
                Some(last) => (last + anim_interval).max(now),
                None => now + anim_interval,
            };
            fold_min(due);
        }

        if let Some(toast_due) = next_toast_expiry {
            fold_min(toast_due.max(now));
        }

        if let Some(modal_due) = next_modal_poll {
            fold_min(modal_due.max(now));
        }

        earliest
    }

    /// Clears pending dirty flags and updates the EWMA render-cost estimator after a completed draw.
    pub fn record_draw_completed(&mut self, finished_at: Instant, draw_duration: Duration) {
        let sample_us = (draw_duration.as_micros() as u64).clamp(100, 250_000);
        // Integer EWMA with alpha = 1/4: smooths spikes while adapting within ~4 frames.
        self.ewma_draw_us = (self.ewma_draw_us.saturating_mul(3) + sample_us) / 4;
        self.last_draw_at = Some(finished_at);
        self.dirty = DirtyFlags::NONE;
    }

    /// Executes an atomic synchronized terminal draw (`CSI ? 2026 h` ... `CSI ? 2026 l`) and
    /// records frame latency into the adaptive EWMA scheduler.
    pub fn draw_synchronized<W, F>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<W>>,
        render_fn: F,
    ) -> std::io::Result<()>
    where
        W: Write,
        F: FnOnce(&mut Frame),
    {
        let start = Instant::now();
        let _ = terminal.backend_mut().write_all(DEC_SYNC_BEGIN);
        let draw_res = terminal.draw(render_fn).map(|_| ());
        let _ = terminal.backend_mut().write_all(DEC_SYNC_END);
        let _ = terminal.backend_mut().flush();
        draw_res?;
        let finished = Instant::now();
        self.record_draw_completed(finished, finished.saturating_duration_since(start));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_idle_law_returns_none_when_clean_and_idle() {
        let mut sched = FrameScheduler::new();
        let t0 = Instant::now();
        assert!(sched.should_draw_now(t0));

        sched.record_draw_completed(t0, Duration::from_micros(1_200));
        assert!(!sched.should_draw_now(t0));

        // Zero-Idle Law: clean + not animating + no toast + no modal poll => None (infinite sleep!)
        assert_eq!(sched.next_deadline(t0, false, None, None), None);
    }

    #[test]
    fn test_urgent_input_bypasses_coalescing_window() {
        let mut sched = FrameScheduler::new();
        let t0 = Instant::now();
        sched.record_draw_completed(t0, Duration::from_millis(8));

        // Non-urgent streaming delta 2ms after draw is coalesced
        let t1 = t0 + Duration::from_millis(2);
        sched.mark_dirty(DirtyFlags::TIMELINE_TAIL);
        assert!(!sched.should_draw_now(t1));

        // Urgent user keystroke renders immediately with 0ms artificial latency
        sched.mark_urgent(DirtyFlags::OVERLAY);
        assert!(sched.should_draw_now(t1));
    }

    #[test]
    fn test_ewma_backpressure_scales_interval_on_heavy_draws() {
        let mut sched = FrameScheduler::new();
        let mut now = Instant::now();
        for _ in 0..8 {
            now += Duration::from_millis(30);
            sched.record_draw_completed(now, Duration::from_millis(25));
        }
        sched.mark_dirty(DirtyFlags::TIMELINE_TAIL);
        let interval = sched.min_frame_interval();
        // 3 * ~25ms = ~75ms (automatically throttles from 60Hz down to ~13Hz under heavy load)
        assert!(interval >= Duration::from_millis(60));
        assert!(interval <= Duration::from_millis(100));
    }

    #[test]
    fn test_toast_expiry_schedules_exact_oneshot_wakeup() {
        let mut sched = FrameScheduler::new();
        let t0 = Instant::now();
        sched.record_draw_completed(t0, Duration::from_millis(1));

        let expiry = t0 + Duration::from_millis(3_800);
        assert_eq!(
            sched.next_deadline(t0, false, Some(expiry), None),
            Some(expiry)
        );
    }
}
