#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="/opt/Aequitas_Core"
BIN_DIR="$INSTALL_DIR/bin"
STATE_DIR="$INSTALL_DIR/state"
SERVICE_SRC="deploy/Aequitas_Core.service"
SERVICE_DEST="/etc/systemd/system/Aequitas_Core.service"

if [ "$EUID" -ne 0 ]; then
  echo "[-] Critical: Deployment must be executed with administrative privileges." >&2
  exit 1
fi

echo "[*] Compiling release binary..."
cargo build --release --locked

echo "[*] Provisioning isolated service user and group..."
getent group aequitas >/dev/null || groupadd -r aequitas
getent passwd aequitas >/dev/null || useradd -r -g aequitas -s /sbin/nologin -d "$INSTALL_DIR" aequitas

echo "[*] Constructing directory layout..."
install -d -m 0555 -o root -g root "$INSTALL_DIR"
install -d -m 0555 -o root -g root "$BIN_DIR"
install -d -m 0755 -o aequitas -g aequitas "$STATE_DIR"

echo "[*] Deploying binary..."
install -m 0555 -o root -g root "target/release/aequitas_core" "$BIN_DIR/aequitas_core"

echo "[*] Installing systemd unit..."
install -m 0644 -o root -g root "$SERVICE_SRC" "$SERVICE_DEST"

echo "[*] Reloading and starting..."
systemctl daemon-reload
systemctl enable --now Aequitas_Core.service

echo "[+] Deployment sealed."
systemctl status Aequitas_Core.service --no-pager || true
