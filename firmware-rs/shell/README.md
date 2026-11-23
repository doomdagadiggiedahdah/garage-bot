# GarageBot firmware (Rust, ESP32)

The ESP32 firmware for the proven `garagebot-logic` core. `src/main.rs` is the
shell from TRUSTED.md T8: one loop that feeds the watchdog, calls `bot_step`
once and does the one op it returns. Every decision is in the proven core.

- Pins as in the old MicroPython `main.py` (git history, commit `2341d02`): reed switch GPIO32 (pull-up, high = open), relay GPIO14 (500 ms press).
- Telegram over HTTPS, with the server certificate checked against the ESP-IDF CA bundle.
- UDP log lines to `log_addr` (default the LAN log server on port 5005, same format as main.py).
- Watchdog: 300 s, panic = reset, fed only at the top of the loop. A WiFi connect
  attempt is bounded at 20 s and never feeds it. That fixes the main.py:120-160 bug, where
  the infinite connect retry kept feeding the watchdog.
- `/version`: version, running slot and the short proof root, e.g.
  `GarageBot 2.0.0+ota ota_0 6338fb2a`.

## Flash layout (4 MB, `partitions.csv`)

| Offset | What |
|---|---|
| 0x1000 | bootloader (with app rollback) |
| 0x8000 | partition table |
| 0x9000 | `nvs` 24 KB: the secrets (TRUSTED.md T13) |
| 0xf000 | `otadata`: which slot boots |
| 0x20000 | `ota_0` app slot, 1.875 MB |
| 0x200000 | `ota_1` app slot, 1.875 MB |

## Secrets (in NVS, never in the image)

Namespace `gb`: `wifi_ssid`, `wifi_pass`, `tg_token`, `chat_id` (integer, as text),
`notify` (e.g. `@a @b`), `log_addr` (optional). The app reads them at boot; if one is
missing it says so on serial and restarts after 30 s. No Telegram command can change them.

## Build and flash

From the garage-bot folder: `./firmware-rs/shell/build_and_flash.sh`. The build host comes
from `EE=user@host` or from the untracked file `shell/.build-host`.

1. eE builds the secret-free app (`remote_build.sh`): `cargo build`, then
   `esptool elf2image`, then `espsecure sign_data` with the key in
   `~/.garagebot-signing/` on eE (outside git). Nothing is sent to eE.
2. The script copies back `out/bootloader.bin`, `out/partition-table.bin`,
   `out/garagebot-app.bin` and `out/garagebot-manifest.txt`, and refuses to go on
   if the app contains a `GB_*` name or a bot-token-shaped string.
3. It makes the NVS image here on the laptop from `secrets.py` and the mention list in `notify.txt`
   (untracked; copy `notify.txt.example`)
   with `esp_idf_nvs_partition_gen`, into `$XDG_RUNTIME_DIR/garagebot-nvs/nvs.bin`.
4. It prints the esptool commands. It never flashes by itself.

`SECRETS=/path/to/fake_secrets.py ./build_and_flash.sh` uses another secrets file (for tests).

## Over-the-air updates (TRUSTED.md T14)

At boot and every 6 h, when the bot is idle, it fetches
`https://github.com/doomdagadiggiedahdah/garage-bot/releases/latest/download/garagebot-manifest.txt`:

```
version=2.0.1
size=<bytes of garagebot-app.bin>
sha256=<hex of garagebot-app.bin>
proof_root=<ROOT.txt root>
```

It installs only if the version is strictly newer than the running one (build
metadata after `+` ignored) and the size fits a slot, and if this version was not
rolled back before. It downloads `releases/download/v<version>/garagebot-app.bin` into
the spare slot and boots it only if ESP-IDF accepts the ECDSA signature and the size,
SHA-256 and embedded version match the manifest. The new image runs on probation: it
marks itself valid after its first good Telegram poll. If it resets before that, the
bootloader goes back to the old slot.

These are release assets. The old MicroPython OTA read `version.txt` and `main.py` from
the main branch; `version.txt` was removed from main after commit `2341d02`, so a board
still on MicroPython no longer updates itself.

To publish a release (by hand): bump `version` in `Cargo.toml`, run `remote_build.sh`
on eE, create GitHub release `v<version>` with `target/ota-out/garagebot-app.bin` and
`garagebot-manifest.txt` as assets. `latest` must point to it.

Signed apps without secure boot: no eFuse is burned and flash is not encrypted, so
USB flashing always works. The signature protects only against a network attacker.
