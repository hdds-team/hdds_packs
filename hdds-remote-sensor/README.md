# HDDS Remote Sensor Demo

Real-time camera + temperature sensor over 433MHz radio using HDDS DDS protocol.

**Demonstrates**: HDDS-Micro SDK (C) + Full HDDS (Rust) + TCP/QUIC WAN transport

```
  ESP32-WROVER-CAM                    Pi Zero 2 W                      Server (hdds.io)
  ┌─────────────────┐                ┌─────────────────┐              ┌─────────────────┐
  │  OV2640 Camera  │                │   pi-gateway    │              │   web-viewer    │
  │       ↓         │   HC-12        │                 │   HDDS       │                 │
  │  HDDS-Micro SDK │  433 MHz       │  Reassemble     │   TCP/QUIC   │  DDS Subscribe  │
  │  (C library)    │ ══════════════►│  fragments      │══════════════│       ↓         │
  │       ↓         │   9600 baud    │       ↓         │    WAN       │   Web UI        │
  │  Fragmentation  │                │  HDDS publish   │              │   :8080         │
  │  40 bytes/pkt   │                │  String+Vec<u8> │              │                 │
  └─────────────────┘                └─────────────────┘              └─────────────────┘
       SENSOR NODE                       PI GATEWAY                      WEB VIEWER
```

## Features

- **Long-range wireless**: HC-12 433 MHz (1km+ range, penetrates walls)
- **Camera streaming**: OV2640 JPEG over constrained 9600 baud link
- **Image fragmentation**: 40-byte packets with automatic reassembly
- **Real DDS/RTPS**: Full interoperability with DDS ecosystem
- **WAN support**: TCP/QUIC transport for Internet connectivity
- **Web dashboard**: Real-time temperature + camera feed
- **Cross-platform**: ESP32 (C SDK), Pi (Rust), Server (Rust)

## Hardware

### Bill of Materials

| Item | Qty | Price |
|------|-----|-------|
| ESP32-WROVER-CAM | 1 | ~10 EUR |
| Pi Zero 2 W | 1 | ~15 EUR |
| HC-12 433MHz module | 2 | ~6 EUR |
| 5V power supply | 1 | ~5 EUR |
| **Total** | | **~36 EUR** |

### Wiring

```
ESP32-WROVER-CAM          HC-12 (#1)
─────────────────         ──────────
GPIO32 (TX)      ───────► RX
GPIO33 (RX)      ◄─────── TX
GND              ───────► GND
                 5V ─────► VCC (EXTERNAL 5V!)

Pi Zero 2 W               HC-12 (#2)
───────────               ──────────
GPIO14 (TX)      ───────► RX
GPIO15 (RX)      ◄─────── TX
GND              ───────► GND
5V pin           ───────► VCC
```

> **Important**: HC-12 modules require **5V power** for reliable transmission.
> 3.3V results in very short range or no transmission.

## Quick Start

### 1. Build HDDS-Micro C SDK for ESP32

```bash
cd /path/to/hdds

# Cross-compile for ESP32 (requires esp-rs toolchain)
cargo +esp build --release -p hdds-micro-c \
    --target xtensa-esp32-espidf \
    -Zbuild-std=std,panic_abort

# Copy library to Arduino project
cp target/xtensa-esp32-espidf/release/libhdds_micro_c.a \
   /path/to/demos/hdds-remote-sensor/sensor-node-arduino/lib/hdds-micro/
```

### 2. Build & Flash ESP32

```bash
cd hdds-remote-sensor

# Build with PlatformIO
./scripts/build-esp32.sh

# Flash and monitor
./scripts/build-esp32.sh flash
```

### 3. Deploy Pi Gateway

```bash
# Cross-compile for ARM64
cd pi-gateway
cargo build --release --target aarch64-unknown-linux-gnu

# Deploy to Pi
cd ../scripts
./deploy-pi.sh pi-gateway.local
```

### 4. Deploy Web Viewer

```bash
# Build for server
cd web-viewer
cargo build --release

# Deploy
cd ../scripts
./deploy-server.sh hdds.io
```

### 5. Configure Network (WAN)

For Pi ↔ Server communication across the Internet:

```bash
# On Pi Gateway - point to server
sudo systemctl edit hdds-gateway
# Add: Environment=HDDS_PEER=server-ip:7411

# On Server - point to Pi (or use discovery)
sudo systemctl edit hdds-viewer
# Add: Environment=HDDS_PEER=pi-public-ip:7411

# Restart services
sudo systemctl restart hdds-gateway hdds-viewer
```

### 6. Open Dashboard

Navigate to: **http://server-ip:8080**

## Project Structure

```
hdds-remote-sensor/
├── sensor-node-arduino/     # ESP32 PlatformIO project
│   ├── src/main.cpp         # Camera capture + HDDS-Micro publish
│   ├── lib/hdds-micro/      # HDDS-Micro C SDK (.a + .h)
│   └── platformio.ini
│
├── pi-gateway/              # Raspberry Pi (Rust)
│   └── src/
│       ├── main.rs          # Serial RX + image reassembly + HDDS publish
│       └── protocol.rs      # HC-12 frame parsing
│
├── web-viewer/              # Server/Cloud (Rust)
│   └── src/main.rs          # HDDS subscribe + axum web server
│
├── web/                     # Static web assets
│   └── index.html           # Dashboard with gauges + camera feed
│
└── scripts/                 # Deployment automation
    ├── build-esp32.sh       # PlatformIO build/flash
    ├── deploy-pi.sh         # Deploy to Raspberry Pi
    └── deploy-server.sh     # Deploy to server
```

## DDS Configuration

### Topics

| Topic | Type | Direction |
|-------|------|-----------|
| `hdds_demo/remote_sensor/temperature` | Temperature | ESP32 → Pi → Server |
| `hdds_demo/remote_sensor/image` | ImageMeta | ESP32 → Pi → Server |

### Types

```rust
#[derive(DDS)]
struct Temperature {
    sensor_id: u32,
    value: f32,      // Celsius
    timestamp: u64,  // ms since boot
}

#[derive(DDS)]
struct ImageMeta {
    image_id: u32,
    width: u16,      // 160
    height: u16,     // 120
    format: String,  // "jpeg"
    data: Vec<u8>,   // Raw JPEG bytes
}
```

### QoS

Default configuration uses:
- **Reliability**: BEST_EFFORT (for real-time streaming)
- **History**: KEEP_LAST(1)
- **Transport**: UDP multicast (LAN) or TCP/QUIC (WAN)

### Environment Variables

Configure transport via environment variables (or in systemd service file):

| Variable | Description | Example |
|----------|-------------|---------|
| `HDDS_PEER` | Static peer for unicast discovery | `192.168.1.100:7411` |
| `HDDS_TCP_PORT` | Enable TCP transport on port | `7410` |
| `HDDS_QUIC` | Enable QUIC transport (NAT traversal) | `1` |
| `HDDS_LOWBW_PRESET` | LowBw transport preset | `hc12`, `lora`, `satellite` |

**TCP** is recommended for WAN connections (reliable, firewall-friendly).

**QUIC** adds NAT traversal and better performance over lossy networks.

**LowBw** presets optimize for constrained links:
- `hc12` - 9600 baud serial radio
- `lora` - 300-38400 bps LoRa
- `satellite` - High latency, low bandwidth

## Protocol Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                         Application                              │
├─────────────────────────────────────────────────────────────────┤
│              DDS API (HDDS-Micro / Full HDDS)                    │
├─────────────────────────────────────────────────────────────────┤
│                      RTPS v2.5 Protocol                          │
├────────────────────────────┬────────────────────────────────────┤
│   HC-12 Framing            │        UDP / TCP / QUIC             │
│   [AA][55][LEN_HI][LEN_LO] │                                      │
│   [RTPS...]                │                                      │
├────────────────────────────┼────────────────────────────────────┤
│   UART @ 9600 baud         │       IP Network                    │
├────────────────────────────┼────────────────────────────────────┤
│   433 MHz Radio            │       Ethernet / WiFi / 4G          │
└────────────────────────────┴────────────────────────────────────┘
      ESP32 ↔ Pi                        Pi ↔ Server
```

## HC-12 Configuration

Both modules must be configured identically (use AT commands):

```
AT+B9600    # Baud rate 9600
AT+C001     # Channel 001 (433.4 MHz)
AT+P8       # Max power (100mW)
AT+FU3      # Mode FU3 (balance range/speed)
```

## Troubleshooting

### No data from ESP32
- Check HC-12 is powered at **5V** (not 3.3V!)
- Verify UART wiring (TX↔RX crossover)
- Monitor ESP32 serial: `pio device monitor`

### Images not appearing
- Reduce JPEG quality in ESP32 (`jpeg_quality = 15`)
- Increase fragment delay (`delay(100)`)
- Check logs: `journalctl -u hdds-gateway -f`

### DDS discovery issues
- Ensure same domain ID on all nodes (default: 0)
- Check firewall allows UDP 7400-7420
- For WAN: use `HDDS_PEER` environment variable

### Pi UART not working
Add to `/boot/config.txt`:
```
enable_uart=1
dtoverlay=disable-bt
```

## Performance

| Metric | Value |
|--------|-------|
| Radio bandwidth | 9600 baud (~800 bytes/sec effective) |
| Temperature rate | 1 sample/2 sec |
| Image rate | 1 image/20 sec (160x120 JPEG) |
| Image transfer time | ~5-10 seconds |
| Radio range | 100m+ open air, 30-60m through walls |

## License

Licensed under either of [Apache-2.0](../LICENSE-APACHE) or
[MIT](../LICENSE-MIT), at your option.

Copyright (c) 2025-2026 naskel.com
