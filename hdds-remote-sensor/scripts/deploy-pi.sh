#!/bin/bash
# HDDS Remote Sensor - Deploy to Raspberry Pi
# Usage: ./deploy-pi.sh <pi-hostname-or-ip>

set -e

PI_HOST="${1:-pi-gateway.local}"
PI_USER="${PI_USER:-pi}"
BINARY_PATH="../pi-gateway/target/aarch64-unknown-linux-gnu/release/hdds-pi-gateway"

echo "==========================================="
echo "  HDDS Remote Sensor - Pi Gateway Deploy"
echo "==========================================="
echo ""
echo "Target: ${PI_USER}@${PI_HOST}"
echo ""

# Check binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo "ERROR: Binary not found at $BINARY_PATH"
    echo "Run: cd pi-gateway && cargo build --release --target aarch64-unknown-linux-gnu"
    exit 1
fi

echo "[1/4] Copying binary..."
scp "$BINARY_PATH" "${PI_USER}@${PI_HOST}:/tmp/hdds-pi-gateway"

echo "[2/4] Installing binary..."
ssh "${PI_USER}@${PI_HOST}" "sudo mv /tmp/hdds-pi-gateway /usr/local/bin/ && sudo chmod +x /usr/local/bin/hdds-pi-gateway"

echo "[3/4] Installing systemd service..."
ssh "${PI_USER}@${PI_HOST}" "sudo tee /etc/systemd/system/hdds-gateway.service > /dev/null" << 'EOF'
[Unit]
Description=HDDS Remote Sensor Gateway
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/hdds-pi-gateway
Restart=always
RestartSec=5
Environment=RUST_LOG=info
# UDP unicast peer (optional, for discovery)
# Environment=HDDS_PEER=192.168.1.100:7411
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
ssh "${PI_USER}@${PI_HOST}" "sudo systemctl daemon-reload && sudo systemctl enable hdds-gateway && sudo systemctl restart hdds-gateway"

echo ""
echo "Done! Check status with:"
echo "  ssh ${PI_USER}@${PI_HOST} 'sudo systemctl status hdds-gateway'"
echo "  ssh ${PI_USER}@${PI_HOST} 'sudo journalctl -u hdds-gateway -f'"
