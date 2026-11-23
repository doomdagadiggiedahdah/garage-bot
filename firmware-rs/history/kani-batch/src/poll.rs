//! The Telegram poll/response logic as a state machine (main.py lines 372-465, 633-667).
//!
//! No I/O: the caller asks `next_request()`, performs it (or, in a proof, invents
//! the outcome) and feeds the result to `on_flush` / `on_updates`.

use crate::stale::{stale_command_age, Cmd};

/// Outcome of one HTTP request. In main.py, Timeout is `OSError`, Error is any
/// other exception; both make the request functions return "nothing".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Net<T> {
    Ok(T),
    Timeout,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// `getUpdates?offset=-1&timeout=0` (flush_pending_updates)
    Flush,
    /// `getUpdates?offset={LAST_UPDATE_ID + 1}`
    GetUpdates { offset: u64 },
}

/// Parsed body of a successful flush request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlushResponse {
    pub ok: bool,
    /// update_id of the last element of `result`, if any
    pub newest: Option<u64>,
}

#[cfg_attr(kani, derive(kani::Arbitrary))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateKind {
    /// No `message` or no `text` (photo, edited message, ...)
    NonText,
    /// Text from a chat other than TELEGRAM_CHAT_ID
    Unauthorized,
    Command { cmd: Cmd, date: Option<i64> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Update {
    pub update_id: u64,
    pub kind: UpdateKind,
}

/// How many `send_telegram_message` calls the handling of an update makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replies {
    Zero,
    One,
    /// `update` with a new version: two announcements, then the OTA flow's own messages
    Several,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Logged or silently skipped; nothing sent, nothing executed
    Skip,
    /// One "Ignored '<cmd>' ..." reply; the command is NOT executed
    IgnoredStale { age: i64 },
    /// `handle_command` runs (door may move); `replies` messages are attempted
    Handled { cmd: Cmd, replies: Replies },
}

impl Action {
    pub fn replies(self) -> Replies {
        match self {
            Action::Skip => Replies::Zero,
            Action::IgnoredStale { .. } => Replies::One,
            Action::Handled { replies, .. } => replies,
        }
    }
}

/// Reply count of `handle_command` + the caller's `if response:` send.
pub fn handle_command_replies(cmd: Cmd, update_available: bool) -> Replies {
    match cmd {
        Cmd::Status | Cmd::Open | Cmd::Close | Cmd::Silence | Cmd::Debug => Replies::One,
        Cmd::Update => {
            if update_available {
                Replies::Several
            } else {
                Replies::One
            }
        }
        Cmd::Unknown => Replies::Zero,
    }
}

/// What the poll loop needs from the outside world while handling one update.
pub trait PollEnv {
    /// `now_unix()` at the moment the update is examined
    fn now_unix(&mut self) -> i64;
    /// Whether `check_for_update()` finds a newer version (only asked for `update`)
    fn update_available(&mut self) -> bool;
    /// Observe the decision for one update
    fn act(&mut self, update: &Update, action: Action);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollState {
    pub last_update_id: u64,
    pub backlog_flushed: bool,
    pub clock_synced: bool,
}

impl PollState {
    pub fn boot(clock_synced: bool) -> Self {
        PollState { last_update_id: 0, backlog_flushed: false, clock_synced }
    }

    /// Which request the next poll makes: never poll for commands until the flush succeeded.
    pub fn next_request(&self) -> Request {
        if !self.backlog_flushed {
            Request::Flush
        } else {
            Request::GetUpdates { offset: self.last_update_id + 1 }
        }
    }

    /// `backlog_flushed = flush_pending_updates()`
    pub fn on_flush(&mut self, res: Net<FlushResponse>) {
        self.backlog_flushed = match res {
            Net::Ok(r) if r.ok => {
                if let Some(id) = r.newest {
                    self.last_update_id = id;
                }
                true
            }
            _ => false,
        };
    }

    /// The body of `for update in updates:` for one update.
    pub fn process_update(&mut self, u: &Update, env: &mut impl PollEnv) -> Action {
        self.last_update_id = u.update_id;
        let action = match u.kind {
            UpdateKind::NonText | UpdateKind::Unauthorized => Action::Skip,
            UpdateKind::Command { cmd, date } => {
                match stale_command_age(self.clock_synced, cmd, date, env.now_unix()) {
                    Some(age) => Action::IgnoredStale { age },
                    None => {
                        let avail = cmd == Cmd::Update && env.update_available();
                        Action::Handled { cmd, replies: handle_command_replies(cmd, avail) }
                    }
                }
            }
        };
        env.act(u, action);
        action
    }

    /// `updates = get_telegram_updates()` followed by the for-loop.
    /// Timeout and Error both yield an empty list and leave the offset unchanged.
    pub fn on_updates(&mut self, res: Net<&[Update]>, env: &mut impl PollEnv) {
        if let Net::Ok(updates) = res {
            for u in updates {
                self.process_update(u, env);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Env {
        now: i64,
        avail: bool,
        log: Vec<(u64, Action)>,
    }
    impl PollEnv for Env {
        fn now_unix(&mut self) -> i64 {
            self.now
        }
        fn update_available(&mut self) -> bool {
            self.avail
        }
        fn act(&mut self, u: &Update, a: Action) {
            self.log.push((u.update_id, a));
        }
    }
    fn cmd(id: u64, cmd: Cmd, date: i64) -> Update {
        Update { update_id: id, kind: UpdateKind::Command { cmd, date: Some(date) } }
    }

    #[test]
    fn no_polling_until_flush_succeeds() {
        let mut s = PollState::boot(true);
        assert_eq!(s.next_request(), Request::Flush);
        s.on_flush(Net::Timeout);
        assert_eq!(s.next_request(), Request::Flush);
        s.on_flush(Net::Ok(FlushResponse { ok: false, newest: None }));
        assert_eq!(s.next_request(), Request::Flush);
        s.on_flush(Net::Ok(FlushResponse { ok: true, newest: Some(41) }));
        assert_eq!(s.next_request(), Request::GetUpdates { offset: 42 });
    }

    #[test]
    fn fresh_executes_stale_is_refused_with_one_reply() {
        let mut s = PollState { last_update_id: 0, backlog_flushed: true, clock_synced: true };
        let mut env = Env { now: 10_000, avail: false, log: vec![] };
        let ups = [cmd(1, Cmd::Open, 9_990), cmd(2, Cmd::Close, 9_000), cmd(3, Cmd::Status, 1)];
        s.on_updates(Net::Ok(&ups), &mut env);
        assert_eq!(env.log[0].1, Action::Handled { cmd: Cmd::Open, replies: Replies::One });
        assert_eq!(env.log[1].1, Action::IgnoredStale { age: 1000 });
        assert_eq!(env.log[2].1, Action::Handled { cmd: Cmd::Status, replies: Replies::One });
        assert_eq!(s.next_request(), Request::GetUpdates { offset: 4 });
    }

    #[test]
    fn failed_poll_leaves_offset_so_telegram_redelivers() {
        let mut s = PollState { last_update_id: 7, backlog_flushed: true, clock_synced: true };
        let mut env = Env { now: 0, avail: false, log: vec![] };
        s.on_updates(Net::Timeout, &mut env);
        s.on_updates(Net::Error, &mut env);
        assert!(env.log.is_empty());
        assert_eq!(s.next_request(), Request::GetUpdates { offset: 8 });
    }

    #[test]
    fn unknown_text_and_foreign_chats_get_no_reply() {
        let mut s = PollState { last_update_id: 0, backlog_flushed: true, clock_synced: true };
        let mut env = Env { now: 100, avail: false, log: vec![] };
        let ups = [
            cmd(1, Cmd::parse("help"), 100),
            Update { update_id: 2, kind: UpdateKind::Unauthorized },
            Update { update_id: 3, kind: UpdateKind::NonText },
        ];
        s.on_updates(Net::Ok(&ups), &mut env);
        assert!(env.log.iter().all(|(_, a)| a.replies() == Replies::Zero));
    }

    #[test]
    fn unsynced_clock_lets_old_guarded_command_run() {
        let mut s = PollState { last_update_id: 0, backlog_flushed: true, clock_synced: false };
        let mut env = Env { now: 999_999, avail: false, log: vec![] };
        s.on_updates(Net::Ok(&[cmd(1, Cmd::Open, 1)]), &mut env);
        assert_eq!(env.log[0].1, Action::Handled { cmd: Cmd::Open, replies: Replies::One });
    }
}
