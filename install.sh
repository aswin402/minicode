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
