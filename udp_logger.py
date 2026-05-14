#!/usr/bin/env python3
"""UDP log listener for ESP32 garage-bot.

Run on the Pi:
    python3 udp_logger.py

Logs are written to stdout (with timestamps) and appended to garage.log.
Log files rotate at 10MB to avoid filling the SD card.

To run as a service, see the systemd unit at the bottom of this file.
"""

import socket
import sys
from datetime import datetime
from pathlib import Path

LISTEN_PORT = 5005
LOG_DIR = Path(__file__).parent
LOG_FILE = LOG_DIR / "garage.log"
MAX_LOG_SIZE = 10 * 1024 * 1024  # 10 MB

def rotate_if_needed():
    if LOG_FILE.exists() and LOG_FILE.stat().st_size > MAX_LOG_SIZE:
        # Keep one rotated backup
        backup = LOG_FILE.with_suffix(".log.1")
        if backup.exists():
            backup.unlink()
        LOG_FILE.rename(backup)

def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("0.0.0.0", LISTEN_PORT))
    print(f"Listening for UDP logs on port {LISTEN_PORT}...")

    while True:
        data, addr = sock.recvfrom(4096)
        timestamp = datetime.now().isoformat(timespec="seconds")
        line = f"{timestamp} {data.decode(errors='replace').rstrip()}"

        # Print to stdout
        print(line, flush=True)

        # Append to log file
        rotate_if_needed()
        with open(LOG_FILE, "a") as f:
            f.write(line + "\n")

if __name__ == "__main__":
    main()

# --- systemd unit (save as /etc/systemd/system/garage-logger.service) ---
#
# [Unit]
# Description=Garage Bot UDP Log Listener
# After=network.target
#
# [Service]
# ExecStart=/usr/bin/python3 /home/pi/garage-bot/udp_logger.py
# WorkingDirectory=/home/pi/garage-bot
# Restart=always
# User=pi
#
# [Install]
# WantedBy=multi-user.target
#
# Then: sudo systemctl enable --now garage-logger
