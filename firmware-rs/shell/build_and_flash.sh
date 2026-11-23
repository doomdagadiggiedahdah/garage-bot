#!/usr/bin/env bash
# GarageBot Rust firmware: build the SECRET-FREE app on eE, make the NVS (secrets)
# image HERE on the laptop, print the flash commands. It never flashes by itself.
#
#   ./firmware-rs/shell/build_and_flash.sh           # secrets from <garage-bot>/secrets.py
#   SECRETS=/path/fake_secrets.py ./build_and_flash.sh   # e.g. synthetic test values
#
# Secrets never leave this machine: eE gets no input at all, and the NVS image is
# written to $XDG_RUNTIME_DIR (tmpfs, mode 700), never into the repo.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
GB="$(dirname "$(dirname "$HERE")")"    # the garage-bot folder (secrets.py, main.py)
# Build host (user@host): from $EE, else from the untracked file .build-host next to this script.
EE="${EE:-$(cat "$HERE/.build-host" 2>/dev/null || true)}"
[ -n "$EE" ] || { echo "set EE=user@build-host, or write it to $HERE/.build-host" >&2; exit 1; }
SECRETS="${SECRETS:-$GB/secrets.py}"
OUT="$HERE/out"                          # secret-free files only
NVSDIR="${XDG_RUNTIME_DIR:-/tmp}/garagebot-nvs"
umask 077
mkdir -p "$OUT" "$NVSDIR"; chmod 700 "$NVSDIR"

# 1. Secret-free build on eE (stdin closed: nothing is sent).
ssh "$EE" 'bash ~/garagebot-rs/shell/remote_build.sh' </dev/null
for f in bootloader.bin partition-table.bin garagebot-app.bin garagebot-manifest.txt; do
    scp -q "$EE:garagebot-rs/shell/target/ota-out/$f" "$OUT/$f"
done

# 2. Prove the app carries no secrets: no build-time secret names, no token-shaped string.
if grep -a -q -E 'GB_WIFI_SSID|GB_WIFI_PASSWORD|GB_TELEGRAM_BOT_TOKEN|GB_TELEGRAM_CHAT_ID|GB_NOTIFY|secrets\.rs' "$OUT/garagebot-app.bin" >/dev/null \
   || grep -a -q -E '[0-9]{8,10}:[A-Za-z0-9_-]{35}' "$OUT/garagebot-app.bin"; then
    echo "REFUSING: garagebot-app.bin contains a secret marker or a bot-token-shaped string" >&2; exit 1
fi
echo "app is secret-free (no GB_* names, no token-shaped string)"

# 3. NVS image from secrets.py, locally. NOTIFY comes from main.py's NOTIFY_USERNAMES.
CSV="$NVSDIR/nvs.csv"
trap 'rm -f "$CSV"' EXIT
python3 - "$SECRETS" "$GB/main.py" "$CSV" <<'PY'
import ast, csv, runpy, sys
sec_path, main_py, out = sys.argv[1:]
s = runpy.run_path(sec_path)
notify = []
for n in ast.walk(ast.parse(open(main_py).read())):
    if isinstance(n, ast.Assign) and any(getattr(t, "id", None) == "NOTIFY_USERNAMES" for t in n.targets):
        notify = ast.literal_eval(n.value)
vals = {
    "wifi_ssid": s["SSID"],
    "wifi_pass": s["PASSWORD"],
    "tg_token": s["TELEGRAM_BOT_TOKEN"],
    "chat_id": str(int(s["TELEGRAM_CHAT_ID"])),
    "notify": " ".join("@" + u for u in notify),
}
if "LOG_ADDR" in s:
    vals["log_addr"] = s["LOG_ADDR"]
assert len(vals["wifi_ssid"].encode()) <= 32 and len(vals["wifi_pass"].encode()) <= 64
with open(out, "w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["key", "type", "encoding", "value"])
    w.writerow(["gb", "namespace", "", ""])
    for k, v in vals.items():
        w.writerow([k, "data", "string", v])
PY
uv run --quiet --no-project --with esp-idf-nvs-partition-gen python -m esp_idf_nvs_partition_gen generate "$CSV" "$NVSDIR/nvs.bin" 0x6000 >/dev/null
rm -f "$CSV"
echo "nvs image (secrets, local only): $NVSDIR/nvs.bin"
cat "$OUT/garagebot-manifest.txt"

cat <<MSG

Flash it yourself (board on USB; first time REPLACES MicroPython, erase also resets the OTA slot choice):
  uvx esptool --chip esp32 -p /dev/ttyUSB0 erase_flash
  uvx esptool --chip esp32 -p /dev/ttyUSB0 -b 460800 write_flash --flash_mode dio --flash_freq 40m --flash_size 4MB \\
      0x1000 "$OUT/bootloader.bin" 0x8000 "$OUT/partition-table.bin" \\
      0x9000 "$NVSDIR/nvs.bin" 0x20000 "$OUT/garagebot-app.bin"
Change secrets only (keeps the app):  uvx esptool --chip esp32 -p /dev/ttyUSB0 write_flash 0x9000 "$NVSDIR/nvs.bin"
Watch it boot:
  uvx --from pyserial pyserial-miniterm /dev/ttyUSB0 115200
Back to MicroPython: erase_flash, then
  uvx esptool --chip esp32 -p /dev/ttyUSB0 write_flash 0x1000 "$GB/ESP32_GENERIC-20250911-v1.26.1.bin"
  and upload main.py + secrets.py again.
MSG
