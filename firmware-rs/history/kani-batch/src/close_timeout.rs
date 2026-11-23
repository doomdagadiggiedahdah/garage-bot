//! "Door did not close" alert after a bot-triggered close (main.py lines 506-516, 701-721).

use crate::ticks::ticks_diff;

pub const CLOSE_TIMEOUT_MS: i32 = 60 * 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseWatch {
    pub bot_triggered_close: bool,
    pub close_requested_time: Option<u32>,
    pub close_timeout_alerted: bool,
}

impl CloseWatch {
    /// `handle_command("close")`. `now_ticks` is a fresh `time.ticks_ms()` taken
    /// inside the handler, so it can be later than the loop's `current_time`.
    /// Returns true if the button is pressed.
    pub fn on_close_command(&mut self, door_open: bool, now_ticks: u32) -> bool {
        if !door_open {
            return false; // "Door is already closed!"
        }
        self.bot_triggered_close = true;
        self.close_requested_time = Some(now_ticks);
        self.close_timeout_alerted = false;
        true
    }

    /// The door-monitor section of one main-loop iteration.
    /// Returns true if the "Door did not close!" warning is sent in this iteration.
    pub fn monitor(&mut self, door_open: bool, current_time: u32) -> bool {
        if door_open {
            if self.bot_triggered_close && !self.close_timeout_alerted {
                if let Some(requested) = self.close_requested_time {
                    if ticks_diff(current_time, requested) >= CLOSE_TIMEOUT_MS {
                        self.close_timeout_alerted = true;
                        return true;
                    }
                }
            }
            false
        } else {
            *self = CloseWatch::default();
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticks::{ticks_ms_at, TICKS_PERIOD};

    #[test]
    fn fires_once_at_timeout() {
        let mut w = CloseWatch::default();
        assert!(w.on_close_command(true, 1_000));
        assert!(!w.monitor(true, 60_999));
        assert!(w.monitor(true, 61_000));
        assert!(!w.monitor(true, 61_500));
    }

    #[test]
    fn fires_across_wrap() {
        let mut w = CloseWatch::default();
        let t0 = TICKS_PERIOD - 30_000;
        w.on_close_command(true, t0);
        assert!(!w.monitor(true, 29_999));
        assert!(w.monitor(true, 30_000));
    }

    #[test]
    fn no_alert_if_door_closes_in_time() {
        let mut w = CloseWatch::default();
        w.on_close_command(true, 0);
        assert!(!w.monitor(true, 20_000));
        assert!(!w.monitor(false, 30_000));
        assert_eq!(w, CloseWatch::default());
        assert!(!w.monitor(true, 90_000)); // reopened by hand: no bot close pending
    }

    #[test]
    fn close_on_closed_door_does_nothing() {
        let mut w = CloseWatch::default();
        assert!(!w.on_close_command(false, 0));
        assert!(!w.monitor(true, 100_000));
    }

    /// Outside the proven envelope: a single loop gap of 2^29 ms hides the timeout.
    #[test]
    fn huge_loop_gap_misses_alert() {
        let mut w = CloseWatch::default();
        w.on_close_command(true, 0);
        assert!(!w.monitor(true, ticks_ms_at(1 << 29)));
    }
}
