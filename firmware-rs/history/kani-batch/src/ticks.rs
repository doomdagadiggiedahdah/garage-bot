//! `time.ticks_ms()` / `time.ticks_diff()` as MicroPython implements them on ESP32.
//!
//! ESP32 is a 32-bit port with 31-bit small ints, so
//! `MICROPY_PY_TIME_TICKS_PERIOD = MP_SMALL_INT_POSITIVE_MASK + 1 = 2^30`
//! (extmod/modtime.c). `ticks_ms()` is the millisecond counter masked to 30 bits:
//! it wraps every 2^30 ms = 12.43 days. `ticks_diff(a, b)` is
//! `((a - b + PERIOD/2) & (PERIOD-1)) - PERIOD/2`, a signed result in
//! `[-2^29, 2^29 - 1]`. It equals the true elapsed time only while that is
//! below 2^29 ms = 6.21 days (the documented maximum interval).

pub const TICKS_PERIOD: u32 = 1 << 30;
pub const TICKS_MASK: u32 = TICKS_PERIOD - 1;
pub const TICKS_HALF: i32 = 1 << 29;
/// Largest elapsed time (ms) that `ticks_diff` reports correctly: 2^29 - 1.
pub const MAX_INTERVAL_MS: u64 = (TICKS_HALF as u64) - 1;

/// The tick counter value at a given "real" time in ms since boot (ghost clock).
pub fn ticks_ms_at(real_ms: u64) -> u32 {
    (real_ms & TICKS_MASK as u64) as u32
}

/// `time.ticks_diff(end, start)`. Both arguments must be valid tick values (< 2^30).
pub fn ticks_diff(end: u32, start: u32) -> i32 {
    let d = end.wrapping_sub(start).wrapping_add(TICKS_HALF as u32) & TICKS_MASK;
    d as i32 - TICKS_HALF
}

pub const POLL_INTERVAL_MS: i32 = 5 * 1000;

/// main.py: `if time.ticks_diff(current_time, last_poll_time) >= POLL_INTERVAL_SECONDS * 1000`
pub fn poll_due(current_time: u32, last_poll_time: u32) -> bool {
    ticks_diff(current_time, last_poll_time) >= POLL_INTERVAL_MS
}

/// main.py: `elapsed_minutes = time.ticks_diff(current_time, open_start_time) / 60000`
/// (whole minutes, as shown in the "open for N minutes" alert via `int()`).
pub fn door_open_minutes(current_time: u32, open_start_time: u32) -> i32 {
    ticks_diff(current_time, open_start_time) / 60_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_diff() {
        assert_eq!(ticks_diff(5_000, 1_000), 4_000);
        assert_eq!(ticks_diff(1_000, 5_000), -4_000);
    }

    #[test]
    fn diff_across_wrap() {
        let start = TICKS_PERIOD - 10;
        assert_eq!(ticks_diff(20, start), 30);
        assert_eq!(ticks_diff(ticks_ms_at((1u64 << 30) + 20), ticks_ms_at((1u64 << 30) - 10)), 30);
    }

    #[test]
    fn max_interval_is_exact_and_one_more_is_not() {
        assert_eq!(ticks_diff(ticks_ms_at(MAX_INTERVAL_MS), 0), MAX_INTERVAL_MS as i32);
        assert_eq!(ticks_diff(ticks_ms_at(MAX_INTERVAL_MS + 1), 0), -TICKS_HALF);
    }

    #[test]
    fn poll_due_at_five_seconds() {
        assert!(!poll_due(4_999, 0));
        assert!(poll_due(5_000, 0));
        assert!(poll_due(4_000, TICKS_PERIOD - 1_000));
    }

    /// FINDING: a door left open for more than 6.21 days reports negative minutes.
    #[test]
    fn door_open_minutes_goes_negative_after_6_days() {
        let six_days = 6 * 24 * 3600 * 1000u64;
        assert_eq!(door_open_minutes(ticks_ms_at(six_days), 0), 8640);
        let seven_days = 7 * 24 * 3600 * 1000u64;
        assert!(door_open_minutes(ticks_ms_at(seven_days), 0) < 0);
        assert_eq!(door_open_minutes(ticks_ms_at(1u64 << 29), 0), -8947);
    }
}
