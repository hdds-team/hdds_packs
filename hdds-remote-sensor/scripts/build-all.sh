#!/bin/bash
# HDDS Remote Sensor - Build All Components
# Usage: ./build-all.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR/.."

echo "==========================================="
echo "  HDDS Remote Sensor - Build All"
echo "==========================================="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

success() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
fail() { echo -e "${RED}[FAIL]${NC} $1"; }

# 1. Build HDDS-Micro C SDK, which needs the esp toolchain AND an hdds
# checkout: the C SDK is built from the hdds sources, not from this repository.
# `command -v cargo +esp` is not a test for that toolchain, it finds plain
# cargo and succeeds everywhere, after which the cd below used to abort the
# whole script under `set -e` and steps 2 to 4 never ran.
echo ""
echo "=== 1. HDDS-Micro C SDK (ESP32) ==="
if ! cargo +esp --version > /dev/null 2>&1; then
    warn "esp-rs toolchain not available, skipping ESP32 SDK build"
elif [ ! -d "${HDDS_SRC:-../hdds}" ]; then
    warn "no hdds checkout at ${HDDS_SRC:-../hdds}, skipping ESP32 SDK build"
    warn "point HDDS_SRC at one to build it"
else
    cd "${HDDS_SRC:-../hdds}"
    if cargo +esp build --release -p hdds-micro-c \
        --target xtensa-esp32-espidf \
        -Zbuild-std=std,panic_abort; then
        success "Built libhdds_micro_c.a"
        cp target/xtensa-esp32-espidf/release/libhdds_micro_c.a \
           "$PROJECT_DIR/sensor-node-arduino/lib/hdds-micro/"
        success "Copied to sensor-node-arduino/lib/hdds-micro/"
    else
        warn "ESP32 build failed"
    fi
fi

# 2. Build Pi Gateway (cross-compile for ARM64)
echo ""
echo "=== 2. Pi Gateway (ARM64) ==="
cd "$PROJECT_DIR/pi-gateway"
if cargo build --release --target aarch64-unknown-linux-gnu; then
    success "Built hdds-pi-gateway (ARM64)"
    ls -lh target/aarch64-unknown-linux-gnu/release/hdds-pi-gateway
else
    fail "Pi Gateway build failed"
fi

# 3. Build Web Viewer (native + ARM64)
echo ""
echo "=== 3. Web Viewer ==="
cd "$PROJECT_DIR/web-viewer"
if cargo build --release; then
    success "Built hdds-web-viewer (native)"
    ls -lh target/release/hdds-web-viewer
else
    fail "Web Viewer native build failed"
fi

if cargo build --release --target aarch64-unknown-linux-gnu 2>/dev/null; then
    success "Built hdds-web-viewer (ARM64)"
else
    warn "Web Viewer ARM64 build failed (might not have cross-linker)"
fi

# 4. Summary
echo ""
echo "==========================================="
echo "  Build Summary"
echo "==========================================="
echo ""

check_file() {
    if [ -f "$1" ]; then
        success "$2: $(du -h "$1" | cut -f1)"
    else
        fail "$2: NOT FOUND"
    fi
}

check_file "$PROJECT_DIR/sensor-node-arduino/lib/hdds-micro/libhdds_micro_c.a" "ESP32 SDK"
check_file "$PROJECT_DIR/pi-gateway/target/aarch64-unknown-linux-gnu/release/hdds-pi-gateway" "Pi Gateway (ARM64)"
check_file "$PROJECT_DIR/web-viewer/target/release/hdds-web-viewer" "Web Viewer (native)"

echo ""
echo "Next steps:"
echo "  1. Flash ESP32: ./scripts/build-esp32.sh flash"
echo "  2. Deploy Pi:   ./scripts/deploy-pi.sh <pi-host>"
echo "  3. Deploy Web:  ./scripts/deploy-server.sh <server>"
