#!/bin/bash
# HDDS Remote Sensor - Build & Flash ESP32
# Usage: ./build-esp32.sh [flash]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/../sensor-node-arduino"

echo "==========================================="
echo "  HDDS Remote Sensor - ESP32 Build"
echo "==========================================="
echo ""

cd "$PROJECT_DIR"

# Check PlatformIO
if ! command -v pio &> /dev/null; then
    echo "PlatformIO not found. Installing..."
    pip install platformio
fi

echo "[1/3] Checking HDDS-Micro library..."
if [ ! -f "lib/hdds-micro/libhdds_micro_c.a" ]; then
    echo "ERROR: libhdds_micro_c.a not found in lib/hdds-micro/"
    echo ""
    echo "Build it with:"
    echo "  cd /projects/public/hdds"
    echo "  cargo +esp build --release -p hdds-micro-c --target xtensa-esp32-espidf -Zbuild-std=std,panic_abort"
    echo ""
    echo "Then copy:"
    echo "  cp target/xtensa-esp32-espidf/release/libhdds_micro_c.a $PROJECT_DIR/lib/hdds-micro/"
    exit 1
fi

echo "[2/3] Building firmware..."
pio run

if [ "$1" == "flash" ]; then
    echo "[3/3] Flashing to ESP32..."
    pio run -t upload

    echo ""
    echo "Flashed! Opening serial monitor..."
    echo "Press Ctrl+C to exit"
    echo ""
    pio device monitor
else
    echo ""
    echo "Build complete!"
    echo ""
    echo "To flash: $0 flash"
    echo "Or manually: cd $PROJECT_DIR && pio run -t upload"
fi
