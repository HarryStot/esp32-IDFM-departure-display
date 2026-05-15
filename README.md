# esp32-IDFM-departure-display

An ESP32 firmware for a novelty bus departure timer housed in a vintage manometer. Connects to the IDFM stop-monitoring API and drives a servo motor to indicate how many minutes until the next bus arrives.

## Project vision

A repurposed analog gauge that displays real-time transport data instead of pressure readings—the servo needle points to minutes remaining on the dial face (0–60 min scale).

## Current features

- connects an ESP32 to Wi-Fi
- queries the IDFM stop-monitoring endpoint over HTTPS
- extracts next departure timestamps from JSON responses
- calculates minutes until departure
- logs timing data for servo calibration

## Planned features

- servo motor control to position gauge needle
- deep sleep mode to reduce power consumption between API polls

## Configuration

Copy [.env.example](.env.example) to [.env](.env) and fill in your local values:

- `WIFI_SSID` – your Wi-Fi network name
- `WIFI_PASSWORD` – your Wi-Fi password
- `IDFM_API_KEY` – your IDFM API key (get one at [IDFM Marketplace](https://marketplace.iledefrance-mobilites.fr/))
- `MONITORING_REF_ENCODED` – the URL-encoded stop point reference (e.g., `STIF%3AStopPoint%3AQ%3A12345%3A`)

The build script reads [.env](.env) locally and injects those values at compile time.

## Build & flash

```bash
cargo build --release
cargo espflash flash --release
```

OR

```bash
cargo run --release
```
