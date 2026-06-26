#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
install-student-node.sh

Configure a student node to join a p2p-ddns OpenClaw teaching intranet.

Usage:
  ./install-student-node.sh --node <domain> --invite <file> [options]

Options:
  --node NAME         Node domain name (e.g. "student-a")
  --invite FILE       Path to invite JSON from teacher
  --transport-port P  Agent transport port (default: 39091)
  --config-dir DIR    p2p-ddns config directory (default: auto-detect)
  --openclaw-config F OpenClaw config file path (for plugin configuration)

What this script does:
  1. Detect OpenClaw deployment mode
  2. Extract p2p ticket from invite JSON
  3. Start p2p-ddns daemon with ticket
  4. Join class with invite
  5. Output OpenClaw plugin configuration snippet

Prerequisites:
  - p2p-ddns and p2p-ddnsctl on PATH
  - jq (JSON processor) for invite parsing
EOF
  exit 0
}

NODE=""
INVITE_FILE=""
TRANSPORT_PORT="${P2P_DDNS_TRANSPORT_PORT:-39091}"
CONFIG_DIR="${P2P_DDNS_CONFIG_DIR:-}"
OPENCLAW_CONFIG="${OPENCLAW_CONFIG:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage ;;
    --node) NODE="$2"; shift 2 ;;
    --invite) INVITE_FILE="$2"; shift 2 ;;
    --transport-port) TRANSPORT_PORT="$2"; shift 2 ;;
    --config-dir) CONFIG_DIR="$2"; shift 2 ;;
    --openclaw-config) OPENCLAW_CONFIG="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; usage ;;
  esac
done

if [[ -z "$NODE" || -z "$INVITE_FILE" ]]; then
  echo "Error: --node and --invite are required"
  usage
fi

if [[ ! -f "$INVITE_FILE" ]]; then
  echo "Error: invite file not found: $INVITE_FILE"
  exit 1
fi

# Check for jq
if ! command -v jq >/dev/null 2>&1; then
  echo "Error: jq is required. Install with: sudo apt-get install jq"
  exit 1
fi

echo "=== Student Node Setup ==="
echo "Node domain:  $NODE"
echo "Invite file:  $INVITE_FILE"
echo ""

# ── 1. Parse invite ───────────────────────────────────────

echo "--- Parsing invite ---"
CLASS_NAME="$(jq -r '.class_name' "$INVITE_FILE")"
TEACHER_DOMAIN="$(jq -r '.teacher_domain' "$INVITE_FILE")"
P2P_TICKET="$(jq -r '.p2p_ticket' "$INVITE_FILE")"
SHARED_SECRET="$(jq -r '.shared_secret' "$INVITE_FILE")"
TRANSPORT_PORT="$(jq -r '.transport_port' "$INVITE_FILE")"

echo "Class:        $CLASS_NAME"
echo "Teacher:      $TEACHER_DOMAIN"
echo "Transport:    port $TRANSPORT_PORT"

# ── 2. Detect OpenClaw deployment mode ─────────────────────

detect_openclaw_mode() {
  if docker ps --format '{{.Names}}' 2>/dev/null | grep -qi openclaw; then
    echo "docker"
  elif systemctl is-active --quiet openclaw 2>/dev/null; then
    echo "host"
  else
    echo "unknown"
  fi
}

OC_MODE="$(detect_openclaw_mode)"
echo "OpenClaw mode: $OC_MODE"

# ── 3. Config path ────────────────────────────────────────

if [[ -z "$CONFIG_DIR" ]]; then
  CONFIG_DIR="$HOME/.config/p2p-ddns"
fi
mkdir -p "$CONFIG_DIR"
echo "Config dir:   $CONFIG_DIR"

# Save the ticket for daemon use
echo "$P2P_TICKET" > "$CONFIG_DIR/ticket.txt"

# ── 4. Start daemon with ticket ───────────────────────────

echo ""
echo "--- Starting p2p-ddns daemon ---"

if systemctl is-active --quiet p2p-ddns 2>/dev/null; then
  echo "p2p-ddns service already running, stopping first..."
  sudo systemctl stop p2p-ddns
fi

p2p-ddns --ticket "$P2P_TICKET" --domain "$NODE" --config "$CONFIG_DIR" &
DAEMON_PID=$!
sleep 2

if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
  echo "Error: daemon failed to start"
  exit 1
fi
echo "Daemon started (PID: $DAEMON_PID)"

# ── 5. Join class ─────────────────────────────────────────

echo ""
echo "--- Joining class ---"
p2p-ddnsctl class join --invite "$INVITE_FILE" --socket-path "$CONFIG_DIR/p2p-ddns.sock" || {
  echo "Error: class join failed"
  kill "$DAEMON_PID" 2>/dev/null || true
  exit 1
}

echo "Join request submitted. Waiting for teacher approval."
p2p-ddnsctl class members list --socket-path "$CONFIG_DIR/p2p-ddns.sock"

# ── 6. OpenClaw plugin config output ──────────────────────

echo ""
echo "=== OpenClaw Plugin Configuration ==="
echo ""
echo "Add this to your OpenClaw plugin config:"
echo ""
cat <<PLUGINCFG
{
  "channels": {
    "p2p-ddns-lan": {
      "enabled": true,
      "transportUrl": "http://127.0.0.1:${TRANSPORT_PORT}",
      "sharedSecret": "${SHARED_SECRET}",
      "dmPolicy": "allowlist"
    }
  }
}
PLUGINCFG

# ── 7. Print next steps ───────────────────────────────────

echo ""
echo "=== Student node is ready ==="
echo ""
echo "Your status is: Pending (waiting for teacher approval)"
echo ""
echo "Teacher must run:"
echo "  p2p-ddnsctl class join-requests list"
echo "  p2p-ddnsctl class join-requests approve <your-node-id>"

wait "$DAEMON_PID" 2>/dev/null || true
