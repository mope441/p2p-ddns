#!/usr/bin/env bash
set -euo pipefail

CONTAINER="${1:-}"

usage() {
  cat <<'EOF'
configure-openclaw-1panel.sh

Configure OpenClaw Docker container with p2p-ddns-lan plugin.

Usage:
  ./configure-openclaw-1panel.sh [container_name]

If container_name is omitted, auto-detects from 'docker ps'.
EOF
  exit 0
}

[[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] && usage

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLUGIN_SRC="$ROOT/plugins/openclaw-p2p-ddns-lan"

if [[ ! -d "$PLUGIN_SRC" ]]; then
  echo "Error: plugin source not found at $PLUGIN_SRC"
  exit 1
fi

# ── 1. Detect container ───────────────────────────────────
if [[ -z "$CONTAINER" ]]; then
  CONTAINER="$(docker ps --format '{{.Names}}' 2>/dev/null | grep -iE 'claw|openclaw' | head -1)"
fi

if [[ -z "$CONTAINER" ]]; then
  echo "Error: No OpenClaw container found. Pass container name as argument."
  echo "Running containers:"
  docker ps --format '{{.Names}}' 2>/dev/null || echo "  (none)"
  exit 1
fi

echo "=== Configure OpenClaw Plugin ==="
echo "Container:  $CONTAINER"

# ── 2. Inspect container ──────────────────────────────────
OPENCLAW_HOME="$(docker inspect "$CONTAINER" --format '{{range .Mounts}}{{if eq .Destination "/home/node/.openclaw"}}{{.Source}}{{end}}{{end}}' 2>/dev/null || echo "")"

if [[ -z "$OPENCLAW_HOME" ]]; then
  # Try alternate mount paths
  OPENCLAW_HOME="$(docker inspect "$CONTAINER" --format '{{range .Mounts}}{{if .Source}}{{.Source}}:{{.Destination}} {{end}}{{end}}' 2>/dev/null | tr ' ' '\n' | grep -i openclaw | head -1 | cut -d: -f1 || echo "")"
fi

if [[ -z "$OPENCLAW_HOME" ]]; then
  echo "Error: Could not detect OpenClaw config directory from Docker mounts."
  echo "Mounts:"
  docker inspect "$CONTAINER" --format '{{range .Mounts}}  {{.Source}} -> {{.Destination}}{{println}}{{end}}'
  exit 1
fi

echo "Config dir: $OPENCLAW_HOME"

# ── 3. Copy plugin ────────────────────────────────────────
PLUGIN_DEST="$OPENCLAW_HOME/extensions/openclaw-p2p-ddns-lan"

echo "Copying plugin..."
sudo mkdir -p "$PLUGIN_DEST"
sudo cp -r "$PLUGIN_SRC"/* "$PLUGIN_DEST/"
sudo chown -R "$(stat -c '%U' "$OPENCLAW_HOME"):$(stat -c '%G' "$OPENCLAW_HOME")" "$PLUGIN_DEST" 2>/dev/null || true

echo "  Plugin copied to: $PLUGIN_DEST"

# ── 4. Restart container ──────────────────────────────────
echo "Restarting container..."
docker restart "$CONTAINER"
sleep 3

# ── 5. Verify ─────────────────────────────────────────────
echo ""
echo "--- Verification ---"

if docker exec "$CONTAINER" ls "/home/node/.openclaw/extensions/openclaw-p2p-ddns-lan" >/dev/null 2>&1; then
  echo "  Plugin dir visible: OK"
else
  echo "  Plugin dir visible: FAIL"
fi

if docker exec "$CONTAINER" openclaw plugins list 2>/dev/null | grep -qi "p2p-ddns-lan"; then
  echo "  Plugin listed: OK"
else
  echo "  Plugin listed: FAIL (may need manual enable)"
fi

echo ""
echo "=== Configure complete ==="
echo "Plugin installed. Restart OpenClaw and enable the plugin:"
echo "  docker exec $CONTAINER openclaw plugin enable p2p-ddns-lan"
