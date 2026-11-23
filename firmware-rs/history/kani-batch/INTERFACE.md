# garagebot-kani: public interface

Crate name: `garagebot_kani`. A pure port of the timing and decision logic in
`garage-bot/main.py` (MicroPython, ESP32). No I/O: tick values, wall-clock
seconds, the door sensor and network outcomes are all inputs.

Proof harnesses go in `src/proofs.rs`, which `lib.rs` already includes as
`#[cfg(kani)] mod proofs;`. Types that harnesses are likely to want as
`kani::any()` (`Cmd`, `UpdateKind`) derive `kani::Arbitrary` under `cfg(kani)`.
All fields listed below are `pub`.

## Units and conventions

- **Ticks**: `u32` values of `time.ticks_ms()`, milliseconds, always `< 2^30`.
  On ESP32, MicroPython's tick period is `2^30` ms (12.43 days), after which the
  counter wraps to 0.
- **Real time**: `u64` milliseconds since boot, never wraps. It does not exist
  in main.py; it is a ghost clock for tests and proofs. The only bridge is
  `ticks_ms_at`.
- **Unix time**: `i64` seconds (`now_unix()`, Telegram `message.date`).
- **Uptime**: `u64` seconds.
- **update_id**: `u64`, Telegram's increasing update number.

## `garagebot_kani::ticks`

```rust
pub const TICKS_PERIOD: u32 = 1 << 30;      // wrap width of ticks_ms(), in ms
pub const TICKS_MASK: u32 = TICKS_PERIOD - 1;
pub const TICKS_HALF: i32 = 1 << 29;        // ticks_diff results lie in [-TICKS_HALF, TICKS_HALF - 1]
pub const MAX_INTERVAL_MS: u64 = (1 << 29) - 1; // documented max interval ticks_diff can measure
pub const POLL_INTERVAL_MS: i32 = 5_000;    // POLL_INTERVAL_SECONDS * 1000

/// Value of time.ticks_ms() at real time `real_ms`: `real_ms mod 2^30`.
pub fn ticks_ms_at(real_ms: u64) -> u32;

/// time.ticks_diff(end, start): signed ms from `start` to `end`, computed modulo
/// the tick period. Precondition: both arguments < TICKS_PERIOD.
pub fn ticks_diff(end: u32, start: u32) -> i32;

/// The main loop's gate for polling Telegram:
/// `ticks_diff(current_time, last_poll_time) >= POLL_INTERVAL_SECONDS * 1000`.
pub fn poll_due(current_time: u32, last_poll_time: u32) -> bool;

/// Whole minutes the door has been open, as shown in the "open for N minutes"
/// alert: `int(ticks_diff(current_time, open_start_time) / 60000)`.
pub fn door_open_minutes(current_time: u32, open_start_time: u32) -> i32;
```

## `garagebot_kani::close_timeout`

The "Door did not close!" warning after a close requested through the bot.

```rust
pub const CLOSE_TIMEOUT_MS: i32 = 60_000;   // CLOSE_TIMEOUT_SECONDS * 1000

/// The three main.py globals of the same names. `Default` is the idle state
/// (false, None, false), which is also what main() sets before the loop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseWatch {
    pub bot_triggered_close: bool,
    pub close_requested_time: Option<u32>,  // ticks
    pub close_timeout_alerted: bool,
}

impl CloseWatch {
    /// The `close` branch of handle_command. `door_open` is the sensor reading;
    /// `now_ticks` is the ticks_ms() sampled inside the handler (it can be later
    /// than the `current_time` the same loop iteration passes to `monitor`).
    /// Returns true if the garage button is pressed (i.e. the door was open).
    pub fn on_close_command(&mut self, door_open: bool, now_ticks: u32) -> bool;

    /// The close-timeout part of the door-monitor section, run once per main-loop
    /// iteration, after any command handling. `current_time` is the ticks value
    /// sampled at the top of that iteration. Returns true iff the warning message
    /// is sent in this iteration.
    pub fn monitor(&mut self, door_open: bool, current_time: u32) -> bool;
}
```

## `garagebot_kani::uptime`

```rust
pub const HEARTBEAT_INTERVAL_MS: i32 = 60_000; // HEARTBEAT_INTERVAL_SECONDS * 1000

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uptime {
    pub uptime_seconds: u64,        // accumulated uptime, seconds
    pub last_heartbeat_time: u32,   // ticks
}

impl Uptime {
    /// State at loop entry: uptime 0, last heartbeat = `now_ticks`.
    pub fn new(now_ticks: u32) -> Self;

    /// The heartbeat section of one loop iteration; `current_time` is that
    /// iteration's ticks value. Returns true iff a heartbeat fired (which is the
    /// only time `uptime_seconds` and `last_heartbeat_time` change).
    pub fn step(&mut self, current_time: u32) -> bool;
}
```

## `garagebot_kani::stale`

```rust
pub const MAX_COMMAND_AGE_SECONDS: i64 = 60;

/// What handle_command recognises. `Unknown` is any other text (it gets no reply).
/// Silence covers silence/quiet/stop/mute; Debug covers debug/info.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd { Status, Open, Close, Silence, Update, Debug, Unknown }

impl Cmd {
    /// Message text to command, as handle_command matches it. (String handling;
    /// not intended for proofs.)
    pub fn parse(text: &str) -> Cmd;

    /// Whether the command is in STALE_GUARDED_COMMANDS (open, close, update).
    pub fn is_stale_guarded(self) -> bool;
}

/// main.py stale_command_age(): `Some(age_seconds)` if the command must be
/// refused as too old, else `None`.
/// - `clock_synced`: whether NTP has set the RTC
/// - `date`: Telegram message.date, Unix seconds; `None` and `Some(0)` both mean "absent"
/// - `now_unix`: current Unix seconds
pub fn stale_command_age(clock_synced: bool, cmd: Cmd, date: Option<i64>, now_unix: i64) -> Option<i64>;
```

## `garagebot_kani::poll`

The Telegram poll/response logic as a state machine. One poll round is:

1. `state.next_request()` says which HTTP request the bot makes.
2. The caller supplies that request's outcome to `on_flush` or `on_updates`.
3. For each update received, the machine reports one `Action` through
   `PollEnv::act` and advances `last_update_id`.

```rust
/// Outcome of one HTTP request. Timeout = OSError in main.py, Error = any other
/// exception or an unusable response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Net<T> { Ok(T), Timeout, Error }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// flush_pending_updates(): getUpdates?offset=-1. Telegram returns only the
    /// newest pending update and forgets all older ones.
    Flush,
    /// get_telegram_updates(): getUpdates?offset=N. Telegram forgets every update
    /// with id < N and returns the pending ones with id >= N, in increasing order.
    GetUpdates { offset: u64 },
}

/// Body of a flush response that arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlushResponse {
    pub ok: bool,               // Telegram's "ok" field
    pub newest: Option<u64>,    // update_id of the returned update, None if the queue was empty
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateKind {
    NonText,        // no message, or a message without text
    Unauthorized,   // text from a chat other than TELEGRAM_CHAT_ID
    Command { cmd: Cmd, date: Option<i64> },  // text from the right chat; date = message.date
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Update { pub update_id: u64, pub kind: UpdateKind }

/// Number of send_telegram_message attempts made in response to one update.
/// An attempt is counted whether or not the send succeeds; sends are not retried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replies {
    Zero,
    One,
    Several,  // more than one (the `update` command when a new version exists)
}

/// What the bot does with one update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing sent, nothing executed.
    Skip,
    /// The command is refused as stale: not executed, one "Ignored ..." reply attempt.
    IgnoredStale { age: i64 },
    /// handle_command(cmd) runs (for Open/Close this is where the door can move),
    /// followed by `replies` reply attempts.
    Handled { cmd: Cmd, replies: Replies },
}

impl Action {
    /// Reply attempts this action makes.
    pub fn replies(self) -> Replies;
}

/// Reply attempts made when handle_command runs `cmd`. `update_available` is the
/// result of the version check and only matters for `Cmd::Update`.
pub fn handle_command_replies(cmd: Cmd, update_available: bool) -> Replies;

/// The outside world as seen while handling one update. Implemented by the
/// caller (a test or a proof harness).
pub trait PollEnv {
    /// now_unix() at the moment an update is examined. Called at most once per
    /// update, only for `UpdateKind::Command`, and before `act` for that update.
    fn now_unix(&mut self) -> i64;
    /// Whether the version check finds a newer version. Called only when an
    /// `update` command is actually handled.
    fn update_available(&mut self) -> bool;
    /// Called exactly once per processed update, in order, with the decision.
    fn act(&mut self, update: &Update, action: Action);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollState {
    pub last_update_id: u64,    // LAST_UPDATE_ID
    pub backlog_flushed: bool,  // whether a flush has succeeded since boot
    pub clock_synced: bool,     // set by NTP sync, outside this machine; the caller may set it to true at any time
}

impl PollState {
    /// State at the start of main(): last_update_id 0, backlog not flushed.
    /// main() then performs one flush (`next_request()` is `Flush`, result to
    /// `on_flush`) before entering the loop; later rounds happen inside the loop.
    pub fn boot(clock_synced: bool) -> Self;

    /// The request the next poll round makes.
    pub fn next_request(&self) -> Request;

    /// Feed the outcome of a `Request::Flush`.
    pub fn on_flush(&mut self, res: Net<FlushResponse>);

    /// Feed the outcome of a `Request::GetUpdates`. `Ok` carries the updates
    /// Telegram returned (possibly empty), in order; each is passed to
    /// `process_update`.
    pub fn on_updates(&mut self, res: Net<&[Update]>, env: &mut impl PollEnv);

    /// Handle a single received update: the body of main.py's `for update in
    /// updates:` loop. Returns the same `Action` it reports to `env.act`.
    pub fn process_update(&mut self, u: &Update, env: &mut impl PollEnv) -> Action;
}
```

## Not in the interface

Not ported: WiFi, NTP, OTA download, the relay, UDP logging, crash dumps, the
hardware watchdog (WATCHDOG_TIMEOUT_MS is only handed to `machine.WDT`; no tick
arithmetic uses it), the open-door alert schedule beyond `door_open_minutes`,
and the mute flag.
