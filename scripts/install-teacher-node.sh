#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
install-teacher-node.sh

Configure a teacher node for the p2p-ddns OpenClaw teaching intranet.

Usage:
  ./install-teacher-node.sh --node <domain> --class <name> [options]

Options:
  --node NAME         Node domain name (e.g. "teacher-claw")
  --class NAME        Human-readable class name (e.g. "AI Course")
  --transport-port P  Agent transport port (default: 39091)
  --config-dir DIR    p2p-ddns config directory (default: auto-detect)
  --output FILE       Write invite JSON to this file (default: invite.json)

What this script does:
  1. Detect OpenClaw deployment mode (Docker vs host)
  2. Detect Docker network mode (host vs bridge)
  3. Ensure p2p-ddns systemd service is installed
  4. Start daemon with --primary
  5. Create class network
  6. Print invite example command

Prerequisites:
  - p2p-ddns and p2p-ddnsctl on PATH
  - systemd (Linux)
EOF
  exit 0
}

NODE=""
CLASS=""
TRANSPORT_PORT="${P2P_DDNS_TRANSPORT_PORT:-39091}"
CONFIG_DIR="${P2P_DDNS_CONFIG_DIR:-}"
OUTPUT_FILE="${P2P_DDNS_OUTPUT:-invite.json}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage ;;
    --node) NODE="$2"; shift 2 ;;
    --class) CLASS="$2"; shift 2 ;;
    --transport-port) TRANSPORT_PORT="$2"; shift 2 ;;
    --config-dir) CONFIG_DIR="$2"; shift 2 ;;
    --output) OUTPUT_FILE="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; usage ;;
  esac
done

if [[ -z "$NODE" || -z "$CLASS" ]]; then
  echo "Error: --node and --class are required"
  usage
fi

echo "=== Teacher Node Setup ==="
echo "Node domain:  $NODE"
echo "Class name:   $CLASS"
echo "Transport:    port $TRANSPORT_PORT"
echo ""

# ── 1. Detect OpenClaw deployment mode ─────────────────────

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

# ── 2. Detect Docker network mode ──────────────────────────

DOCKER_HOST_NET="false"
if [[ "$OC_MODE" == "docker" ]]; then
  OPENCLAW_CONTAINER="$(docker ps --format '{{.Names}}' 2>/dev/null | grep -i openclaw | head -1)"
  if [[ -n "$OPENCLAW_CONTAINER" ]]; then
    NET_MODE="$(docker inspect "$OPENCLAW_CONTAINER" --format '{{.HostConfig.NetworkMode}}' 2>/dev/null || echo "unknown")"
    if [[ "$NET_MODE" == "host" ]]; then
      DOCKER_HOST_NET="true"
    fi
    echo "Container:    $OPENCLAW_CONTAINER"
    echo "Network mode: $NET_MODE"
  fi
fi

# ── 3. Config path detection ──────────────────────────────

if [[ -z "$CONFIG_DIR" ]]; then
  CONFIG_DIR="$HOME/.config/p2p-ddns"
fi
mkdir -p "$CONFIG_DIR"
echo "Config dir:   $CONFIG_DIR"

# ── 4. Start daemon ───────────────────────────────────────

echo ""
echo "--- Starting p2p-ddns daemon ---"

# Check if already running
if systemctl is-active --quiet p2p-ddns 2>/dev/null; then
  echo "p2p-ddns service already running, stopping first..."
  sudo systemctl stop p2p-ddns
fi

# Build daemon args
DAEMON_ARGS="--primary --domain \"$NODE\""
if [[ -n "$CONFIG_DIR" ]]; then
  DAEMON_ARGS="$DAEMON_ARGS --config \"$CONFIG_DIR\""
fi

# Start daemon in background
echo "Starting: p2p-ddns $DAEMON_ARGS"
p2p-ddns $DAEMON_ARGS &
DAEMON_PID=$!
sleep 2

if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
  echo "Error: daemon failed to start"
  exit 1
fi
echo "Daemon started (PID: $DAEMON_PID)"

# ── 5. Create class network ───────────────────────────────

echo ""
echo "--- Creating class network ---"
p2p-ddnsctl class create "$CLASS" --socket-path "$CONFIG_DIR/p2p-ddns.sock" || {
  echo "Error: class create failed"
  kill "$DAEMON_PID" 2>/dev/null || true
  exit 1
}

p2p-ddnsctl class info --socket-path "$CONFIG_DIR/p2p-ddns.sock"

# ── 6. Print next steps ───────────────────────────────────

echo ""
echo "=== Teacher node is ready ==="
echo ""
echo "To create a student invite:"
echo "  p2p-ddnsctl class invite create --display-name \"Student Name\" --output invite.json"
echo ""
echo "To stop the daemon:"
echo "  p2p-ddnsctl stop"
echo "  # or: kill $DAEMON_PID"

# Keep daemon running in foreground
echo ""
echo "Daemon running. Press Ctrl+C to stop."
wait "$DAEMON_PID" 2>/dev/null || true
