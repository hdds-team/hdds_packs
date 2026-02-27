# Pi Gateway - Full HDDS Stack

Raspberry Pi Zero 2 W gateway that receives sensor data from HC-12 radio and publishes via full HDDS stack.

## Architecture

```
ESP32-CAM + DHT22          Pi Zero 2 W                   Network
+----------------+        +------------------+        +------------+
| Sensor Node    |   433  | Pi Gateway       |  WiFi  | HDDS       |
| - Camera       | -----> | - HC-12 RX       | -----> | Subscribers|
| - Temp/Humid   |  MHz   | - Full HDDS      |        | - hdds.io  |
| - HC-12 TX     |        | - Web UI (8080)  |        +------------+
+----------------+        +------------------+
```

## Hardware

- **Pi Zero 2 W**: Full Linux = native Rust compilation
- **HC-12** connected via USB-UART adapter (/dev/ttyUSB0)
- WiFi for network connectivity

## Build

```bash
# Native build (on Pi or cross-compile)
cargo build --release

# Binary is ~2.8MB thanks to LTO and strip
ls -lh target/release/hdds-pi-gateway
```

### Cross-compile for ARM64

```bash
# Add target
rustup target add aarch64-unknown-linux-gnu

# Cross-compile
cargo build --release --target aarch64-unknown-linux-gnu
```

## Run

```bash
# Set serial port (default: /dev/ttyUSB0)
./hdds-pi-gateway

# With debug logging
RUST_LOG=debug ./hdds-pi-gateway
```

## Web UI

Access `http://<pi-ip>:8080` for:
- Real-time temperature/humidity display
- Camera image preview
- Radio stats (frames received, errors)

## DDS Topics

The gateway publishes to:
- `hdds_demo/remote_sensor/temperature` - Temperature struct with `value: f32`
- `hdds_demo/remote_sensor/humidity` - Humidity struct with `value: f32`

## Configuration

Edit constants in `src/main.rs`:
```rust
const SERIAL_PORT: &str = "/dev/ttyUSB0";  // HC-12 via USB-UART
const SERIAL_BAUD: u32 = 9600;
const WEB_PORT: u16 = 8080;
const DDS_DOMAIN: u32 = 0;
```

## See Also

- `../sensor-node/` - ESP32-CAM firmware (C/ESP-IDF)
- `../wifi-gateway-esp32-archive/` - Alternative ESP32 gateway using hdds-micro
- HDDS repository - Full HDDS stack
