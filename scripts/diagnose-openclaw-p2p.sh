#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
diagnose-openclaw-p2p.sh

Diagnose p2p-ddns + OpenClaw plugin integration status.

Usage:
  ./diagnose-openclaw-p2p.sh [options]

Options:
  --transport-url URL  Agent transport URL (default: http://127.0.0.1:39091)
  --admin-http URL     p2p-ddns admin HTTP endpoint (default: auto-detect)

Checks:
  - p2p-ddns daemon running
  - p2p-ddns-agent-transport port reachable
  - OpenClaw container/service running
  - OpenClaw plugin enabled
  - transportUrl reachable
  - contacts API responding
  - directory peers visible
EOF
  exit 0
}

TRANSPORT_URL="${P2P_DDNS_TRANSPORT_URL:-http://127.0.0.1:39091}"
ADMIN_HTTP="${P2P_DDNS_ADMIN_HTTP:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage ;;
    --transport-url) TRANSPORT_URL="$2"; shift 2 ;;
    --admin-http) ADMIN_HTTP="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; usage ;;
  esac
done

PASS=0
FAIL=0

check() {
  local label="$1"
  local result="$2"
  local detail="${3:-}"
  if [[ "$result" == "ok" ]]; then
    echo "  [PASS] $label"
    PASS=$((PASS + 1))
  else
    echo "  [FAIL] $label"
    if [[ -n "$detail" ]]; then
      echo "         $detail"
    fi
    FAIL=$((FAIL + 1))
  fi
}

echo "=== p2p-ddns OpenClaw Integration Diagnosis ==="
echo ""

# ── 1. p2p-ddns daemon ────────────────────────────────────

echo "--- p2p-ddns Daemon ---"

if systemctl is-active --quiet p2p-ddns 2>/dev/null; then
  check "p2p-ddns.service active" "ok"
elif pgrep -f "p2p-ddns.*--primary\|p2p-ddns.*--ticket" >/dev/null 2>&1; then
  check "p2p-ddns process running" "ok" "(manual process, not systemd)"
else
  check "p2p-ddns running" "fail" "Daemon not found. Start with: p2p-ddns --primary --domain <name>"
fi

# ── 2. Transport port reachable ────────────────────────────

echo ""
echo "--- Agent Transport ---"

TRANSPORT_HOST="$(echo "$TRANSPORT_URL" | sed -E 's|https?://([^:/]+).*|\1|')"
TRANSPORT_PORT="$(echo "$TRANSPORT_URL" | grep -oP ':\K\d+' || echo "39091")"

if timeout 2 bash -c "echo >/dev/tcp/${TRANSPORT_HOST}/${TRANSPORT_PORT}" 2>/dev/null; then
  check "transport port $TRANSPORT_PORT reachable" "ok"
else
  check "transport port $TRANSPORT_PORT reachable" "fail" \
    "Cannot connect to $TRANSPORT_HOST:$TRANSPORT_PORT. Is agent transport running?"
fi

# ── 3. Daemon admin ────────────────────────────────────────

echo ""
echo "--- p2p-ddns Admin ---"

if [[ -n "$ADMIN_HTTP" ]]; then
  if curl -s --max-time 2 "$ADMIN_HTTP/health" >/dev/null 2>&1; then
    check "admin HTTP health" "ok" "($ADMIN_HTTP)"
  else
    check "admin HTTP health" "fail" "$ADMIN_HTTP unreachable"
  fi
else
  # Try local socket
  SOCKET_PATH="/run/p2p-ddns/p2p-ddns.sock"
  if [[ -S "$SOCKET_PATH" ]]; then
    check "admin socket exists" "ok" "($SOCKET_PATH)"
  else
    check "admin socket exists" "fail" "Socket not found at $SOCKET_PATH"
  fi
fi

# ── 4. OpenClaw container/service ─────────────────────────

echo ""
echo "--- OpenClaw ---"

OPENCLAW_RUNNING="false"
if docker ps --format '{{.Names}}' 2>/dev/null | grep -qi openclaw; then
  CONTAINER="$(docker ps --format '{{.Names}}' 2>/dev/null | grep -i openclaw | head -1)"
  check "OpenClaw container" "ok" "($CONTAINER)"
  OPENCLAW_RUNNING="true"
elif systemctl is-active --quiet openclaw 2>/dev/null; then
  check "OpenClaw service" "ok" "(systemd)"
  OPENCLAW_RUNNING="true"
else
  check "OpenClaw running" "fail" "No OpenClaw container or service found"
fi

# ── 5. OpenClaw plugin ─────────────────────────────────────

echo ""
echo "--- OpenClaw Plugin ---"

if [[ "$OPENCLAW_RUNNING" == "true" ]] && command -v openclaw >/dev/null 2>&1; then
  if openclaw plugin list 2>/dev/null | grep -qi "p2p-ddns-lan"; then
    check "p2p-ddns-lan plugin installed" "ok"
    if openclaw plugin list 2>/dev/null | grep -qi "p2p-ddns-lan.*enabled"; then
      check "p2p-ddns-lan plugin enabled" "ok"
    else
      check "p2p-ddns-lan plugin enabled" "fail" "Plugin installed but not enabled"
    fi
  else
    check "p2p-ddns-lan plugin installed" "fail" "Plugin not found. Install from plugins/openclaw-p2p-ddns-lan/"
  fi
else
  check "openclaw CLI available" "fail" "Cannot verify plugin. Install openclaw CLI first."
fi

# ── 6. Contacts API ────────────────────────────────────────

echo ""
echo "--- Contacts API ---"

CONTACTS_URL="${TRANSPORT_URL}/contacts"
if CONTACTS_RESP="$(curl -s --max-time 3 "$CONTACTS_URL" 2>/dev/null)"; then
  CONTACT_COUNT="$(echo "$CONTACTS_RESP" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "?")"
  check "contacts API" "ok" "($CONTACT_COUNT peers found)"
else
  check "contacts API" "fail" "$CONTACTS_URL unreachable"
fi

# ── 7. Summary ─────────────────────────────────────────────

echo ""
echo "=== Diagnosis Summary ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""

if [[ "$FAIL" -eq 0 ]]; then
  echo "All checks passed. System is healthy."
  exit 0
else
  echo "$FAIL check(s) failed. Review the [FAIL] items above."
  exit 1
fi
