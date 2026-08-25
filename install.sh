#!/usr/bin/env bash
set -e

# minicode installer script
# Repository: https://github.com/aswin402/minicode

REPO="aswin402/minicode"
INSTALL_DIR="${MINICODE_INSTALL_DIR:-$HOME/.local/bin}"

echo "⚡ Installing minicode..."

# Detect OS and Architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    linux)
        case "$ARCH" in
            x86_64)
                TARGET="x86_64-unknown-linux-gnu"
                ;;
            aarch64|arm64)
                TARGET="aarch64-unknown-linux-gnu"
                ;;
            *)
                echo "❌ Unsupported architecture: $ARCH on Linux"
                exit 1
                ;;
        esac
        ;;
    darwin)
        case "$ARCH" in
            x86_64)
                TARGET="x86_64-apple-darwin"
                ;;
            arm64|aarch64)
                TARGET="aarch64-apple-darwin"
                ;;
            *)
                echo "❌ Unsupported architecture: $ARCH on macOS"
                exit 1
                ;;
        esac
        ;;
    *)
        echo "❌ Unsupported operating system: $OS"
        exit 1
        ;;
esac

# Fetch latest release tag
LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "⚠️ Unable to detect latest release version via GitHub API. Falling back to build from source via cargo..."
    cargo install --git "https://github.com/$REPO.git"
    echo "✔ minicode installed via cargo!"
    exit 0
fi

ASSET_NAME="minicode-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${LATEST_TAG}/${ASSET_NAME}"

echo "📥 Downloading minicode ${LATEST_TAG} for ${TARGET}..."
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_NAME"
tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR"

mkdir -p "$INSTALL_DIR"
mv "$TMP_DIR/minicode" "$INSTALL_DIR/minicode"
chmod +x "$INSTALL_DIR/minicode"

echo "✨ minicode successfully installed to $INSTALL_DIR/minicode"

# ── Optional: Obscura headless engine ────────────────────────────────────────
# Preferred browser automation engine (Pure Rust, stealth). Fully optional:
# without it, browser tools fall back to Firefox/Chrome via CDP.
install_obscura() {
    if command -v obscura >/dev/null 2>&1; then
        echo "✔ Obscura already installed ($(command -v obscura))"
        return 0
    fi

    # Map platform to Obscura release asset names ({arch}-{os}[-{variant}]).
    local OBS_ARCH OBS_OS ASSET FILE_URL
    case "$ARCH" in
        x86_64)          OBS_ARCH="x86_64" ;;
        aarch64|arm64)   OBS_ARCH="aarch64" ;;
        *) echo "⚠️ Skipping Obscura: unsupported architecture '$ARCH'"; return 0 ;;
    esac
    case "$OS" in
        linux)  OBS_OS="linux" ;;
        darwin) OBS_OS="macos" ;;
        *)      echo "⚠️ Skipping Obscura: unsupported OS '$OS'"; return 0 ;;
    esac

    # stealth build enables anti-bot evasion while keeping screenshot support.
    local VARIANT="${OBSCURA_VARIANT:-stealth}"
    local OBS_TAG
    OBS_TAG=$(curl -s "https://api.github.com/repos/h4ckf0r0day/obscura/releases/latest" \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$OBS_TAG" ]; then
        echo "⚠️ Skipping Obscura: could not detect latest release"
        return 0
    fi

    ASSET="obscura-${OBS_ARCH}-${OBS_OS}-${VARIANT}.tar.gz"
    FILE_URL="https://github.com/h4ckf0r0day/obscura/releases/download/${OBS_TAG}/${ASSET}"
    echo "📥 Downloading Obscura ${OBS_TAG} (${ASSET})..."
    if ! curl -fsSL "$FILE_URL" -o "$TMP_DIR/$ASSET"; then
        echo "⚠️ Skipping Obscura: download failed (browser tools will use Firefox/Chrome)"
        return 0
    fi

    mkdir -p "$TMP_DIR/obscura_extract"
    tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR/obscura_extract"
    local BIN_PATH
    BIN_PATH=$(find "$TMP_DIR/obscura_extract" -type f -name 'obscura' | head -1)
    if [ -z "$BIN_PATH" ]; then
        echo "⚠️ Skipping Obscura: binary not found in archive"
        return 0
    fi
    mv "$BIN_PATH" "$INSTALL_DIR/obscura"
    chmod +x "$INSTALL_DIR/obscura"
    echo "✨ Obscura installed to $INSTALL_DIR/obscura (engine priority: Obscura → Firefox → Chrome)"
}

if [ "${SKIP_OBSCURA:-0}" = "1" ]; then
    echo "⏭️  SKIP_OBSCURA=1 set — not installing Obscura (Firefox/Chrome fallback will be used)"
else
    install_obscura || true
fi

# Check PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo ""
        echo "⚠️ Note: $INSTALL_DIR is not currently in your PATH."
        echo "Add the following line to your ~/.bashrc or ~/.zshrc:"
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        ;;
esac

echo ""
echo "🚀 Run 'minicode' to launch the TUI, or 'minicode --help' for CLI options."
