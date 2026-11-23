# GarageBot

A Telegram bot on an ESP32 that watches a garage door and can open or close it. It reads a reed
switch, presses the opener through a relay, answers commands in a group chat, and warns when the
door has been open too long.

The firmware is written in Rust. Its decision logic is proven with the Kani model checker; see
[docs/formal-verification.md](docs/formal-verification.md).

## Where things are

| Path | What |
|---|---|
| `firmware-rs/logic/` | The decision logic: pure Rust, `no_std`, no hardware. This is the proven part. |
| `firmware-rs/shell/` | The ESP32 firmware (ESP-IDF): WiFi, Telegram over HTTPS, pins, watchdog, OTA. It calls the logic once per loop pass and does the one thing it returns. Details in [firmware-rs/shell/README.md](firmware-rs/shell/README.md). |
| `firmware-rs/*.md`, `ROOT.txt`, `cache.py` | Contracts, the verifier's spec, the list of trusted assumptions, the proof log and the proof root. |
| `tools/proofq/` | The job queue that runs the Kani proofs. |
| `udp_logger.py`, `LOG_SERVER_SETUP.md` | The UDP log receiver the board sends its log lines to (port 5005). |
| `secrets.py.template` | Template for the untracked `secrets.py` (WiFi and Telegram values). |

## Build and flash

```
./firmware-rs/shell/build_and_flash.sh
```

The script builds and signs the app on a build host (`EE=user@host`, or the untracked file
`firmware-rs/shell/.build-host`), checks that the image holds no secrets, makes the secrets image
locally from `secrets.py`, and prints the `esptool` commands. It never flashes by itself. Secrets
live in the board's NVS partition, not in the app image. No eFuse is burned, so USB flashing
always works.

## Over-the-air updates

At boot and every 6 hours, when idle, the board reads the manifest of the latest
[GitHub release](https://github.com/doomdagadiggiedahdah/garage-bot/releases). If the version there
is newer, it downloads the signed app into the spare slot, checks size, SHA-256 and signature, and
reboots into it. The new image is kept only after its first good Telegram poll; otherwise the
bootloader goes back to the old one.

To release: bump `version` in `firmware-rs/shell/Cargo.toml`, run `remote_build.sh` on the build
host, and create the GitHub release `v<version>` with `garagebot-app.bin` and
`garagebot-manifest.txt` as assets. Pushing to `main` does not update the board.

`/version` in the chat shows the running version, slot and proof root.

## The Python version

The first GarageBot was MicroPython. Its last version is commit `2341d02`. `main.py` is still in
the tree for one reason: `build_and_flash.sh` reads the `NOTIFY_USERNAMES` list from it. It is not
what runs on the board.
