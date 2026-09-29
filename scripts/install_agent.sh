#!/bin/sh
set -e

# ReDash Agent One-Line Installer
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/reways/redash/main/scripts/install_agent.sh | sh -s -- --hub ws://hub.yourdomain.com:8080/v1/agent/ws --key <KEY>

HUB_URL="ws://127.0.0.1:8080/v1/agent/ws"
NODE_ID="$(hostname)"
AUTH_TOKEN=""
IDENTITY_KEY=""
TRUSTED_KEY=""
VERSION="v0.2.1-beta"
REPO="reways/redash"

while [ $# -gt 0 ]; do
  case "$1" in
    --hub|-h)
      HUB_URL="$2"
      shift 2
      ;;
    --node-id|-n)
      NODE_ID="$2"
      shift 2
      ;;
    --token|-t)
      AUTH_TOKEN="$2"
      shift 2
      ;;
    --key|-k|--trusted-key)
      TRUSTED_KEY="$2"
      shift 2
      ;;
    --identity-key)
      IDENTITY_KEY="$2"
      shift 2
      ;;
    --version|-v)
      VERSION="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Fail before downloading or installing on missing/unsafe configuration.
[ -n "$NODE_ID" ] || NODE_ID="$(hostname)"
case "$NODE_ID" in *[!a-zA-Z0-9_.-]*|'') echo "Invalid node ID" >&2; exit 1;; esac
[ "${#AUTH_TOKEN}" -ge 32 ] || { echo "A unique per-node enrollment token (32+ characters) is required" >&2; exit 1; }
case "$AUTH_TOKEN" in *[!a-zA-Z0-9_-]*) echo "Invalid token characters" >&2; exit 1;; esac
for key in "$TRUSTED_KEY" "$IDENTITY_KEY"; do
  if [ -n "$key" ]; then
    [ "${#key}" -eq 64 ] || { echo "Keys must have 64 hexadecimal characters" >&2; exit 1; }
    case "$key" in *[!a-fA-F0-9]*) echo "Invalid key encoding" >&2; exit 1;; esac
  fi
done
case "$HUB_URL" in *[!a-zA-Z0-9:/._?=\&%+-]*) echo "Invalid Hub URL" >&2; exit 1;; esac

echo ""
echo "  ⚡ ReDash Next-Gen Agent Automated Installer"
echo "  -------------------------------------------------"
echo "  Node ID:     $NODE_ID"
echo "  Hub URL:     $HUB_URL"
echo "  Zero-Trust:  ${TRUSTED_KEY:-Disabled (Open)}"
echo "  Version:     $VERSION"
echo "  -------------------------------------------------"
echo ""

# 1. Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    OS_TAG="linux"
    ;;
  Darwin)
    OS_TAG="macos"
    ;;
  *)
    echo "❌ Unsupported OS: $OS"
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)
    ARCH_TAG="x86_64"
    ;;
  aarch64|arm64)
    ARCH_TAG="aarch64"
    ;;
  *)
    echo "❌ Unsupported architecture: $ARCH"
    exit 1
    ;;
esac

# On macOS arm64 tag
if [ "$OS_TAG" = "macos" ]; then
  ARCH_TAG="arm64"
fi

TAR_NAME="redash-agent-${OS_TAG}-${ARCH_TAG}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${TAR_NAME}"

echo "⬇️  Downloading $DOWNLOAD_URL ..."
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$TAR_NAME" || {
    echo "⚠️  Failed to download release asset from GitHub ($DOWNLOAD_URL)."
    echo "    Attempting local build or fallback..."
  }
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$TMP_DIR/$TAR_NAME" "$DOWNLOAD_URL" || true
fi

BIN_DEST="/usr/local/bin/redash-agent"
if [ -f "$TMP_DIR/$TAR_NAME" ]; then
  tar -xzf "$TMP_DIR/$TAR_NAME" -C "$TMP_DIR"
  sudo cp "$TMP_DIR/redash-agent" "$BIN_DEST"
  sudo chmod +x "$BIN_DEST"
  echo "✅ Installed binary to $BIN_DEST"
else
  # If running in local repo environment
  if [ -f "target/release/redash-agent" ]; then
    sudo cp "target/release/redash-agent" "$BIN_DEST"
    sudo chmod +x "$BIN_DEST"
    echo "✅ Copied local release build to $BIN_DEST"
  else
    echo "❌ Binary not found and download failed."
    exit 1
  fi
fi

# 2. Setup Systemd Service (Linux)
if [ "$OS" = "Linux" ] && [ -d /etc/systemd/system ]; then
  echo "⚙️  Configuring Systemd unit /etc/systemd/system/redash-agent.service ..."

  umask 077
  cat > "$TMP_DIR/redash-agent.env" << ENV_EOF
REDASH_HUB_URL=$HUB_URL
REDASH_NODE_ID=$NODE_ID
REDASH_AUTH_TOKEN=$AUTH_TOKEN
REDASH_TRUSTED_KEY=$TRUSTED_KEY
REDASH_IDENTITY_KEY=$IDENTITY_KEY
ENV_EOF
  sudo install -m 600 "$TMP_DIR/redash-agent.env" /etc/redash-agent.env

  cat << SERVICE_EOF | sudo tee /etc/systemd/system/redash-agent.service > /dev/null
[Unit]
Description=ReDash Host Telemetry & Zero-Trust Remediation Agent
After=network.target docker.service
Wants=docker.service

[Service]
Type=simple
User=root
EnvironmentFile=/etc/redash-agent.env
ExecStart=$BIN_DEST
Restart=always
RestartSec=5s
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
SERVICE_EOF

  sudo systemctl daemon-reload
  sudo systemctl enable --now redash-agent
  echo "🚀 ReDash Agent started via systemd!"
  sudo systemctl status redash-agent --no-pager || true
fi

echo ""
if [ "$OS" = "Linux" ] && [ -d /etc/systemd/system ]; then
  echo "Agent service configured. Verify enrollment and telemetry in the Hub."
else
  echo "Binary installed. macOS service setup is manual: run redash-agent with the provisioned environment variables."
fi
echo ""
