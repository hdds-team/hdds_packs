#!/bin/bash
# HDDS Remote Sensor - Deploy Web Viewer to Server
# Usage: ./deploy-server.sh <server-hostname-or-ip>

set -e

SERVER_HOST="${1:-hdds.io}"
SERVER_USER="${SERVER_USER:-deploy}"
BINARY_PATH="../web-viewer/target/release/hdds-web-viewer"
BINARY_PATH_ARM="../web-viewer/target/aarch64-unknown-linux-gnu/release/hdds-web-viewer"

echo "==========================================="
echo "  HDDS Remote Sensor - Web Viewer Deploy"
echo "==========================================="
echo ""
echo "Target: ${SERVER_USER}@${SERVER_HOST}"
echo ""

# Determine which binary to use
if [ -f "$BINARY_PATH" ]; then
    BINARY="$BINARY_PATH"
    echo "Using x86_64 binary"
elif [ -f "$BINARY_PATH_ARM" ]; then
    BINARY="$BINARY_PATH_ARM"
    echo "Using aarch64 binary"
else
    echo "ERROR: Binary not found"
    echo "Run: cd web-viewer && cargo build --release"
    exit 1
fi

echo "[1/4] Copying binary..."
scp "$BINARY" "${SERVER_USER}@${SERVER_HOST}:/tmp/hdds-web-viewer"

echo "[2/4] Installing binary..."
ssh "${SERVER_USER}@${SERVER_HOST}" "sudo mv /tmp/hdds-web-viewer /usr/local/bin/ && sudo chmod +x /usr/local/bin/hdds-web-viewer"

echo "[3/4] Installing systemd service..."
ssh "${SERVER_USER}@${SERVER_HOST}" "sudo tee /etc/systemd/system/hdds-viewer.service > /dev/null" << 'EOF'
[Unit]
Description=HDDS Remote Sensor Web Viewer
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/hdds-web-viewer
Restart=always
RestartSec=5
Environment=RUST_LOG=info
# UDP unicast peer (optional, for discovery)
# Environment=HDDS_PEER=192.168.1.50:7411
# TCP transport for WAN (recommended)
Environment=HDDS_TCP_PORT=7410
# QUIC transport for NAT traversal (optional)
# Environment=HDDS_QUIC=1
# LowBw transport preset (optional: hc12, lora, satellite)
# Environment=HDDS_LOWBW_PRESET=hc12

[Install]
WantedBy=multi-user.target
EOF

echo "[4/4] Enabling and starting service..."
ssh "${SERVER_USER}@${SERVER_HOST}" "sudo systemctl daemon-reload && sudo systemctl enable hdds-viewer && sudo systemctl restart hdds-viewer"

echo ""
echo "Done! Service running on http://${SERVER_HOST}:8080"
echo ""
echo "Check status with:"
echo "  ssh ${SERVER_USER}@${SERVER_HOST} 'sudo systemctl status hdds-viewer'"
echo "  ssh ${SERVER_USER}@${SERVER_HOST} 'sudo journalctl -u hdds-viewer -f'"
