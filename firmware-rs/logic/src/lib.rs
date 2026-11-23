//! GarageBot logic: pure, no_std, no allocation, no I/O.
//!
//! Layout
//!   lib.rs          constants and types            (prelude, trusted definitions)
//!   spec.rs         executable specifications      (prelude; compiled only for kani / tests / feature "spec")
//!   fns/NAME.rs     one function each: contract + body + its contract_NAME harness
//!   props.rs        top-level properties, proven from the specifications alone (kani::stub(f, spec_f))
//!   verifier_proofs.rs   written by the independent verifier (not by the builder)
//!
//! Rule: bodies in fns/ must NOT call anything from spec.rs. The firmware build
//! does not even compile spec.rs, so a body that leans on it cannot link.
#![cfg_attr(not(any(test, kani)), no_std)]
#![allow(clippy::manual_range_contains)]
#![recursion_limit = "512"]

// @prelude
// ---------------------------------------------------------------- constants
/// A guarded command older than this many seconds is not executed (and gets no reply).
pub const STALE_AFTER_S: u64 = 60;
/// A time anchor older than this (monotonic ms) is not trusted any more.
pub const ANCHOR_MAX_AGE_MS: u64 = 300_000;
/// Slack added to the estimated "now" (seconds): 1 for sub-second truncation, 1 for clock drift.
pub const NOW_SLACK_S: u64 = 2;
/// Unix seconds above this are treated as garbage.
pub const MAX_UNIX_S: u64 = 1 << 40;
/// Update ids at or above this are treated as malformed.
pub const MAX_UPDATE_ID: u64 = 1 << 62;
/// Updates asked for and kept per poll.
pub const MAX_BATCH: u32 = 8;
pub const POLL_INTERVAL_MS: u64 = 5_000;
pub const HEARTBEAT_INTERVAL_MS: u64 = 60_000;
pub const INITIAL_ALERT_MS: u64 = 15 * 60_000;
pub const REPEAT_ALERT_MS: u64 = 5 * 60_000;
pub const CLOSE_TIMEOUT_MS: u64 = 60_000;
/// "Door closed after X.Y minutes" is only sent when the door was open at least this long.
pub const CLOSED_NOTICE_MIN_MS: u64 = 60_000;
/// Hardware watchdog timeout. The shell feeds it once, right before each `bot_step`.
pub const WDT_TIMEOUT_MS: u64 = 300_000;
/// Longest budget any single op may have.
pub const MAX_OP_BUDGET_MS: u64 = 240_000;
/// Allowance for everything in a loop pass that is not the op itself.
pub const LOOP_SLACK_MS: u64 = 10_000;
/// Continuous WiFi outage after which the device reboots.
pub const WIFI_REBOOT_AFTER_MS: u64 = 30 * 60_000;
/// With no successful poll for this long the bot is deaf (expired certificate, DNS, a block): reboot.
pub const DEAF_REBOOT_AFTER_MS: u64 = 30 * 60_000;
/// After this many failed sends in a row, the message that just failed is dropped.
pub const SEND_TRIES: u8 = 3;
pub const RELAY_PRESS_MS: u64 = 500;
/// The firmware asks for an update (checks the release manifest) at boot and then this often.
pub const OTA_CHECK_INTERVAL_MS: u64 = 6 * 3_600_000;
/// Longest version string `parse_version` accepts (e.g. `v65535.65535.65535+build`).
pub const VERSION_CAP: usize = 24;

pub const NAME_CAP: usize = 32;
pub const TEXT_CAP: usize = 512;
pub const CMD_TEXT_CAP: usize = 64;
pub const TMP_CAP: usize = 64;
pub const INBOX_CAP: usize = 8;
/// Containers nested deeper than this make the JSON scan fail.
pub const MAX_DEPTH: u8 = 32;

// ---------------------------------------------------------------- types
/// A Telegram username: 0..=32 bytes of [A-Za-z0-9_]. Bytes past `len` are zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Name {
    pub len: u8,
    pub bytes: [u8; NAME_CAP],
}
impl Name {
    pub const EMPTY: Name = Name { len: 0, bytes: [0; NAME_CAP] };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Cmd {
    Status,
    Open,
    Close,
    Silence,
    Update,
    Diag,
    Version,
    Unknown,
}

/// "Unix second `unix_s` was not yet over at monotonic time `mono_ms`" is false;
/// i.e. the true unix time at `mono_ms` was strictly below `unix_s` seconds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Anchor {
    pub unix_s: u64,
    pub mono_ms: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Age {
    /// No usable clock, no date on the message, or a date in the future.
    Unknown,
    /// Sent before this boot.
    PreBoot,
    /// Older than STALE_AFTER_S; payload = estimated age in seconds.
    Stale(u64),
    /// At most STALE_AFTER_S old; payload = estimated age in seconds (an upper bound).
    Fresh(u64),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Reply {
    Status { open: bool },
    AlreadyOpen,
    Opening,
    AlreadyClosed,
    Closing,
    Muted,
    UpdateQueued,
    Diag,
    Version,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Outgoing {
    pub who: Name,
    pub reply: Reply,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Decision {
    pub press: bool,
    pub reply: Option<Reply>,
    pub arm_close: bool,
    pub mute: bool,
    pub queue_update: bool,
}

/// One Telegram update, already reduced to what the core needs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct UpdateIn {
    pub update_id: u64,
    /// The message has text and comes from the configured chat.
    pub authorized: bool,
    pub cmd: Cmd,
    /// `message.date` (unix seconds), if present.
    pub date: Option<u64>,
    pub who: Name,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct PollResult {
    /// HTTP 200, complete well-formed body, `"ok":true`.
    pub ok: bool,
    /// Updates kept from this response.
    pub count: u32,
    /// The response's `Date` header as unix seconds.
    pub date_unix: Option<u64>,
    /// Monotonic ms read BEFORE the request was sent.
    pub t_req_ms: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct CloseReq {
    pub at: u64,
    pub who: Name,
    pub alerted: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct DoorState {
    pub open_since: Option<u64>,
    pub last_alert: Option<u64>,
    pub muted: bool,
    pub close: Option<CloseReq>,
}
pub const DOOR_IDLE: DoorState = DoorState { open_since: None, last_alert: None, muted: false, close: None };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct ClosedNotice {
    pub who: Name,
    /// Minutes the door was open, in tenths, rounded half up.
    pub tenths: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct DoorEvents {
    /// Whole minutes the door has been open.
    pub open_alert: Option<u64>,
    pub close_timeout: Option<Name>,
    pub closed_notice: Option<ClosedNotice>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct State {
    /// Smallest update id not yet processed; sent as `offset`.
    pub next_offset: u64,
    /// Updates of the current batch still to process.
    pub batch_left: u32,
    pub anchor: Option<Anchor>,
    /// Upper bound on the unix second of this boot, fixed by the first anchor.
    pub boot_unix: Option<u64>,
    pub last_poll: Option<u64>,
    /// The last poll succeeded and returned nothing (so Telegram holds no backlog and has our offset).
    pub quiesced: bool,
    pub update_pending: bool,
    pub pend_online: bool,
    pub pend_reply: Option<Outgoing>,
    pub pend_close_timeout: Option<Name>,
    pub pend_open_alert: Option<u64>,
    pub pend_closed_notice: Option<ClosedNotice>,
    pub door: DoorState,
    pub last_heartbeat: Option<u64>,
    pub wifi_down_since: Option<u64>,
    pub last_step: Option<u64>,
    /// Time of the last successful poll (None = none since boot).
    pub last_good_poll: Option<u64>,
    /// Failed sends in a row (always below SEND_TRIES).
    pub send_fails: u8,
}
pub const BOOT_STATE: State = State {
    next_offset: 0,
    batch_left: 0,
    anchor: None,
    boot_unix: None,
    last_poll: None,
    quiesced: false,
    update_pending: false,
    pend_online: true,
    pend_reply: None,
    pend_close_timeout: None,
    pend_open_alert: None,
    pend_closed_notice: None,
    door: DOOR_IDLE,
    last_heartbeat: None,
    wifi_down_since: None,
    last_step: None,
    last_good_poll: None,
    send_fails: 0,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct StepIn {
    /// Monotonic milliseconds since boot (never wraps: u64).
    pub now_ms: u64,
    pub door_open: bool,
    pub wifi_up: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Msg {
    Online,
    Reply(Outgoing),
    CloseTimeout { who: Name },
    OpenAlert { minutes: u64 },
    ClosedAfter(ClosedNotice),
}

/// The one blocking thing the shell does in this loop pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Op {
    Reboot,
    WifiConnect,
    Send(Msg),
    Poll { offset: u64 },
    NextUpdate,
    Ota,
    Idle,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct StepOut {
    pub state: State,
    pub op: Op,
    /// Some(uptime seconds) when a heartbeat line is due in this pass.
    pub heartbeat: Option<u64>,
}

// ---- over-the-air update
/// A release version MAJOR.MINOR.PATCH. Build metadata (`+...`) is not part of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

/// What the release manifest promises about the app image.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Manifest {
    pub version: Version,
    /// Bytes in the image file.
    pub size: u32,
    pub sha256: [u8; 32],
}

/// What the shell measured after writing the download into the spare slot.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Download {
    /// Bytes received and written.
    pub size: u32,
    /// SHA-256 over exactly those bytes.
    pub sha256: [u8; 32],
    /// esp_ota_end accepted the image (format, image hash and signature).
    pub image_ok: bool,
    /// The version in the written image's app descriptor, parsed (None = missing or unparseable).
    pub image_version: Option<Version>,
}

// ---- text
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Text {
    pub len: u16,
    /// Something did not fit. Once set it stays set and nothing more is appended.
    pub overflow: bool,
    pub buf: [u8; TEXT_CAP],
}
impl Text {
    pub const EMPTY: Text = Text { len: 0, overflow: false, buf: [0; TEXT_CAP] };
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Stats {
    pub loops: u64,
    pub http_ok: u32,
    pub http_fail: u32,
    pub sock_err: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Diag {
    pub door_open: bool,
    pub uptime_s: u64,
    pub free_mem: u32,
    pub rssi: i32,
    pub stats: Stats,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Level {
    Info,
    Warn,
    Error,
}

// ---- JSON scan
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum LexMode {
    Value,
    Str,
    Esc,
    Uni,
    Err,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Lex {
    pub mode: LexMode,
    /// Hex digits still expected in a \uXXXX escape.
    pub u_left: u8,
    pub u_acc: u16,
}
pub const LEX_START: Lex = Lex { mode: LexMode::Value, u_left: 0, u_acc: 0 };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Tok {
    /// Nothing to report (whitespace, or part of an escape).
    Skip,
    ObjOpen,
    ObjClose,
    ArrOpen,
    ArrClose,
    Colon,
    Comma,
    StrStart,
    StrByte(u8),
    StrEnd,
    /// One byte of a number / true / false / null.
    Scalar(u8),
    Bad,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Key {
    None,
    Other,
    Ok,
    Result,
    UpdateId,
    Message,
    Date,
    Text,
    Chat,
    From,
    Id,
    Username,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub enum Cap {
    None,
    Key,
    Scalar,
    Text,
    User,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct RawUpdate {
    pub update_id: Option<u64>,
    pub chat_id: Option<i64>,
    pub date: Option<u64>,
    pub has_text: bool,
    /// The text was longer than CMD_TEXT_CAP bytes (so it cannot be a command).
    pub text_over: bool,
    pub text_len: u8,
    pub text: [u8; CMD_TEXT_CAP],
    pub who: Name,
}
pub const RAW_EMPTY: RawUpdate = RawUpdate {
    update_id: None,
    chat_id: None,
    date: None,
    has_text: false,
    text_over: false,
    text_len: 0,
    text: [0; CMD_TEXT_CAP],
    who: Name::EMPTY,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Walk {
    pub err: bool,
    /// Containers currently open.
    pub depth: u8,
    /// How many of them, from the root, lie on the path
    /// root{ "result":[ update{ "message":{ "chat":{} | "from":{} } } ] }.  on <= depth, on <= 5.
    pub on: u8,
    /// At on == 5: true = inside "from", false = inside "chat".
    pub in_from: bool,
    /// The next string in the current on-path object is a key.
    pub expect_key: bool,
    /// Key of the member whose value comes next in the current on-path object.
    pub key: Key,
    pub cap: Cap,
    pub tmp_len: u8,
    pub tmp_over: bool,
    pub tmp: [u8; TMP_CAP],
    /// Root object carried `"ok":true`.
    pub ok_true: bool,
    /// A root container was opened and closed again.
    pub done: bool,
    pub cur: RawUpdate,
}
pub const WALK_START: Walk = Walk {
    err: false,
    depth: 0,
    on: 0,
    in_from: false,
    expect_key: false,
    key: Key::None,
    cap: Cap::None,
    tmp_len: 0,
    tmp_over: false,
    tmp: [0; TMP_CAP],
    ok_true: false,
    done: false,
    cur: RAW_EMPTY,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Scan {
    pub lx: Lex,
    pub w: Walk,
}
pub const SCAN_START: Scan = Scan { lx: LEX_START, w: WALK_START };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Inbox {
    pub head: u8,
    pub len: u8,
    pub items: [RawUpdate; INBOX_CAP],
}
pub const INBOX_EMPTY: Inbox = Inbox { head: 0, len: 0, items: [RAW_EMPTY; INBOX_CAP] };

/// Everything the firmware remembers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct Bot {
    pub core: State,
    pub scan: Scan,
    pub inbox: Inbox,
    pub stats: Stats,
    /// When the update timer last asked for an update check (None = not since boot).
    pub ota_last_check: Option<u64>,
}
pub const BOT_BOOT: Bot = Bot {
    core: BOOT_STATE,
    scan: SCAN_START,
    inbox: INBOX_EMPTY,
    stats: Stats { loops: 0, http_ok: 0, http_fail: 0, sock_err: 0 },
    ota_last_check: None,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(kani, derive(kani::Arbitrary))]
pub struct BotStep {
    pub bot: Bot,
    pub op: Op,
    pub heartbeat: Option<u64>,
}
// @end

#[cfg(any(kani, test, feature = "spec"))]
pub mod spec;
#[cfg(any(kani, test, feature = "spec"))]
pub use spec::*;

// @fns (generated list, one file per function)
include!("fns_list.rs");

#[cfg(kani)]
include!("props.rs");

#[cfg(all(kani, has_verifier_proofs))]
mod verifier_proofs;
