import machine
from machine import Pin, WDT
import network
import time
import urequests
import json
import gc
import sys
import socket
import os
from secrets import SSID, PASSWORD, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID, MQTT_BROKER, MQTT_PORT, MQTT_CLIENT_ID, MQTT_LOG_TOPIC, MQTT_CRASH_TOPIC
from umqtt.simple import MQTTClient

# ============ CONFIGURATION ============
SENSOR_PIN = 32
RELAY_PIN = 14
INITIAL_ALERT_MINUTES = 15
REPEAT_ALERT_MINUTES = 5
POLL_INTERVAL_SECONDS = 5
LAST_UPDATE_ID = 0

# OTA Configuration
GITHUB_USER = "doomdagadiggiedahdah"
GITHUB_REPO = "garage-bot"
GITHUB_BRANCH = "main"
CURRENT_VERSION = "1.0.2"  # Bump this when you release new versions
CHECK_UPDATE_ON_BOOT = True  # Auto-check for updates on startup

# Heartbeat interval (prints status even when idle)
HEARTBEAT_INTERVAL_SECONDS = 60  # Print status every minute

# Enable hardware watchdog (resets ESP32 if code hangs)
ENABLE_WATCHDOG = True
WATCHDOG_TIMEOUT_MS = 300000  # 5 minutes

# ============ GLOBALS ============
mqtt_client = None
wdt = None
boot_time = None
loop_count = 0
bot_triggered_close = False

# ============ LOGGING ============
def log(message, level="INFO"):
    """Unified logging with timestamp and memory info"""
    free_mem = gc.mem_free()
    timestamp = time.ticks_ms() // 1000
    uptime = timestamp - (boot_time or timestamp)
    log_line = f"[{uptime:>6}s] [{level:>5}] [mem:{free_mem:>6}] {message}"
    print(log_line)
    
    # Also try MQTT for remote logging
    if level in ["ERROR", "WARN"] and mqtt_client:
        try:
            mqtt_client.publish(MQTT_LOG_TOPIC, log_line)
        except:
            pass

def log_exception(e, context=""):
    """Log exception with full traceback"""
    import io
    buf = io.StringIO()
    sys.print_exception(e, buf)
    trace = buf.getvalue()
    log(f"EXCEPTION in {context}:\n{trace}", "ERROR")

# ============ MQTT ============
def connect_mqtt():
    global mqtt_client
    try:
        if wdt:
            wdt.feed()
        mqtt_client = MQTTClient(MQTT_CLIENT_ID, MQTT_BROKER, MQTT_PORT)
        mqtt_client.connect()
        log("MQTT connected")
        return True
    except Exception as e:
        log_exception(e, "connect_mqtt")
        mqtt_client = None
        return False

def ensure_mqtt():
    global mqtt_client
    if mqtt_client is None:
        connect_mqtt()

# ============ SETUP ============
sensor = Pin(SENSOR_PIN, Pin.IN, Pin.PULL_UP)
relay = Pin(RELAY_PIN, Pin.OUT)
relay.value(0)

# ============ WIFI ============
def connect_wifi():
    wlan = network.WLAN(network.STA_IF)
    log(wlan.active(True))
    
    attempt = 0
    while True:  # INFINITE RETRY
        attempt += 1
        
        # Check if already connected
        if wlan.isconnected():
            log(f"WiFi connected! IP: {wlan.ifconfig()[0]}")
            return True
        
        log(f"WiFi attempt #{attempt}...")
        
        try:
            wlan.config(txpower=80)
            wlan.connect(SSID, PASSWORD)
        except Exception as e:
            log(f"Connect failed: {e}", "WARN")
            time.sleep(10)
            if wdt:
                wdt.feed()
            continue
        
        # Wait up to 30 seconds for this attempt
        for i in range(30):
            if wdt:
                wdt.feed()
            
            if wlan.isconnected():
                log(f"WiFi connected on attempt #{attempt}! IP: {wlan.ifconfig()[0]}")
                return True
            
            time.sleep(1)
        
        # Failed, wait before retry
        log(f"Attempt #{attempt} failed, retrying in 1s...", "WARN")
        time.sleep(1)
        if wdt:
            wdt.feed()

def ensure_wifi():
    wlan = network.WLAN(network.STA_IF)
    if not wlan.isconnected():
        log("WiFi disconnected, reconnecting...", "WARN")
        return connect_wifi()
    return True

# ============ OTA UPDATE FUNCTIONS ============
def check_for_update():
    """Check GitHub for a newer version"""
    try:
        if wdt:
            wdt.feed()
        
        log("Checking for updates...")
        
        # Fetch version file from GitHub
        url = f"https://raw.githubusercontent.com/{GITHUB_USER}/{GITHUB_REPO}/{GITHUB_BRANCH}/version.txt"
        response = urequests.get(url, timeout=10)  # 15 second timeout
        
        if response.status_code == 200:
            remote_version = response.text.strip()
            response.close()
            gc.collect()
            
            log(f"Version check: Current={CURRENT_VERSION}, Remote={remote_version}")
            
            if remote_version != CURRENT_VERSION:
                return remote_version
            else:
                log("Already on latest version")
        else:
            log(f"Version check failed: HTTP {response.status_code}", "WARN")
            response.close()
        
        return None
        
    except OSError as e:
        log(f"Version check timeout/network error: {e}", "WARN")
        gc.collect()
        return None
    except Exception as e:
        log_exception(e, "check_for_update")
        return None

def do_ota_update():
    """Download new main.py from GitHub and reboot"""
    try:
        # Feed watchdog before starting
        if wdt:
            wdt.feed()
        
        log("Starting OTA update from GitHub...")
        send_telegram_message("Downloading update from GitHub...")
        
        url = f"https://raw.githubusercontent.com/{GITHUB_USER}/{GITHUB_REPO}/{GITHUB_BRANCH}/main.py"
        
        # Longer timeout for downloading full file
        response = urequests.get(url, timeout=10)
        
        # Feed watchdog during download
        if wdt:
            wdt.feed()
        
        if response.status_code == 200:
            # Get new code
            new_code = response.text
            response.close()
            
            # Feed watchdog again
            if wdt:
                wdt.feed()
            
            gc.collect()
            
            log(f"Downloaded {len(new_code)} bytes, writing to main.py...")
            
            # Write new code
            with open("main.py", "w") as f:
                f.write(new_code)
            
            log("Update written successfully!")
            send_telegram_message("Update installed! Rebooting in 3 seconds...")
            time.sleep(3)
            machine.reset()
        else:
            log(f"Download failed: HTTP {response.status_code}", "ERROR")
            response.close()
            send_telegram_message(f"Update failed: HTTP {response.status_code}")
            return False
            
    except OSError as e:
        log(f"OTA download timeout/network error: {e}", "ERROR")
        send_telegram_message(f"Update failed: Network error")
        gc.collect()
        return False
    except Exception as e:
        log_exception(e, "do_ota_update")
        send_telegram_message(f"Update failed: {str(e)[:100]}")
        return False

# ============ TELEGRAM FUNCTIONS ============
def send_telegram_message(message):
    try:
        if wdt:
            wdt.feed()  # Reset watchdog right before potentially slow operation
        
        url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage"
        data = {"chat_id": TELEGRAM_CHAT_ID, "text": message}
        response = urequests.post(url, json=data, timeout=3)  # 3 second timeout
        status = response.status_code
        response.close()
        
        # CRITICAL: Force garbage collection after HTTP request
        gc.collect()
        
        log(f"Telegram send: {status}")
        return status == 200
    except OSError as e:
        log(f"Telegram timeout/network error: {e}", "WARN")
        gc.collect()
        return False
    except Exception as e:
        log_exception(e, "send_telegram_message")
        gc.collect()
        return False


def get_telegram_updates():
    global LAST_UPDATE_ID
    try:
        if wdt:
            wdt.feed()  # Reset watchdog right before potentially slow operation
        
        url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/getUpdates?offset={LAST_UPDATE_ID + 1}&timeout=2"
        response = urequests.get(url, timeout=10)  # 10 second timeout
        data = response.json()
        response.close()
        
        # CRITICAL: Force garbage collection after HTTP request
        gc.collect()
        
        if data.get("ok") and data.get("result"):
            return data["result"]
        return []
    except OSError as e:
        log(f"Telegram poll timeout: {e}", "WARN")
        gc.collect()
        return []
    except Exception as e:
        log_exception(e, "get_telegram_updates")
        gc.collect()
        return []


# ============ DOOR FUNCTIONS ============
def is_door_open():
    return sensor.value() == 1

def get_door_status_text():
    return "Door is OPEN" if is_door_open() else "Door is CLOSED"

def press_garage_button():
    log("Pressing garage button")
    relay.value(1)
    time.sleep(0.5)
    relay.value(0)
    log("Button press complete")

# ============ COMMAND HANDLERS ============
def handle_command(message_text):
    global bot_triggered_close
    cmd = message_text.lower().strip()
    if "@" in cmd:
        cmd = cmd.split("@")[0]
    
    log(f"Command received: {cmd}")
    
    if cmd in ["help", "/help", "?"]:
        return """Garage Door Bot Commands:

status - Check if door is open or closed
open - Open the door (if closed)
close - Close the door (if open)
press - Press the button (toggle door)
silence - Mute alerts until door closes
version - Show current firmware version
update - Check for and install updates
debug - Show system info
help - Show this message"""
    
    elif cmd in ["status", "/status"]:
        return get_door_status_text()
    
    elif cmd in ["open", "/open"]:
        if is_door_open():
            return "Door is already open!"
        else:
            press_garage_button()
            return "Opening door..."
    
    elif cmd in ["close", "/close"]:
        if not is_door_open():
            return "Door is already closed!"
        else:
            bot_triggered_close = True
            press_garage_button()
            return "Closing door..."

    elif cmd in ["press", "/press", "toggle", "/toggle"]:
        if is_door_open():
            bot_triggered_close = True
        press_garage_button()
        current = "open" if is_door_open() else "closed"
        return f"Button pressed! Door was {current}."
    
    elif cmd in ["silence", "/silence", "quiet", "stop", "mute"]:
        return "SILENCE"
    
    elif cmd in ["version", "/version", "ver"]:
        return f"Version: {CURRENT_VERSION}"
    
    elif cmd in ["update", "/update"]:
        new_version = check_for_update()
        if new_version:
            send_telegram_message(f"Update available: {new_version} (current: {CURRENT_VERSION})")
            send_telegram_message("Installing update now...")
            do_ota_update()
            return None  # Will reboot, so no response
        else:
            return f"Already on latest version ({CURRENT_VERSION})"
    
    # Debug command to check system health remotely
    elif cmd in ["debug", "/debug", "info", "/info"]:
        uptime = (time.ticks_ms() // 1000) - boot_time
        return f"""System Info:
Version: {CURRENT_VERSION}
Uptime: {uptime // 60}m {uptime % 60}s
Free memory: {gc.mem_free()} bytes
Loop count: {loop_count}
Door: {'OPEN' if is_door_open() else 'CLOSED'}
MQTT: {'connected' if mqtt_client else 'disconnected'}"""
    
    else:
        return None

# ============ MAIN LOOP ============
def main():
    global LAST_UPDATE_ID, wdt, boot_time, loop_count, bot_triggered_close
    
    boot_time = time.ticks_ms() // 1000
    
    log("="*40)
    log("Garage door monitor starting")
    log(f"Version: {CURRENT_VERSION}")
    log(f"Free memory at boot: {gc.mem_free()}")
    log("="*40)
    
    # Initialize watchdog timer
    if ENABLE_WATCHDOG:
        log(f"Enabling watchdog timer ({WATCHDOG_TIMEOUT_MS}ms)")
        wdt = WDT(timeout=WATCHDOG_TIMEOUT_MS)
    
    # Retry WiFi connection until successful
    connect_wifi()
    
    # Connect MQTT
    connect_mqtt()
    
    # Send startup message
    send_telegram_message(f"Garage door bot is online!\nVersion: {CURRENT_VERSION}\n\nType 'help' for commands.")
    
    # Check for updates on boot (optional)
    if CHECK_UPDATE_ON_BOOT:
        log("Checking for updates on boot...")
        new_version = check_for_update()
        if new_version:
            log(f"Update available: {new_version}")
            send_telegram_message(f"Update available: {new_version}\nSend 'update' to install, or it will auto-update on next restart.")
        else:
            log("No updates available")
    
    # Door monitoring state
    open_start_time = None
    last_alert_time = None
    notifications_muted = False
    bot_triggered_close = False
    
    # Timing
    last_poll_time = time.ticks_ms() / 1000
    last_heartbeat_time = time.ticks_ms() / 1000
    last_door_state = is_door_open()
    
    log("Entering main loop")
    
    while True:
        loop_count += 1
        current_time = time.ticks_ms() / 1000
        
        # CRITICAL: Feed the watchdog
        if wdt:
            wdt.feed()
        
        # Ensure WiFi is connected
        if not ensure_wifi():
            time.sleep(5)
            continue
        
        # Ensure MQTT is connected
        ensure_mqtt()
        
        # -------- Heartbeat --------
        if current_time - last_heartbeat_time >= HEARTBEAT_INTERVAL_SECONDS:
            last_heartbeat_time = current_time
            door_state = "OPEN" if is_door_open() else "CLOSED"
            uptime_min = int((current_time - boot_time) / 60)
            log(f"HEARTBEAT: door={door_state}, uptime={uptime_min}m, loops={loop_count}, mem={gc.mem_free()}")
            
            # Periodic garbage collection
            gc.collect()
        
        # -------- Poll Telegram for commands --------
        if current_time - last_poll_time >= POLL_INTERVAL_SECONDS:
            last_poll_time = current_time
            
            updates = get_telegram_updates()
            for update in updates:
                LAST_UPDATE_ID = update["update_id"]
                
                if "message" in update and "text" in update["message"]:
                    message_text = update["message"]["text"]
                    chat_id = update["message"]["chat"]["id"]
                    
                    if str(chat_id) == str(TELEGRAM_CHAT_ID):
                        response = handle_command(message_text)
                        
                        if response == "SILENCE":
                            notifications_muted = True
                            send_telegram_message("🔇 Alerts muted until door closes.")
                        elif response:
                            send_telegram_message(response)
                    else:
                        log(f"Unauthorized chat: {chat_id}", "WARN")
        
        # -------- Monitor door state --------
        door_open = is_door_open()
        
        # Log state changes
        if door_open != last_door_state:
            log(f"Door state changed: {'OPEN' if door_open else 'CLOSED'}")
            last_door_state = door_open
        
        if door_open:
            if open_start_time is None:
                open_start_time = current_time
                last_alert_time = None
                notifications_muted = False
                log("Door opened - starting timer")
            else:
                elapsed_minutes = (current_time - open_start_time) / 60
                
                should_alert = False
                if last_alert_time is None and elapsed_minutes >= INITIAL_ALERT_MINUTES:
                    should_alert = True
                elif last_alert_time is not None:
                    minutes_since_alert = (current_time - last_alert_time) / 60
                    if minutes_since_alert >= REPEAT_ALERT_MINUTES:
                        should_alert = True
                
                if should_alert and not notifications_muted:
                    message = f"ALERT: Garage door has been open for {int(elapsed_minutes)} minutes!"
                    log(f"Sending alert: {message}")
                    send_telegram_message(message)
                    last_alert_time = current_time
        
        else:
            if open_start_time is not None:
                elapsed = (current_time - open_start_time) / 60
                log(f"Door closed after {elapsed:.1f} minutes")

                if elapsed >= 1 and bot_triggered_close:
                    send_telegram_message(f"Door closed after {elapsed:.1f} minutes.")

            open_start_time = None
            last_alert_time = None
            notifications_muted = False
            bot_triggered_close = False
        
        time.sleep(0.5)

# ============ ENTRY POINT ============
if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        # Try to log the crash
        log_exception(e, "MAIN")
        
        # Try to send crash notification
        try:
            gc.collect()
            import io
            buf = io.StringIO()
            sys.print_exception(e, buf)
            crash_msg = f"CRASH: {buf.getvalue()}"
            
            # Try MQTT first (faster)
            if mqtt_client:
                mqtt_client.publish(MQTT_CRASH_TOPIC, crash_msg)
            
            # Then try Telegram
            send_telegram_message(f"CRASH:\n{buf.getvalue()[:500]}")
        except:
            pass
        
        # Print to serial for Pi logger
        print("="*40)
        print("FATAL CRASH - RESTARTING IN 5 SECONDS")
        print("="*40)
        sys.print_exception(e)
        
        time.sleep(5)
        machine.reset()
