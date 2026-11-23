//! GarageBot firmware shell (TRUSTED.md T8). Every decision (timeouts, retries,
//! ordering, reboots, who may press) is in the proven `garagebot_logic` core.
//! This file only does what the core's `Op` says, one op per loop pass:
//!
//!   feed watchdog -> bot_step -> do that one op -> repeat
//!
//! The watchdog is fed in exactly one place: the top of the loop. Nothing inside
//! an op feeds it, so a stuck WiFi connect or HTTP call resets the board after
//! WDT_TIMEOUT_MS instead of hanging forever (the bug in main.py:120-160, which
//! fed the watchdog inside its infinite connect retry).
//!
//! Secrets (WiFi, Telegram token and chat id, NOTIFY) are not in the image: they
//! are read from the NVS partition at boot (namespace "gb"), which the laptop
//! writes over USB. Nothing over the network can change them.
//!
//! Over-the-air update (Op::Ota): the decisions (is it newer, does it fit, does
//! the download match, may the new image be marked valid) are the proven
//! `version_newer`/`ota_plan`/`ota_accept`/`ota_mark_valid`. This file only
//! fetches, hashes, writes and asks esp-idf to check the signature.

use std::net::UdpSocket;
use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::http::client::{Configuration as HttpCfg, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::sys;
use esp_idf_svc::wifi::{ClientConfiguration, Configuration, EspWifi};

use garagebot_logic::*;

// The app descriptor's version is CARGO_PKG_VERSION ("2.0.0+ota"): ota_accept
// reads it back from a downloaded image before booting it.
#[allow(unexpected_cfgs)]
mod app_desc {
    esp_idf_svc::sys::esp_app_desc!();
}

mod proof_root {
    include!(concat!(env!("OUT_DIR"), "/proof_root.rs"));
}
use proof_root::PROOF_ROOT;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// GitHub Releases on the repo the MicroPython OTA uses (it reads raw files from
/// the main branch; these release assets do not touch them).
const OTA_MANIFEST_URL: &str =
    "https://github.com/doomdagadiggiedahdah/garage-bot/releases/latest/download/garagebot-manifest.txt";
const OTA_APP_URL_PREFIX: &str = "https://github.com/doomdagadiggiedahdah/garage-bot/releases/download/v";
const OTA_APP_FILE: &str = "garagebot-app.bin";
/// The whole update op (manifest + image) stops at this many ms (budget is MAX_OP_BUDGET_MS).
const OTA_DEADLINE_MS: u64 = 200_000;
const MANIFEST_CAP: usize = 1024;
const API: &str = "https://api.telegram.org/bot";
/// Each HTTP op stops reading at this many ms after it started (budget is 30 s).
const HTTP_DEADLINE_MS: u64 = 25_000;
/// WiFi connect op polls for at most this long (budget is 20 s).
const WIFI_WAIT_MS: u64 = 19_000;

/// T8 item 3 and 9: 64-bit monotonic ms since boot, ~0 at reset.
fn now_ms() -> u64 {
    (unsafe { sys::esp_timer_get_time() } as u64) / 1000
}

fn feed_watchdog() {
    unsafe { sys::esp_task_wdt_reset() };
}

fn free_mem() -> u32 {
    unsafe { sys::esp_get_free_heap_size() }
}

fn rssi() -> i32 {
    let mut ap: sys::wifi_ap_record_t = unsafe { core::mem::zeroed() };
    if unsafe { sys::esp_wifi_sta_get_ap_info(&mut ap) } == sys::ESP_OK {
        ap.rssi as i32
    } else {
        0
    }
}

fn wifi_up(w: &EspWifi) -> bool {
    w.is_connected().unwrap_or(false)
        && w.sta_netif().get_ip_info().map(|i| !i.ip.is_unspecified()).unwrap_or(false)
}

fn text(t: &Text) -> &[u8] {
    &t.buf[..(t.len as usize).min(TEXT_CAP)]
}

/// Read at boot from NVS namespace "gb" (TRUSTED.md T13).
struct Secrets {
    ssid: String,
    pass: String,
    token: String,
    chat: i64,
    notify: String,
    log_addr: String,
}

fn load_secrets(part: EspDefaultNvsPartition) -> Result<Secrets, String> {
    let nvs = EspNvs::<NvsDefault>::new(part, "gb", false).map_err(|e| format!("open nvs namespace gb: {e}"))?;
    let get = |k: &str| -> Result<Option<String>, String> {
        let mut buf = [0u8; 256];
        match nvs.get_str(k, &mut buf) {
            Ok(v) => Ok(v.map(|s| s.trim_end_matches('\0').to_string())),
            Err(e) => Err(format!("read {k}: {e}")),
        }
    };
    let need = |k: &str| -> Result<String, String> {
        match get(k)? {
            Some(v) if !v.is_empty() => Ok(v),
            _ => Err(format!("missing {k}")),
        }
    };
    let ssid = need("wifi_ssid")?;
    let pass = need("wifi_pass")?;
    let token = need("tg_token")?;
    let chat = need("chat_id")?.trim().parse::<i64>().map_err(|_| "chat_id is not an integer".to_string())?;
    let notify = get("notify")?.unwrap_or_default();
    let log_addr = get("log_addr")?.filter(|s| !s.is_empty()).unwrap_or_else(|| "192.168.50.180:5005".into());
    if ssid.len() > 32 || pass.len() > 64 {
        return Err("ssid > 32 or password > 64 bytes".into());
    }
    Ok(Secrets { ssid, pass, token, chat, notify, log_addr })
}

struct Log {
    sock: Option<UdpSocket>,
    addr: String,
}

impl Log {
    /// One raw line to serial and, when WiFi is up, to the UDP log server.
    fn raw(&mut self, line: &[u8]) {
        println!("{}", String::from_utf8_lossy(line));
        if self.sock.is_none() {
            self.sock = UdpSocket::bind("0.0.0.0:0").ok();
        }
        if let Some(s) = &self.sock {
            let _ = s.send_to(line, self.addr.as_str());
        }
    }
    fn line(&mut self, level: Level, msg: &str) {
        let t = render_log(uptime_s(now_ms()), level, free_mem(), msg.as_bytes());
        self.raw(text(&t));
    }
}

/// One HTTPS exchange. Returns (status 200 and body read to its end, socket/timeout error, Date header).
/// Every body byte goes to `on_byte` in order (T8 item 5).
fn https(method: Method, url: &str, body: Option<&[u8]>, deadline: u64, mut on_byte: impl FnMut(u8)) -> (bool, bool, Vec<u8>) {
    https_chunks(false, method, url, body, deadline, |c| c.iter().for_each(|&x| on_byte(x)))
}

/// As `https`, body handed over in chunks. `big` = larger header buffers for
/// GitHub (its redirect Location headers are long); GET redirects are followed.
fn https_chunks(big: bool, method: Method, url: &str, body: Option<&[u8]>, deadline: u64, mut on_chunk: impl FnMut(&[u8])) -> (bool, bool, Vec<u8>) {
    let cfg = HttpCfg {
        timeout: Some(Duration::from_secs(10)),
        crt_bundle_attach: Some(sys::esp_crt_bundle_attach), // T8 item 10
        buffer_size: if big { Some(4096) } else { None },
        buffer_size_tx: if big { Some(2048) } else { None },
        ..Default::default()
    };
    let mut c = match EspHttpConnection::new(&cfg) {
        Ok(c) => c,
        Err(_) => return (false, true, Vec::new()),
    };
    let len = body.map(|b| b.len().to_string()).unwrap_or_default();
    let mut hdrs: Vec<(&str, &str)> = Vec::new();
    if body.is_some() {
        hdrs.push(("Content-Type", "application/json"));
        hdrs.push(("Content-Length", &len));
    }
    if c.initiate_request(method, url, &hdrs).is_err() {
        return (false, true, Vec::new());
    }
    if let Some(b) = body {
        if c.write_all(b).is_err() {
            return (false, true, Vec::new());
        }
    }
    if c.initiate_response().is_err() {
        return (false, true, Vec::new());
    }
    let status = c.status();
    let date = c.header("Date").map(|s| s.as_bytes().to_vec()).unwrap_or_default();
    let mut buf = [0u8; 1024];
    loop {
        if now_ms() > deadline {
            return (false, true, date);
        }
        match c.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => on_chunk(&buf[..n]),
            Err(_) => return (false, true, date),
        }
    }
    (status == 200, false, date)
}

// ---- over-the-air update (TRUSTED.md T14)

fn cstr_bytes(c: &[core::ffi::c_char]) -> Vec<u8> {
    c.iter().take_while(|&&x| x != 0).map(|&x| x as u8).collect()
}

/// Version string in an app slot's descriptor, parsed by the proven parse_version.
fn slot_version(part: *const sys::esp_partition_t) -> Option<Version> {
    if part.is_null() {
        return None;
    }
    let mut d: sys::esp_app_desc_t = unsafe { core::mem::zeroed() };
    if unsafe { sys::esp_ota_get_partition_description(part, &mut d) } != sys::ESP_OK {
        return None;
    }
    parse_version(&cstr_bytes(&d.version))
}

fn running_label() -> String {
    let p = unsafe { sys::esp_ota_get_running_partition() };
    if p.is_null() {
        return "?".into();
    }
    String::from_utf8_lossy(&cstr_bytes(unsafe { &(*p).label })).into_owned()
}

/// True while the running image is on probation (booted by OTA, not yet marked valid).
fn running_pending_verify() -> bool {
    let p = unsafe { sys::esp_ota_get_running_partition() };
    let mut st: sys::esp_ota_img_states_t = 0;
    !p.is_null()
        && unsafe { sys::esp_ota_get_state_partition(p, &mut st) } == sys::ESP_OK
        && st == sys::esp_ota_img_states_t_ESP_OTA_IMG_PENDING_VERIFY
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Manifest text: `key=value` lines; version, size and sha256 (64 hex) are required.
fn parse_manifest(body: &[u8]) -> Option<Manifest> {
    let text = core::str::from_utf8(body).ok()?;
    let (mut v, mut size, mut sha) = (None, None, None);
    for line in text.lines() {
        let Some((k, val)) = line.split_once('=') else { continue };
        match k.trim() {
            "version" => v = parse_version(val.trim().as_bytes()),
            "size" => size = val.trim().parse::<u32>().ok(),
            "sha256" => sha = unhex32(val),
            _ => {}
        }
    }
    Some(Manifest { version: v?, size: size?, sha256: sha? })
}

struct Sha(sys::mbedtls_sha256_context);
impl Sha {
    fn new() -> Self {
        let mut c: sys::mbedtls_sha256_context = unsafe { core::mem::zeroed() };
        unsafe {
            sys::mbedtls_sha256_init(&mut c);
            sys::mbedtls_sha256_starts(&mut c, 0);
        }
        Sha(c)
    }
    fn update(&mut self, b: &[u8]) {
        unsafe { sys::mbedtls_sha256_update(&mut self.0, b.as_ptr(), b.len() as _) };
    }
    fn finish(mut self) -> [u8; 32] {
        let mut out = [0u8; 32];
        unsafe {
            sys::mbedtls_sha256_finish(&mut self.0, out.as_mut_ptr());
            sys::mbedtls_sha256_free(&mut self.0);
        }
        out
    }
}

/// One update check. Returns only when there is nothing to install or the
/// install was refused; on success it switches the boot slot and restarts.
fn ota_check(log: &mut Log) {
    let t0 = now_ms();
    let deadline = t0 + OTA_DEADLINE_MS;
    let Some(running) = parse_version(VERSION.as_bytes()) else {
        log.line(Level::Warn, "ota: own version unparseable, skipping");
        return;
    };
    // The manifest: small text, read to its end, capped.
    let mut body = Vec::new();
    let (ok, _, _) = https_chunks(true, Method::Get, OTA_MANIFEST_URL, None, deadline, |c| {
        let room = MANIFEST_CAP.saturating_sub(body.len());
        body.extend_from_slice(&c[..c.len().min(room)]);
    });
    if !ok {
        log.line(Level::Info, "ota: no manifest (no release yet, or network error)");
        return;
    }
    let Some(m) = parse_manifest(&body) else {
        log.line(Level::Warn, "ota: manifest unparseable");
        return;
    };
    let slot = unsafe { sys::esp_ota_get_next_update_partition(core::ptr::null()) };
    if slot.is_null() {
        log.line(Level::Warn, "ota: no spare app slot");
        return;
    }
    let slot_size = unsafe { (*slot).size };
    let bad = slot_version(unsafe { sys::esp_ota_get_last_invalid_partition() });
    if !ota_plan(running, m, slot_size, bad) {
        log.line(Level::Info, &format!("ota: up to date ({VERSION}; release {}.{}.{})", m.version.major, m.version.minor, m.version.patch));
        return;
    }
    log.line(Level::Info, &format!("ota: downloading {}.{}.{} ({} bytes)", m.version.major, m.version.minor, m.version.patch, m.size));
    let mut h: sys::esp_ota_handle_t = 0;
    if unsafe { sys::esp_ota_begin(slot, m.size as usize, &mut h) } != sys::ESP_OK {
        log.line(Level::Warn, "ota: esp_ota_begin failed");
        return;
    }
    let url = format!("{OTA_APP_URL_PREFIX}{}.{}.{}/{OTA_APP_FILE}", m.version.major, m.version.minor, m.version.patch);
    let mut sha = Sha::new();
    let mut got: u32 = 0;
    let mut write_ok = true;
    let (ok, _, _) = https_chunks(true, Method::Get, &url, None, deadline, |c| {
        // Never write past the manifest size (which ota_plan checked fits the slot).
        let room = (m.size.saturating_sub(got)) as usize;
        let take = &c[..c.len().min(room)];
        sha.update(take);
        if c.len() > take.len() {
            got = got.saturating_add(c.len() as u32); // over-long: size check refuses it
        } else {
            got += take.len() as u32;
        }
        if write_ok && !take.is_empty() && unsafe { sys::esp_ota_write(h, take.as_ptr() as *const _, take.len()) } != sys::ESP_OK {
            write_ok = false;
        }
    });
    let digest = sha.finish();
    if !ok || !write_ok {
        unsafe { sys::esp_ota_abort(h) };
        log.line(Level::Warn, if ok { "ota: flash write failed" } else { "ota: download failed" });
        return;
    }
    // esp_ota_end checks the image format, its hash and (signed apps) its ECDSA signature.
    let image_ok = unsafe { sys::esp_ota_end(h) } == sys::ESP_OK;
    let dl = Download { size: got, sha256: digest, image_ok, image_version: if image_ok { slot_version(slot) } else { None } };
    if !ota_accept(running, m, slot_size, dl) {
        log.line(Level::Warn, &format!("ota: refused (image_ok={image_ok}, size {got}/{}, sha match {})", m.size, digest == m.sha256));
        return;
    }
    if unsafe { sys::esp_ota_set_boot_partition(slot) } != sys::ESP_OK {
        log.line(Level::Warn, "ota: set boot partition failed");
        return;
    }
    log.line(Level::Warn, "ota: installed, restarting into the new image (pending verify)");
    FreeRtos::delay_ms(200);
    unsafe { sys::esp_restart() };
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    let p = Peripherals::take()?;

    // Relay low before anything else: a floating pin at reset must not press.
    let mut relay = PinDriver::output(p.pins.gpio14)?;
    relay.set_low()?;
    let door = PinDriver::input(p.pins.gpio32, Pull::Up)?; // high = open (reed switch), as in main.py

    // T8 item 7: watchdog armed at WDT_TIMEOUT_MS before the first network call, panic = reset.
    let wdt = sys::esp_task_wdt_config_t {
        timeout_ms: WDT_TIMEOUT_MS as u32,
        idle_core_mask: 0,
        trigger_panic: true,
    };
    unsafe {
        sys::esp_task_wdt_reconfigure(&wdt);
        sys::esp_task_wdt_add(core::ptr::null_mut());
    }

    let sysloop = EspSystemEventLoop::take()?;
    // reinit=false: never erase the NVS partition, it holds the secrets.
    let nvs = EspDefaultNvsPartition::take_with(false)?;
    let sec = match load_secrets(nvs.clone()) {
        Ok(s) => s,
        Err(e) => {
            // No secrets, nothing to do: say why on serial, then retry from a reset.
            // The watchdog (armed above) would reset us anyway.
            println!("GarageBot {VERSION}: NVS secrets unusable ({e}); flash the nvs image from build_and_flash.sh");
            FreeRtos::delay_ms(30_000);
            unsafe { sys::esp_restart() }
        }
    };
    let mut wifi = EspWifi::new(p.modem, sysloop, Some(nvs))?;
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: sec.ssid.as_str().try_into().map_err(|_| anyhow::anyhow!("ssid"))?,
        password: sec.pass.as_str().try_into().map_err(|_| anyhow::anyhow!("password"))?,
        ..Default::default()
    }))?;
    wifi.start()?;

    let mut log = Log { sock: None, addr: sec.log_addr.clone() };
    let slot = running_label();
    // What /version and the boot message show: version, running slot, short proof root.
    let version_line = format!("{VERSION} {slot} {PROOF_ROOT}");
    let mut pending = running_pending_verify();
    log.line(Level::Info, &format!("GarageBot {version_line} (Rust) booting{}", if pending { ", pending verify" } else { "" }));
    let poll_base = format!("{API}{}/", sec.token);
    let send_url = format!("{API}{}/sendMessage", sec.token);
    let chat_id = sec.chat;
    let notify = sec.notify.clone();
    let mut bot = BOT_BOOT;

    loop {
        feed_watchdog(); // the only feed (T8 item 1)
        let door_open = door.is_high();
        let bs = bot_step(bot, StepIn { now_ms: now_ms(), door_open, wifi_up: wifi_up(&wifi) });
        bot = bs.bot;
        // A new image stays on probation until the core has seen one good poll.
        if ota_mark_valid(pending, bot.core.last_good_poll) {
            if unsafe { sys::esp_ota_mark_app_valid_cancel_rollback() } == sys::ESP_OK {
                pending = false;
                log.line(Level::Info, "ota: first good poll, image marked valid (rollback cancelled)");
            }
        }
        let diag = || Diag { door_open, uptime_s: uptime_s(now_ms()), free_mem: free_mem(), rssi: rssi(), stats: bot.stats };
        if bs.heartbeat.is_some() {
            let d = diag();
            log.raw(text(&render_heartbeat(d)));
            log.raw(text(&render_heap(d)));
        }
        match bs.op {
            Op::Reboot => {
                log.line(Level::Warn, "core ordered reboot");
                FreeRtos::delay_ms(200);
                unsafe { sys::esp_restart() };
            }
            Op::WifiConnect => {
                // Bounded, never feeds the watchdog: if WiFi never comes up the core reboots us.
                let t0 = now_ms();
                let _ = wifi.disconnect();
                if wifi.connect().is_ok() {
                    while now_ms() - t0 < WIFI_WAIT_MS && !wifi_up(&wifi) {
                        FreeRtos::delay_ms(250);
                    }
                }
                log.line(Level::Info, if wifi_up(&wifi) { "wifi up" } else { "wifi connect attempt failed" });
            }
            Op::Send(m) => {
                let t0 = now_ms();
                let body = render_send_body(chat_id, &render_msg(m, notify.as_bytes(), version_line.as_bytes(), diag()));
                let (ok, sock_err, _) = if body.overflow {
                    (false, false, Vec::new())
                } else {
                    https(Method::Post, &send_url, Some(text(&body)), t0 + HTTP_DEADLINE_MS, |_| {})
                };
                bot = bot_http_result(bot, ok, sock_err);
                bot = bot_send_done(bot, m, ok); // T8 item 8, same m, before the next step
                if !ok {
                    log.line(Level::Warn, "send failed");
                }
            }
            Op::Poll { offset } => {
                let path = render_poll_path(offset);
                let url = format!("{poll_base}{}", core::str::from_utf8(text(&path)).unwrap_or(""));
                let t_req = now_ms(); // T8 item 4: before the request is sent
                let mut b = bot;
                let (ok, sock_err, date) = https(Method::Get, &url, None, t_req + HTTP_DEADLINE_MS, |x| b = bot_feed(b, x));
                b = bot_http_result(b, ok, sock_err);
                bot = bot_poll_done(b, ok, &date, t_req, now_ms());
                if !ok {
                    log.line(Level::Warn, "poll failed");
                }
            }
            Op::NextUpdate => {
                let now = now_ms();
                // Log only, no decision: peek at the update the core is about to take.
                let peek = inbox_pop(bot.inbox).1.and_then(|r| to_update_in(r, chat_id));
                let before = bot.core;
                let (b, press) = bot_next_update(bot, chat_id, now, door_open);
                bot = b;
                if let Some(u) = peek {
                    let guarded = matches!(u.cmd, Cmd::Open | Cmd::Close | Cmd::Silence | Cmd::Update);
                    let taken = bot.core.next_offset > before.next_offset && bot.core.pend_reply.is_none() && !press;
                    if u.authorized && guarded && taken {
                        let why = match judge_age(before.anchor, before.boot_unix, now, u.date) {
                            Age::Unknown => "no usable clock".to_string(),
                            Age::PreBoot => "sent before this boot".to_string(),
                            Age::Stale(a) => format!("{a}s old"),
                            Age::Fresh(_) => "?".to_string(),
                        };
                        log.line(Level::Warn, &format!("dropped {:?} silently ({why})", u.cmd));
                    }
                }
                if press {
                    // T8 item 2: once, for RELAY_PRESS_MS.
                    relay.set_high()?;
                    FreeRtos::delay_ms(RELAY_PRESS_MS as u32);
                    relay.set_low()?;
                    log.line(Level::Info, "relay pressed");
                }
            }
            Op::Ota => ota_check(&mut log),
            Op::Idle => FreeRtos::delay_ms(200),
        }
    }
}
