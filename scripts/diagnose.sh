#!/usr/bin/env bash
set -euo pipefail

# ── Helper: mask sensitive values ─────────────────────────
mask() {
  local val="$1"
  if [[ ${#val} -le 14 ]]; then
    echo "***"
  else
    echo "${val:0:8}...${val: -6}"
  fi
}

usage() {
  cat <<'EOF'
diagnose.sh

Diagnose p2p-ddns + OpenClaw integration status.
EOF
  exit 0
}

[[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] && usage

echo "=== p2p-ddns Diagnosis ==="
echo "Host: $(hostname)"
echo "Date: $(date -Iseconds)"
echo ""

# ── 1. systemd services ───────────────────────────────────
echo "--- systemd services ---"
for svc in p2p-ddns p2p-ddns-agent-transport; do
  echo ""
  echo "[$svc]"
  systemctl status "$svc" 2>/dev/null | head -5 || echo "  Not found"
done

# ── 2. Journal ────────────────────────────────────────────
echo ""
echo "--- Journal (last 80 lines) ---"
for svc in p2p-ddns p2p-ddns-agent-transport; do
  echo ""
  echo "[$svc]"
  sudo journalctl -u "$svc" -n 80 --no-pager 2>/dev/null | tail -20 || echo "  No logs"
done

# ── 3. Service config ─────────────────────────────────────
echo ""
echo "--- Service Config ---"
for svc in p2p-ddns p2p-ddns-agent-transport; do
  echo ""
  echo "[$svc]"
  systemctl cat "$svc" 2>/dev/null || echo "  Not found"
done

# ── 4. Env files ──────────────────────────────────────────
echo ""
echo "--- Env Files ---"
for f in /etc/p2p-ddns/p2p-ddns.env /etc/p2p-ddns/agent-transport.env; do
  echo ""
  echo "[$f]"
  if [[ -f "$f" ]]; then
    while IFS= read -r line; do
      if [[ "$line" =~ SECRET|TICKET|secret|ticket ]]; then
        key="${line%%=*}"
        val="${line#*=}"
        echo "  $key=$(mask "$val")"
      else
        echo "  $line"
      fi
    done < "$f"
  else
    echo "  Not found"
  fi
done

# ── 5. Ports ──────────────────────────────────────────────
echo ""
echo "--- Ports ---"
ss -lntup 2>/dev/null | grep -E '8080|39091|7777' || echo "  No matching ports"
echo ""
ss -lunp 2>/dev/null | grep 7777 || echo "  No UDP 7777"

# ── 6. Docker ─────────────────────────────────────────────
echo ""
echo "--- Docker OpenClaw ---"
OPENCLAW_CONTAINER="$(docker ps --format '{{.Names}}' 2>/dev/null | grep -iE 'claw|openclaw' | head -1 || echo "")"
if [[ -n "$OPENCLAW_CONTAINER" ]]; then
  echo "Container: $OPENCLAW_CONTAINER"
  echo ""
  echo "Mounts:"
  docker inspect "$OPENCLAW_CONTAINER" --format '{{range .Mounts}}  {{.Source}} -> {{.Destination}}{{println}}{{end}}' 2>/dev/null
else
  echo "  No OpenClaw container found"
fi

# ── 7. Health checks ──────────────────────────────────────
echo ""
echo "--- Health Checks ---"

# Host checks
echo ""
echo "[Host -> transport]"
if curl -s -i --max-time 3 http://127.0.0.1:39091/health 2>/dev/null; then
  echo "  health: OK"
else
  echo "  health: FAIL"
fi

echo ""
echo "[Host -> contacts]"
if curl -s --max-time 3 http://127.0.0.1:39091/contacts 2>/dev/null | head -5; then
  echo "  contacts: OK"
else
  echo "  contacts: FAIL"
fi

# Container checks
if [[ -n "${OPENCLAW_CONTAINER:-}" ]]; then
  echo ""
  echo "[Container -> transport]"
  if docker exec "$OPENCLAW_CONTAINER" curl -s --max-time 3 http://host.docker.internal:39091/health 2>/dev/null; then
    echo "  health: OK"
  else
    echo "  health: FAIL"
  fi
fi

# ── 8. Class status ───────────────────────────────────────
echo ""
echo "--- Class Status ---"
if command -v p2p-ddnsctl >/dev/null 2>&1; then
  p2p-ddnsctl --admin-http 127.0.0.1:8080 class info 2>/dev/null || echo "  No class configured"
  echo ""
  p2p-ddnsctl --admin-http 127.0.0.1:8080 class members list 2>/dev/null || echo "  No members"
else
  echo "  p2p-ddnsctl not available"
fi

echo ""
echo "=== Diagnosis complete ==="
