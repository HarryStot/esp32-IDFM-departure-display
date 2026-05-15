# esp32_time_bus

An ESP32 firmware project that connects to Wi-Fi, calls the Île-de-France Mobilités stop-monitoring API, and parses the next departures from the JSON response.

## What it does

- connects an ESP32 to a Wi-Fi network
- queries the IDFM stop-monitoring endpoint over HTTPS
- extracts departure timestamps from the response
- logs the current time and upcoming departures

## Configuration

Copy [.env.example](.env.example) to [.env](.env) and fill in your local values:

- `WIFI_SSID`
- `WIFI_PASSWORD`
- `IDFM_API_KEY`

The build script reads [.env](.env) locally and injects those values at compile time.

## Security note

Do not commit Wi-Fi credentials or API keys. If any secret was already committed, rotate it and rewrite the git history before publishing.

## Build

Use the normal Cargo build flow for the ESP32 target configured by this project.
