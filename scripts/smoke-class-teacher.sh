#!/usr/bin/env bash
set -euo pipefail

# Smoke test: teacher-side class operations
# Requires: p2p-ddns daemon running with --admin-http 127.0.0.1:8080

ADMIN="${P2P_DDNS_ADMIN:-http://127.0.0.1:8080}"
INVITE_FILE="${P2P_DDNS_INVITE_FILE:-/tmp/student-one-invite.json}"

echo "=== Teacher Smoke Test ==="
echo "Admin: $ADMIN"
echo ""

fail() { echo "FAIL: $1"; exit 1; }

# 1. Class create
echo "--- class create ---"
if p2p-ddnsctl --admin-http "$ADMIN" class create "AI" 2>&1; then
  echo "PASS: class create"
else
  # Class may already exist, try info
  if p2p-ddnsctl --admin-http "$ADMIN" class info 2>&1; then
    echo "PASS: class already exists"
  else
    fail "class create"
  fi
fi

# 2. Class info
echo ""
echo "--- class info ---"
if p2p-ddnsctl --admin-http "$ADMIN" class info 2>&1; then
  echo "PASS: class info"
else
  fail "class info"
fi

# 3. Members list
echo ""
echo "--- members list ---"
if p2p-ddnsctl --admin-http "$ADMIN" class members list 2>&1; then
  echo "PASS: members list"
else
  fail "members list"
fi

# 4. Invite create
echo ""
echo "--- invite create ---"
if p2p-ddnsctl --admin-http "$ADMIN" class invite create \
  --display-name "Student One" \
  --expires 86400 \
  --output "$INVITE_FILE" 2>&1; then
  echo "PASS: invite create"
else
  fail "invite create"
fi

# 5. Verify invite file
if [[ -f "$INVITE_FILE" ]]; then
  echo "PASS: invite file exists ($INVITE_FILE)"
  # Show invite content (mask secret)
  if command -v jq >/dev/null 2>&1; then
    jq 'del(.shared_secret, .p2p_ticket)' "$INVITE_FILE" 2>/dev/null || true
  fi
else
  fail "invite file not created"
fi

echo ""
echo "=== Teacher smoke PASS ==="
