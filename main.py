import machine
from machine import Pin
import network
import time
import urequests
import json
from secrets import SSID, PASSWORD, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID

# ============ CONFIGURATION ============
SENSOR_PIN = 32      # GPIO pin for door sensor (with internal pull-up)
RELAY_PIN = 27       # GPIO pin for relay control

# Alert timing
INITIAL_ALERT_MINUTES = 15   # First alert after door open for X minutes
REPEAT_ALERT_MINUTES = 5     # Then repeat alert every X minutes

# Telegram polling
POLL_INTERVAL_SECONDS = 2    # How often to check for new commands
LAST_UPDATE_ID = 0           # Track which messages we've already processed

# ============ SETUP ============
sensor = Pin(SENSOR_PIN, Pin.IN, Pin.PULL_UP)
relay = Pin(RELAY_PIN, Pin.OUT)
relay.value(0)  # Make sure relay starts off

# ============ WIFI ============
def connect_wifi():
    wlan = network.WLAN(network.STA_IF)
    wlan.active(True)
    
    if not wlan.isconnected():
        print("Connecting to WiFi...")
        wlan.connect(SSID, PASSWORD)
        
        for i in range(30):
            if wlan.isconnected():
                print("Connected!")
                print("IP:", wlan.ifconfig()[0])
                break
            time.sleep(1)
        
        if not wlan.isconnected():
            print("Failed to connect to WiFi")
            return False
    
    return True

def ensure_wifi():
    """Reconnect WiFi if disconnected"""
    wlan = network.WLAN(network.STA_IF)
    if not wlan.isconnected():
        print("WiFi disconnected, reconnecting...")
        return connect_wifi()
    return True

# ============ TELEGRAM FUNCTIONS ============
def send_telegram_message(message):
    try:
        url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage"
        data = {
            "chat_id": TELEGRAM_CHAT_ID,
            "text": message
        }
        response = urequests.post(url, json=data)
        status = response.status_code
        response.close()
        return status == 200
    except Exception as e:
        print(f"Error sending Telegram message: {e}")
        return False

def get_telegram_updates():
    """Fetch new messages from Telegram"""
    global LAST_UPDATE_ID
    try:
        # Use offset to only get new messages we haven't seen
        url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/getUpdates?offset={LAST_UPDATE_ID + 1}&timeout=1"
        response = urequests.get(url)
        data = response.json()
        response.close()
        
        if data.get("ok") and data.get("result"):
            return data["result"]
        return []
    except Exception as e:
        print(f"Error getting Telegram updates: {e}")
        return []

# ============ DOOR FUNCTIONS ============
def is_door_open():
    """Check door sensor state"""
    # Pull-up: 1 = open (switch open), 0 = closed (switch closed to GND)
    return sensor.value() == 1

def get_door_status_text():
    """Return human-readable door status"""
    if is_door_open():
        return "Door is OPEN"
    else:
        return "Door is CLOSED"

def press_garage_button():
    """Simulate pressing the garage door button"""
    print("Pressing garage button...")
    relay.value(1)
    time.sleep(0.5)
    relay.value(0)
    print("Button press complete")

# ============ COMMAND HANDLERS ============
def handle_command(message_text):
    """Process incoming commands and return response"""
    cmd = message_text.lower().strip()
    
    if cmd in ["help", "/help", "?"]:
        return """Garage Door Bot Commands:

status - Check if door is open or closed
open - Open the door (if closed)
close - Close the door (if open)
press - Press the button (toggle door)
silence - Mute alerts until door closes
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
            press_garage_button()
            return "Closing door..."
    
    elif cmd in ["press", "/press", "toggle", "/toggle"]:
        press_garage_button()
        current = "open" if is_door_open() else "closed"
        return f"Button pressed! Door was {current}."
    
    elif cmd in ["silence", "/silence", "quiet", "stop", "mute"]:
        return "SILENCE"  # Special return value handled in main loop
    
    else:
        return None  # Unknown command, ignore

# ============ MAIN LOOP ============
def main():
    global LAST_UPDATE_ID
    
    print("Starting garage door monitor...")
    
    if not connect_wifi():
        print("Cannot proceed without WiFi")
        return
    
    # Send startup message
    send_telegram_message("Garage door bot is online!\n\nType 'help' for commands.")
    
    # Door monitoring state
    open_start_time = None
    last_alert_time = None
    notifications_muted = False
    
    # Timing
    last_poll_time = time.ticks_ms() / 1000  # Initialize to current time
    
    print("Entering main loop...")
    
    while True:
        current_time = time.ticks_ms() / 1000  # Seconds
        
        # Ensure WiFi is connected
        if not ensure_wifi():
            time.sleep(5)
            continue
        
        # -------- Poll Telegram for commands --------
        if current_time - last_poll_time >= POLL_INTERVAL_SECONDS:
            last_poll_time = current_time
            
            updates = get_telegram_updates()
            for update in updates:
                # Update the offset so we don't process this message again
                LAST_UPDATE_ID = update["update_id"]
                
                if "message" in update and "text" in update["message"]:
                    message_text = update["message"]["text"]
                    chat_id = update["message"]["chat"]["id"]
                    
                    # Only respond to messages from our authorized chat
                    if str(chat_id) == str(TELEGRAM_CHAT_ID):
                        response = handle_command(message_text)
                        
                        if response == "SILENCE":
                            notifications_muted = True
                            send_telegram_message("Alerts muted until door closes.")
                        elif response:
                            send_telegram_message(response)
                    else:
                        print(f"Ignored message from unauthorized chat: {chat_id}")
        
        # -------- Monitor door state --------
        door_open = is_door_open()
        
        if door_open:
            if open_start_time is None:
                # Door just opened
                open_start_time = current_time
                last_alert_time = None
                notifications_muted = False
                print("Door opened")
            else:
                # Door has been open, check if we need to alert
                elapsed_minutes = (current_time - open_start_time) / 60
                
                should_alert = False
                
                # First alert after INITIAL_ALERT_MINUTES
                if last_alert_time is None and elapsed_minutes >= INITIAL_ALERT_MINUTES:
                    should_alert = True
                # Repeat alerts every REPEAT_ALERT_MINUTES
                elif last_alert_time is not None:
                    minutes_since_alert = (current_time - last_alert_time) / 60
                    if minutes_since_alert >= REPEAT_ALERT_MINUTES:
                        should_alert = True
                
                if should_alert and not notifications_muted:
                    message = f"ALERT: Garage door has been open for {int(elapsed_minutes)} minutes!"
                    print(f"Sending alert: {message}")
                    send_telegram_message(message)
                    last_alert_time = current_time
        
        else:
            # Door is closed
            if open_start_time is not None:
                elapsed = (current_time - open_start_time) / 60
                print(f"Door closed after {elapsed:.1f} minutes")
                
                # Notify that door closed (if it was open long enough to matter)
                if elapsed >= 1:
                    send_telegram_message(f"Door closed after {elapsed:.1f} minutes.")
            
            # Reset state
            open_start_time = None
            last_alert_time = None
            notifications_muted = False
        
        time.sleep(0.5)  # Small delay to prevent tight loop

if __name__ == "__main__":
    main()
