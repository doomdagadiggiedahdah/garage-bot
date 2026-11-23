//! Stale-command protection (main.py `stale_command_age`, commit 2341d02).

pub const MAX_COMMAND_AGE_SECONDS: i64 = 60;

#[cfg_attr(kani, derive(kani::Arbitrary))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Status,
    Open,
    Close,
    Silence,
    Update,
    Debug,
    /// Anything `handle_command` does not recognise (returns None). Includes "help".
    Unknown,
}

impl Cmd {
    /// `handle_command`'s matching: lower-case, strip, cut at the first '@'.
    pub fn parse(text: &str) -> Cmd {
        let lower = text.to_lowercase();
        let cmd = lower.trim().split('@').next().unwrap_or("");
        match cmd {
            "status" | "/status" => Cmd::Status,
            "open" | "/open" => Cmd::Open,
            "close" | "/close" => Cmd::Close,
            "silence" | "/silence" | "quiet" | "stop" | "mute" => Cmd::Silence,
            "update" | "/update" => Cmd::Update,
            "debug" | "/debug" | "info" | "/info" => Cmd::Debug,
            _ => Cmd::Unknown,
        }
    }

    /// `cmd.lstrip("/") in STALE_GUARDED_COMMANDS`
    pub fn is_stale_guarded(self) -> bool {
        matches!(self, Cmd::Open | Cmd::Close | Cmd::Update)
    }
}

/// Returns the age in seconds if this is a guarded command that is too old, else None.
/// `date` is Telegram's `message.date` (Unix seconds); None or 0 is "missing" (falsy).
pub fn stale_command_age(clock_synced: bool, cmd: Cmd, date: Option<i64>, now_unix: i64) -> Option<i64> {
    if !clock_synced {
        return None;
    }
    if !cmd.is_stale_guarded() {
        return None;
    }
    let sent = match date {
        Some(d) if d != 0 => d,
        _ => return None,
    };
    let age = now_unix.saturating_sub(sent); // Python ints cannot overflow
    if age > MAX_COMMAND_AGE_SECONDS {
        Some(age)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_matches_handle_command() {
        assert_eq!(Cmd::parse(" /Open@garage_bot "), Cmd::Open);
        assert_eq!(Cmd::parse("close"), Cmd::Close);
        assert_eq!(Cmd::parse("mute"), Cmd::Silence);
        assert_eq!(Cmd::parse("info"), Cmd::Debug);
        assert_eq!(Cmd::parse("hello"), Cmd::Unknown);
    }

    /// FINDING: the startup message says "Type 'help' for commands" but there is no handler.
    #[test]
    fn help_is_not_a_command() {
        assert_eq!(Cmd::parse("help"), Cmd::Unknown);
        assert_eq!(Cmd::parse("/help"), Cmd::Unknown);
    }

    #[test]
    fn boundary_is_strictly_greater_than_60() {
        assert_eq!(stale_command_age(true, Cmd::Open, Some(1000), 1060), None);
        assert_eq!(stale_command_age(true, Cmd::Open, Some(1000), 1061), Some(61));
    }

    #[test]
    fn unguarded_unsynced_and_undated_are_never_stale() {
        assert_eq!(stale_command_age(true, Cmd::Status, Some(1), 999_999), None);
        assert_eq!(stale_command_age(false, Cmd::Open, Some(1), 999_999), None);
        assert_eq!(stale_command_age(true, Cmd::Open, None, 999_999), None);
        assert_eq!(stale_command_age(true, Cmd::Open, Some(0), 999_999), None);
    }

    #[test]
    fn clock_behind_gives_negative_age_not_stale() {
        assert_eq!(stale_command_age(true, Cmd::Close, Some(2000), 1000), None);
    }
}
