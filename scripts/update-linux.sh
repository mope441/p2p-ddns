#!/usr/bin/env bash
set -euo pipefail

# ── Config ────────────────────────────────────────────────
REPO_URL="${P2P_DDNS_REPO:-https://github.com/mope441/p2p-ddns.git}"
BRANCH="${P2P_DDNS_BRANCH:-openclaw-plugin}"
REPO_DIR="${P2P_DDNS_REPO_DIR:-/opt/p2p-ddns-mope-test}"
BACKUP_DIR="/opt/p2p-ddns-backup/$(date +%Y%m%d-%H%M%S)"

usage() {
  cat <<'EOF'
update-linux.sh

Update p2p-ddns binaries from GitHub, rebuild, and restart services.

Usage:
  ./update-linux.sh

Env vars (with defaults):
  P2P_DDNS_REPO       https://github.com/mope441/p2p-ddns.git
  P2P_DDNS_BRANCH     openclaw-plugin
  P2P_DDNS_REPO_DIR   /opt/p2p-ddns-mope-test
EOF
  exit 0
}

[[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] && usage

echo "=== p2p-ddns Update ==="
echo "Repo:     $REPO_URL"
echo "Branch:   $BRANCH"
echo "Dir:      $REPO_DIR"

# ── 1. Clone or fetch ─────────────────────────────────────
if [[ -d "$REPO_DIR/.git" ]]; then
  echo "Fetching updates..."
  cd "$REPO_DIR"
  git fetch origin
  git checkout "$BRANCH"
  git pull origin "$BRANCH"
else
  echo "Cloning repo..."
  if [[ -d "$REPO_DIR" ]]; then
    echo "Directory exists but is not a git repo. Removing..."
    sudo rm -rf "$REPO_DIR"
  fi
  sudo mkdir -p "$(dirname "$REPO_DIR")"
  sudo chown "$USER:$USER" "$(dirname "$REPO_DIR")"
  git clone -b "$BRANCH" "$REPO_URL" "$REPO_DIR"
  cd "$REPO_DIR"
fi

chown -R "$USER:$USER" "$REPO_DIR"

# ── 2. Build ──────────────────────────────────────────────
echo ""
echo "--- Building ---"
cargo fmt --all -- --check || true
cargo build --release --bins

# ── 3. Backup old binaries ────────────────────────────────
BINS=("p2p-ddns" "p2p-ddnsctl" "p2p-ddns-agent-transport")
echo ""
echo "--- Backing up old binaries to $BACKUP_DIR ---"
sudo mkdir -p "$BACKUP_DIR"
for bin in "${BINS[@]}"; do
  if [[ -f "/usr/local/bin/$bin" ]]; then
    sudo cp "/usr/local/bin/$bin" "$BACKUP_DIR/$bin" 2>/dev/null || true
    echo "  Backed up $bin"
  fi
done

# ── 4. Stop services ──────────────────────────────────────
echo ""
echo "--- Stopping services ---"
sudo systemctl stop p2p-ddns-agent-transport 2>/dev/null || true
sudo systemctl stop p2p-ddns 2>/dev/null || true
# Also kill any running processes
sudo pkill -f "p2p-ddns-agent-transport" 2>/dev/null || true
sudo pkill -f "p2p-ddns" 2>/dev/null || true
sleep 1

# ── 5. Install binaries ───────────────────────────────────
echo ""
echo "--- Installing binaries ---"
for bin in "${BINS[@]}"; do
  if [[ -f "target/release/$bin" ]]; then
    sudo cp "target/release/$bin" "/usr/local/bin/$bin"
    sudo chmod 755 "/usr/local/bin/$bin"
    echo "  Installed $bin"
  else
    echo "  WARNING: target/release/$bin not found"
  fi
done

# ── 6. Restart services ───────────────────────────────────
echo ""
echo "--- Restarting services ---"
sudo systemctl start p2p-ddns 2>/dev/null || true
sudo systemctl start p2p-ddns-agent-transport 2>/dev/null || true
sleep 2

# ── 7. Verify ─────────────────────────────────────────────
echo ""
echo "--- Verification ---"
if command -v p2p-ddnsctl >/dev/null 2>&1; then
  echo "p2p-ddnsctl version: $(p2p-ddnsctl --version 2>/dev/null || echo 'N/A')"

  if p2p-ddnsctl --admin-http 127.0.0.1:8080 status 2>/dev/null; then
    echo "  status: OK"
  else
    echo "  status: FAIL (daemon may need manual start)"
  fi

  if p2p-ddnsctl --admin-http 127.0.0.1:8080 class --help 2>/dev/null; then
    echo "  class --help: OK"
  else
    echo "  class --help: FAIL"
  fi
else
  echo "  p2p-ddnsctl not on PATH"
fi

echo ""
echo "=== Update complete ==="
echo "Binaries in:  /usr/local/bin/"
echo "Backup in:    $BACKUP_DIR"
