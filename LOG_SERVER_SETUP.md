# Log Server Setup

This document describes how to set up the log-receiving server for the garage-bot ESP32. The ESP32 sends all log output as UDP packets to a server on the LAN. Any Linux machine on the same network can act as the log server.

<!-- IMPORTANT: The log server IP is hardcoded in main.py (UDP_LOG_HOST). If the server changes, update main.py and ARCHITECTURE.md to match. -->

## What the Log Server Does

**UDP Logger** (`udp_logger.py`) — Listens on UDP port 5005, timestamps each line, writes to `garage.log` (rotates at 10MB, keeps one backup).

## Quick Setup on a New Machine

### 1. Copy the logger script

```bash
mkdir -p ~/garage-bot
# Copy udp_logger.py from this repo to ~/garage-bot/
scp udp_logger.py <user>@<new-host>:~/garage-bot/
```

### 2. Create the systemd service

Create `/etc/systemd/system/garage-udp-logger.service`:

```ini
[Unit]
Description=Garage Bot UDP Log Listener
After=network.target

[Service]
ExecStart=/usr/bin/python3 /home/<user>/garage-bot/udp_logger.py
WorkingDirectory=/home/<user>/garage-bot
Restart=always
RestartSec=5
User=<user>

[Install]
WantedBy=multi-user.target
```

Then enable and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now garage-udp-logger
```

### 3. Verify it's listening

```bash
# From any machine on the LAN:
echo "test" | nc -u -w1 <new-host-ip> 5005

# Check on the server:
tail ~/garage-bot/garage.log
```

### 4. Point the ESP32 to the new server

Rust firmware: the address is the NVS key `log_addr` (set `LOG_ADDR = "host:port"` in
`secrets.py`, then rebuild the NVS image with `firmware-rs/shell/build_and_flash.sh`); without
it the default in `firmware-rs/shell/src/main.rs` is used.

Old MicroPython firmware (history, commit `2341d02`): the IP was hardcoded in `main.py` as
`UDP_LOG_HOST`, changed like this:

1. Edit `UDP_LOG_HOST` in `main.py` to the new server's LAN IP
2. Update `ARCHITECTURE.md` (Machines table + UDP section)
3. Update the Current Deployment table below
4. Bump the version (`CURRENT_VERSION` in `main.py` + `version.txt`) to trigger OTA

## Current Deployment

| Machine | Role | LAN IP | Tailscale IP |
|---------|------|--------|-------------|
| ESP32 | Garage door sensor + relay + Telegram bot | 192.168.50.104 | N/A |
| Jetson Orin Nano | UDP log receiver (active) | 192.168.50.180 | <jetson-tailscale-ip> |
| Pi-5 | Previous log receiver (can be decommissioned) | 192.168.50.90 | 100.72.95.67 |

## Log Format

Each line in `garage.log`:

```
2026-08-10T10:01:08 [540403s] [ INFO] [mem: 95952] HEARTBEAT: door=OPEN, uptime=9006m, ...
```

- ISO timestamp from the log server's clock
- `[Ns]` — ESP32 uptime in seconds
- `[LEVEL]` — INFO, WARN, ERROR
- `[mem:N]` — ESP32 free heap in bytes
- Message body

### HEAP telemetry line (machine-parseable)

```
HEAP uptime_min,mem_free,loop_count,http_ok,http_fail,sock_err
```

Sent every 60 seconds alongside the human-readable heartbeat.

## Monitoring

```bash
# Watch live logs (via Tailscale)
ssh mat@<jetson-tailscale-ip> "tail -f ~/garage-bot/garage.log"

# Check service status
ssh mat@<jetson-tailscale-ip> "systemctl status garage-udp-logger"
```
