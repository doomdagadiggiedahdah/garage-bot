import machine
import network
import time
import urequests
import json
from secrets import SSID, PASSWORD, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID

# ============ CONFIGURATION ============
SENSOR_PIN = 34  # GPIO pin for door sensor

# Alert timing (can be adjusted)
INITIAL_ALERT_MINUTES = .05   # First alert after door open for X minutes
REPEAT_ALERT_MINUTES = 5    # Then repeat alert every X minutes

# ============ SETUP ============
sensor = machine.ADC(machine.Pin(SENSOR_PIN))
sensor.atten(machine.ADC.ATTN_11DB)

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

# ============ TELEGRAM MESSAGE ============
def send_telegram_message(message):
    try:
        url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/sendMessage"
        data = {
            "chat_id": TELEGRAM_CHAT_ID,
            "text": message
        }
        print(f"Debug: Sending data: {data}")
        response = urequests.post(url, json=data)
        status = response.status_code
        print(f"Telegram sent! Status: {status}")
        if status != 200:
            print(f"Response: {response.text}")
        response.close()
        return True
    except Exception as e:
        print(f"Error sending Telegram message: {e}")
        return False

# ============ DOOR MONITORING ============
def is_door_open():
    # Sensor reads LOW when open (magnet away), HIGH when closed (magnet near)
    reading = sensor.read()
    return reading < 1000  # Adjust threshold if needed

def check_telegram_commands():
    """Check for incoming Telegram messages and return True if notifications should be disabled"""
    try:
        url = f"https://api.telegram.org/bot{TELEGRAM_BOT_TOKEN}/getUpdates"
        response = urequests.get(url)
        data = response.json()
        response.close()
        
        if data.get("ok") and data.get("result"):
            # Get the most recent message
            latest_update = data["result"][-1]
            if "message" in latest_update:
                message_text = latest_update["message"].get("text", "").lower()
                chat_id = latest_update["message"]["chat"]["id"]
                
                # Check for disable command
                if "silence" in message_text or "quiet" in message_text or "stop" in message_text:
                    send_telegram_message("🤐 Notifications muted until door closes.")
                    return True
        
        return False
    except Exception as e:
        print(f"Error checking Telegram commands: {e}")
        return False

def main():
    print("Starting garage door monitor...")
    
    if not connect_wifi():
        print("Cannot proceed without WiFi")
        return
    
    open_start_time = None
    last_alert_time = None
    notifications_disabled = False
    
    while True:
        door_open = is_door_open()
        # Use time.ticks_ms() for relative timing since RTC may not be accurate
        current_time = time.ticks_ms() / 1000  # Convert to seconds for compatibility
        
        # Check for incoming commands
        if notifications_disabled:
            notifications_disabled = check_telegram_commands()
        
        if door_open:
            if open_start_time is None:
                open_start_time = current_time
                last_alert_time = None
                notifications_disabled = False
                print("Door opened")
            else:
                elapsed_minutes = (current_time - open_start_time) / 60
                
                # Send alert if:
                # 1. First alert: door has been open for 15+ minutes and no alert sent yet
                # 2. Repeat alerts: last alert was 5+ minutes ago
                should_send_alert = False
                
                if last_alert_time is None and elapsed_minutes >= INITIAL_ALERT_MINUTES:
                    should_send_alert = True
                elif last_alert_time is not None:
                    minutes_since_last_alert = (current_time - last_alert_time) / 60
                    if minutes_since_last_alert >= REPEAT_ALERT_MINUTES:
                        should_send_alert = True
                
                if should_send_alert and not notifications_disabled:
                    message = f"ALERT: Garage door has been open for {int(elapsed_minutes)} minutes!"
                    print(f"Sending alert: {message}")
                    send_telegram_message(message)
                    last_alert_time = current_time
        else:
            if open_start_time is not None:
                elapsed = (current_time - open_start_time) / 60
                print(f"Door closed after {elapsed:.1f} minutes")
            
            open_start_time = None
            last_alert_time = None
            notifications_disabled = False
        
        time.sleep(10)  # Check every 10 seconds

if __name__ == "__main__":
    main()
