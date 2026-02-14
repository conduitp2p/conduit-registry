#!/usr/bin/env bash
# One-time setup: install conduit-registry systemd service.
#
# Usage (on the registry node):
#   ./deploy/setup-systemd.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SERVICE_FILE="$SCRIPT_DIR/conduit-registry.service"

if [ ! -f "$SERVICE_FILE" ]; then
    echo "Error: $SERVICE_FILE not found"
    exit 1
fi

echo "==> Installing conduit-registry.service"
cp "$SERVICE_FILE" /etc/systemd/system/
systemctl daemon-reload
systemctl enable conduit-registry

echo "==> Stopping any existing nohup process..."
pkill -f "conduit-registry" || true
sleep 2

echo "==> Starting conduit-registry via systemd..."
systemctl start conduit-registry
systemctl status conduit-registry --no-pager

echo ""
echo "Done. Monitor with:"
echo "  journalctl -u conduit-registry -f"
