#!/usr/bin/env bash
# ==============================================================================
# minicode — Remote & Source Installer Script (cargo-pretty UI edition)
# Repository: https://github.com/aswin402/minicode
# Inspired by https://github.com/romancitodev/cargo-pretty
# ==============================================================================

set -euo pipefail

REPO="aswin402/minicode"
INSTALL_DIR="${MINICODE_INSTALL_DIR:-$HOME/.local/bin}"

# ── cargo-pretty Truecolor Palette (Rust Gradient + Catppuccin Mocha) ─────────
BOLD="\033[1m"
RESET="\033[0m"

RAMP_0="\033[38;2;193;68;14m"
RAMP_1="\033[38;2;212;100;46m"
RAMP_2="\033[38;2;232;131;78m"
RAMP_3="\033[38;2;240;166;119m"
RAMP_4="\033[38;2;246;201;160m"
RAMP_5="\033[38;2;250;228;208m"
RAMP_6="\033[38;2;255;255;255m"

GREEN="\033[38;2;166;227;161m"
YELLOW="\033[38;2;249;226;175m"
RED="\033[38;2;243;139;168m"
SAPPHIRE="\033[38;2;116;199;236m"
SUBTEXT0="\033[38;2;166;173;200m"
SURFACE1="\033[38;2;69;71;90m"

render_progress_bar() {
    local pct="$1"
    local width=28
    local filled=$(( pct * width / 100 ))
    if [ "$filled" -gt "$width" ]; then filled="$width"; fi
    local bar=""
    local i=0
    while [ "$i" -lt "$width" ]; do
        if [ "$i" -lt "$filled" ]; then
            if [ "$i" -lt 4 ]; then bar+="${RAMP_0}█"
            elif [ "$i" -lt 8 ]; then bar+="${RAMP_1}█"
            elif [ "$i" -lt 12 ]; then bar+="${RAMP_2}█"
            elif [ "$i" -lt 16 ]; then bar+="${RAMP_3}█"
            elif [ "$i" -lt 20 ]; then bar+="${RAMP_4}█"
            elif [ "$i" -lt 24 ]; then bar+="${RAMP_5}█"
            else bar+="${RAMP_6}█"
            fi
        else
            bar+="${SURFACE1}░"
        fi
        i=$(( i + 1 ))
    done
    bar+="${RESET}"
    printf "%b" "$bar"
}

echo ""
echo -e "  ${RAMP_0}${BOLD}▄▄▄▄███▄▄▄▄    ▄█  ███▄▄▄▄    ▄█   ▄████████  ▄██████▄  ████████▄     ▄████████${RESET}"
echo -e "  ${RAMP_2}${BOLD}▄██▀▀▀███▀▀▀██▄ ███  ███▀▀▀██▄ ███  ███    ███ ███    ███ ███   ▀███   ███    ███${RESET}"
echo -e "  ${RAMP_4}${BOLD}███   ███   ███ ███▌ ███   ███ ███▌ ███    █▀  ███    ███ ███    ███   ███    █▀ ${RESET}"
echo -e "  ${RAMP_6}${BOLD}███   ███   ███ ███▌ ███   ███ ███▌ ███        ███    ███ ███    ███  ▄███▄▄▄    ${RESET}"
echo -e "  ${RAMP_4}${BOLD}███   ███   ███ ███▌ ███   ███ ███▌ ███        ███    ███ ███    ███ ▀▀███▀▀▀    ${RESET}"
echo -e "  ${RAMP_2}${BOLD}███   ███   ███ ███  ███   ███ ███  ███    █▄  ███    ███ ███    ███   ███    █▄ ${RESET}"
echo -e "  ${RAMP_0}${BOLD}███   ███   ███ ███  ███   ███ ███  ████████▀   ▀██████▀  ████████▀    ██████████${RESET}"
echo ""
echo -e "  ${SAPPHIRE}${BOLD}minicode installer${RESET} ${SUBTEXT0}(${REPO})${RESET}"
echo -e "  ${SURFACE1}────────────────────────────────────────${RESET}"

START_SEC=$(date +%s)

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
                echo -e "  ${RED}${BOLD}✗ Unsupported architecture: $ARCH on Linux${RESET}"
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
                echo -e "  ${RED}${BOLD}✗ Unsupported architecture: $ARCH on macOS${RESET}"
                exit 1
                ;;
        esac
        ;;
    *)
        echo -e "  ${RED}${BOLD}✗ Unsupported operating system: $OS${RESET}"
        exit 1
        ;;
esac

echo -e "    ${SUBTEXT0}target        ${RESET} ${SAPPHIRE}${TARGET}${RESET}"

# Fetch latest release tag
LATEST_TAG=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)

if [ -z "$LATEST_TAG" ]; then
    echo -e "  ${YELLOW}⚠ Release tag unavailable via GitHub API — building from source via cargo...${RESET}"
    cargo install --git "https://github.com/$REPO.git"
    END_SEC=$(date +%s)
    ELAPSED=$(( END_SEC - START_SEC ))
    echo -e "  ${GREEN}${BOLD}✓ minicode built & installed via cargo in ${ELAPSED}s${RESET}"
    exit 0
fi

ASSET_NAME="minicode-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${LATEST_TAG}/${ASSET_NAME}"

echo -e "    ${SUBTEXT0}release       ${RESET} ${SAPPHIRE}${LATEST_TAG}${RESET}"
BAR_50=$(render_progress_bar 50)
printf "  ${SUBTEXT0}Download     ${RESET} %b  ${BOLD} 50%%${RESET} ${SUBTEXT0}(${ASSET_NAME})${RESET}\n" "$BAR_50"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_NAME"
tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR"

mkdir -p "$INSTALL_DIR"
mv "$TMP_DIR/minicode" "$INSTALL_DIR/minicode"
chmod +x "$INSTALL_DIR/minicode"

BAR_100=$(render_progress_bar 100)
printf "  ${SUBTEXT0}Install      ${RESET} %b  ${BOLD}100%%${RESET}\n\n" "$BAR_100"

END_SEC=$(date +%s)
ELAPSED=$(( END_SEC - START_SEC ))
BIN_SIZE=$(ls -lh "$INSTALL_DIR/minicode" | awk '{print $5}')

echo -e "  ${GREEN}${BOLD}✓ minicode ${LATEST_TAG} installed in ${ELAPSED}s${RESET}"
echo -e "    ${SUBTEXT0}binary size   ${RESET} ${SAPPHIRE}${BIN_SIZE}${RESET}"
echo -e "    ${SUBTEXT0}installed to  ${RESET} ${GREEN}${INSTALL_DIR}/minicode${RESET}"

# ── Optional: Obscura headless engine ────────────────────────────────────────
# Preferred browser automation engine (Pure Rust, stealth). Fully optional:
# without it, browser tools fall back to Firefox/Chrome via CDP.
install_obscura() {
    if command -v obscura >/dev/null 2>&1; then
        echo -e "    ${SUBTEXT0}obscura       ${RESET} ${GREEN}already installed ($(command -v obscura))${RESET}"
        return 0
    fi

    local OBS_ARCH OBS_OS ASSET FILE_URL
    case "$ARCH" in
        x86_64)          OBS_ARCH="x86_64" ;;
        aarch64|arm64)   OBS_ARCH="aarch64" ;;
        *) echo -e "    ${SUBTEXT0}obscura       ${RESET} ${YELLOW}skipped (unsupported arch '$ARCH')${RESET}"; return 0 ;;
    esac
    case "$OS" in
        linux)  OBS_OS="linux" ;;
        darwin) OBS_OS="macos" ;;
        *)      echo -e "    ${SUBTEXT0}obscura       ${RESET} ${YELLOW}skipped (unsupported OS '$OS')${RESET}"; return 0 ;;
    esac

    local VARIANT="${OBSCURA_VARIANT:-stealth}"
    local OBS_TAG
    OBS_TAG=$(curl -s "https://api.github.com/repos/h4ckf0r0day/obscura/releases/latest" \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)
    if [ -z "$OBS_TAG" ]; then
        echo -e "    ${SUBTEXT0}obscura       ${RESET} ${YELLOW}skipped (release lookup failed)${RESET}"
        return 0
    fi

    ASSET="obscura-${OBS_ARCH}-${OBS_OS}-${VARIANT}.tar.gz"
    FILE_URL="https://github.com/h4ckf0r0day/obscura/releases/download/${OBS_TAG}/${ASSET}"
    if ! curl -fsSL "$FILE_URL" -o "$TMP_DIR/$ASSET"; then
        echo -e "    ${SUBTEXT0}obscura       ${RESET} ${YELLOW}skipped (using Firefox/Chrome CDP fallback)${RESET}"
        return 0
    fi

    mkdir -p "$TMP_DIR/obscura_extract"
    tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR/obscura_extract"
    local BIN_PATH
    BIN_PATH=$(find "$TMP_DIR/obscura_extract" -type f -name 'obscura' | head -1)
    if [ -z "$BIN_PATH" ]; then
        echo -e "    ${SUBTEXT0}obscura       ${RESET} ${YELLOW}skipped (binary not found in archive)${RESET}"
        return 0
    fi
    mv "$BIN_PATH" "$INSTALL_DIR/obscura"
    chmod +x "$INSTALL_DIR/obscura"
    echo -e "    ${SUBTEXT0}obscura       ${RESET} ${GREEN}✓ ${OBS_TAG} installed (${INSTALL_DIR}/obscura)${RESET}"
}

if [ "${SKIP_OBSCURA:-0}" = "1" ]; then
    echo -e "    ${SUBTEXT0}obscura       ${RESET} ${SUBTEXT0}skipped (SKIP_OBSCURA=1)${RESET}"
else
    install_obscura || true
fi

echo -e "  ${SURFACE1}────────────────────────────────────────${RESET}"

# Check PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo -e "  ${YELLOW}⚠ ${INSTALL_DIR} is not currently in your PATH.${RESET}"
        echo -e "  ${SUBTEXT0}Add to ~/.bashrc or ~/.zshrc:${RESET} ${BOLD}export PATH=\"\$PATH:$INSTALL_DIR\"${RESET}"
        ;;
esac

echo -e "  ${GREEN}${BOLD}✓ Ready!${RESET} ${SUBTEXT0}Run 'minicode' to launch the TUI, or 'minicode --help' for CLI options.${RESET}\n"
