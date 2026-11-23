#!/bin/bash
# Runs on eE. Builds the SECRET-FREE app and writes everything to flash (except
# nvs, which the laptop makes from secrets.py) into target/ota-out/:
#   bootloader.bin (0x1000)  partition-table.bin (0x8000)  garagebot-app.bin (0x20000, signed)
#   garagebot-manifest.txt   (for a GitHub release; nothing is uploaded here)
# Signing key: ~/.garagebot-signing/signing_key.pem (outside git, chmod 600).
set -euo pipefail
. ~/export-esp.sh
source ~/.cargo/env
cd "$(dirname "$0")"
# ESP-IDF needs a python3 that can make a venv (the system one lacks ensurepip): use uv's 3.11.
"$HOME/.local/bin/uv" python install 3.11 >/dev/null
export PATH="$(dirname "$("$HOME/.local/bin/uv" python find 3.11 --managed-python)"):$HOME/.cargo/bin:$PATH"
KEY="$HOME/.garagebot-signing/signing_key.pem"
[ -r "$KEY" ] || { echo "no signing key at $KEY" >&2; exit 1; }
# No GB_* secret may reach the build, even by accident.
for v in GB_WIFI_SSID GB_WIFI_PASSWORD GB_TELEGRAM_BOT_TOKEN GB_TELEGRAM_CHAT_ID GB_NOTIFY GB_LOG_ADDR; do unset "$v"; done

cargo build --release
REL=target/xtensa-esp32-espidf/release
IDF=.embuild/espressif/esp-idf/v5.3.3
PY=.embuild/espressif/python_env/idf5.3_py3.11_env/bin/python
OUT=target/ota-out
rm -rf "$OUT"; mkdir -p "$OUT"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
"$PY" -m esptool --chip esp32 elf2image --flash_mode dio --flash_freq 40m --flash_size 4MB \
    --secure-pad --elf-sha256-offset 0xb0 -o "$OUT/app-unsigned.bin" "$REL/garagebot" >/dev/null
"$PY" -m espsecure sign_data --version 1 --keyfile "$KEY" --output "$OUT/garagebot-app.bin" "$OUT/app-unsigned.bin" >/dev/null
"$PY" -m espsecure verify_signature --version 1 --keyfile "$HOME/.garagebot-signing/verification_key.bin" "$OUT/garagebot-app.bin" >/dev/null
rm "$OUT/app-unsigned.bin"
cp "$REL/bootloader.bin" "$OUT/bootloader.bin"
"$PY" "$IDF/components/partition_table/gen_esp32part.py" --flash-size 4MB partitions.csv "$OUT/partition-table.bin" >/dev/null

SIZE=$(stat -c %s "$OUT/garagebot-app.bin")
BL=$(stat -c %s "$OUT/bootloader.bin")
[ "$SIZE" -le $((0x1E0000)) ] || { echo "app $SIZE bytes does not fit a 0x1E0000 slot" >&2; exit 1; }
[ "$BL" -le $((0x7000)) ] || { echo "bootloader $BL bytes overlaps the partition table" >&2; exit 1; }
SHA=$(sha256sum "$OUT/garagebot-app.bin" | cut -d' ' -f1)
ROOT=$(sed -n 's/^root \([0-9a-f]*\).*/\1/p' ../ROOT.txt | head -1)
cat > "$OUT/garagebot-manifest.txt" <<MAN
version=$VERSION
size=$SIZE
sha256=$SHA
proof_root=$ROOT
MAN
echo "app $VERSION: $SIZE bytes (slot 1966080), bootloader $BL bytes, signed ECDSA-P256"
echo "out: $(pwd)/$OUT"
