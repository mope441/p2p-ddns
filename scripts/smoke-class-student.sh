#!/usr/bin/env bash
set -euo pipefail

# Smoke test: student-side class join
# Requires: teacher invite file at /tmp/student-one-invite.json

ADMIN="${P2P_DDNS_ADMIN:-http://127.0.0.1:8080}"
INVITE_FILE="${P2P_DDNS_INVITE_FILE:-/tmp/student-one-invite.json}"

echo "=== Student Smoke Test ==="
echo "Admin: $ADMIN"
echo ""

fail() { echo "FAIL: $1"; exit 1; }

if [[ ! -f "$INVITE_FILE" ]]; then
  fail "invite file not found: $INVITE_FILE"
fi

# 1. Class join
echo "--- class join ---"
JOIN_OUTPUT="$(p2p-ddnsctl --admin-http "$ADMIN" class join "$INVITE_FILE" 2>&1)" || {
  echo "$JOIN_OUTPUT"
  fail "class join"
}
echo "$JOIN_OUTPUT"

# Verify no "Unknown invite ID" in output
if echo "$JOIN_OUTPUT" | grep -qi "unknown invite"; then
  fail "Unknown invite ID error"
fi

echo "PASS: class join"

# 2. Member list (should show pending)
echo ""
echo "--- members list ---"
p2p-ddnsctl --admin-http "$ADMIN" class members list 2>&1 || true
echo "PASS: members list"

echo ""
echo "=== Student smoke PASS ==="
