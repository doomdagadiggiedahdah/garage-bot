//! Heartbeat-driven uptime accumulator (main.py lines 616-619).

use crate::ticks::ticks_diff;

pub const HEARTBEAT_INTERVAL_MS: i32 = 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uptime {
    pub uptime_seconds: u64,
    pub last_heartbeat_time: u32,
}

impl Uptime {
    pub fn new(now_ticks: u32) -> Self {
        Uptime { uptime_seconds: 0, last_heartbeat_time: now_ticks }
    }

    /// One loop iteration's heartbeat check. Returns true if a heartbeat fired.
    pub fn step(&mut self, current_time: u32) -> bool {
        let d = ticks_diff(current_time, self.last_heartbeat_time);
        if d >= HEARTBEAT_INTERVAL_MS {
            self.uptime_seconds += (d / 1000) as u64; // Python `//`; d > 0 so identical
            self.last_heartbeat_time = current_time;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticks::ticks_ms_at;

    #[test]
    fn accumulates_across_many_wraps() {
        // 60 days of 60.5 s loop-aligned heartbeats: > 4 wraps of the 12.4-day counter
        let mut u = Uptime::new(0);
        let mut t: u64 = 0;
        let mut prev = 0;
        while t < 60 * 24 * 3600 * 1000 {
            t += 500;
            u.step(ticks_ms_at(t));
            assert!(u.uptime_seconds >= prev);
            prev = u.uptime_seconds;
        }
        // FINDING (drift): each heartbeat fires at 60.0 s exactly here, so no loss...
        assert_eq!(u.uptime_seconds, 60 * 24 * 3600);
    }

    /// FINDING: the sub-second remainder is dropped at every heartbeat.
    #[test]
    fn drops_sub_second_remainder() {
        let mut u = Uptime::new(0);
        let mut t: u64 = 0;
        for _ in 0..1000 {
            t += 60_999;
            assert!(u.step(ticks_ms_at(t)));
        }
        assert_eq!(t / 1000, 60_999);
        assert_eq!(u.uptime_seconds, 60_000); // 999 s (1.6 %) lost
    }

    #[test]
    fn stall_longer_than_half_period_loses_time_but_never_goes_backward() {
        let mut u = Uptime::new(0);
        assert!(!u.step(ticks_ms_at(1 << 29)));
        assert_eq!(u.uptime_seconds, 0);
    }
}
