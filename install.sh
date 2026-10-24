#!/usr/bin/env bash
# ==============================================================================
# minicode — Remote & Source Installer Script (Aura Dark + cargo-pretty UI)
# Repository: https://github.com/aswin402/minicode
# ==============================================================================

set -euo pipefail

REPO="aswin402/minicode"
INSTALL_DIR="${MINICODE_INSTALL_DIR:-$HOME/.local/bin}"

# ── minicode Official Aura Dark Palette (from src/ui/theme.rs) ────────────────
BOLD="\033[1m"
RESET="\033[0m"

PURPLE="\033[38;2;162;119;255m"   # #a277ff (Aura Purple — brand_accent)
GREEN="\033[38;2;97;255;202m"     # #61ffca (Aura Mint Green — success)
CYAN="\033[38;2;130;226;255m"     # #82e2ff (Aura Cyan — info)
YELLOW="\033[38;2;255;202;133m"   # #ffca85 (Aura Warm Orange — warning)
RED="\033[38;2;255;103;103m"      # #ff6767 (Aura Coral Red — destructive)
TEXT="\033[38;2;237;236;238m"     # #edecee (Aura Bright — text_primary)
MUTED="\033[38;2;138;138;147m"    # #8a8a93 (Aura Comment — muted)
BORDER="\033[38;2;61;55;94m"      # #3d375e (Aura Line — border)

# Aura Progress Bar Gradient (Purple #a277ff -> Pink #f694ff -> Cyan #82e2ff -> Mint #61ffca)
RAMP_0="\033[38;2;162;119;255m"
RAMP_1="\033[38;2;195;130;255m"
RAMP_2="\033[38;2;246;148;255m"
RAMP_3="\033[38;2;188;187;255m"
RAMP_4="\033[38;2;130;226;255m"
RAMP_5="\033[38;2;113;240;228m"
RAMP_6="\033[38;2;97;255;202m"

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
            bar+="${BORDER}░"
        fi
        i=$(( i + 1 ))
    done
    bar+="${RESET}"
    printf "%b" "$bar"
}

echo -e "${PURPLE}${BOLD}"
echo "   ___ ___                           _     "
echo "  |   Y   | _   ___  _   ___  ___  _| | ___ "
echo "  |.      || | |   || | |  _|| . || . || -_|"
echo "  |. \\_/  ||_| |_|_||_| |___||___||___||___|"
echo -e "${RESET}"
echo -e "  ${CYAN}⚡ Installing minicode (${REPO})...${RESET}"
echo -e "  ${BORDER}────────────────────────────────────────${RESET}"

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

echo -e "    ${MUTED}target        ${RESET} ${CYAN}${TARGET}${RESET}"

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

echo -e "    ${MUTED}release       ${RESET} ${PURPLE}${BOLD}${LATEST_TAG}${RESET}"
BAR_50=$(render_progress_bar 50)
printf "  ${MUTED}Download     ${RESET} %b  ${TEXT}${BOLD} 50%%${RESET} ${MUTED}(${ASSET_NAME})${RESET}\n" "$BAR_50"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_NAME"
tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR"

mkdir -p "$INSTALL_DIR"
mv "$TMP_DIR/minicode" "$INSTALL_DIR/minicode"
chmod +x "$INSTALL_DIR/minicode"

BAR_100=$(render_progress_bar 100)
printf "  ${MUTED}Install      ${RESET} %b  ${TEXT}${BOLD}100%%${RESET}\n\n" "$BAR_100"

END_SEC=$(date +%s)
ELAPSED=$(( END_SEC - START_SEC ))
BIN_SIZE=$(ls -lh "$INSTALL_DIR/minicode" | awk '{print $5}')

echo -e "  ${GREEN}${BOLD}✓ minicode ${LATEST_TAG} installed in ${ELAPSED}s${RESET}"
echo -e "    ${MUTED}binary size   ${RESET} ${CYAN}${BIN_SIZE}${RESET}"
echo -e "    ${MUTED}installed to  ${RESET} ${GREEN}${INSTALL_DIR}/minicode${RESET}"

# ── Optional: Obscura headless engine ────────────────────────────────────────
install_obscura() {
    if command -v obscura >/dev/null 2>&1; then
        echo -e "    ${MUTED}obscura       ${RESET} ${GREEN}already installed ($(command -v obscura))${RESET}"
        return 0
    fi

    local OBS_ARCH OBS_OS ASSET FILE_URL
    case "$ARCH" in
        x86_64)          OBS_ARCH="x86_64" ;;
        aarch64|arm64)   OBS_ARCH="aarch64" ;;
        *) echo -e "    ${MUTED}obscura       ${RESET} ${YELLOW}skipped (unsupported arch '$ARCH')${RESET}"; return 0 ;;
    esac
    case "$OS" in
        linux)  OBS_OS="linux" ;;
        darwin) OBS_OS="macos" ;;
        *)      echo -e "    ${MUTED}obscura       ${RESET} ${YELLOW}skipped (unsupported OS '$OS')${RESET}"; return 0 ;;
    esac

    local VARIANT="${OBSCURA_VARIANT:-stealth}"
    local OBS_TAG
    OBS_TAG=$(curl -s "https://api.github.com/repos/h4ckf0r0day/obscura/releases/latest" \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)
    if [ -z "$OBS_TAG" ]; then
        echo -e "    ${MUTED}obscura       ${RESET} ${YELLOW}skipped (release lookup failed)${RESET}"
        return 0
    fi

    ASSET="obscura-${OBS_ARCH}-${OBS_OS}-${VARIANT}.tar.gz"
    FILE_URL="https://github.com/h4ckf0r0day/obscura/releases/download/${OBS_TAG}/${ASSET}"
    if ! curl -fsSL "$FILE_URL" -o "$TMP_DIR/$ASSET"; then
        echo -e "    ${MUTED}obscura       ${RESET} ${YELLOW}skipped (using Firefox/Chrome CDP fallback)${RESET}"
        return 0
    fi

    mkdir -p "$TMP_DIR/obscura_extract"
    tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR/obscura_extract"
    local BIN_PATH
    BIN_PATH=$(find "$TMP_DIR/obscura_extract" -type f -name 'obscura' | head -1)
    if [ -z "$BIN_PATH" ]; then
        echo -e "    ${MUTED}obscura       ${RESET} ${YELLOW}skipped (binary not found in archive)${RESET}"
        return 0
    fi
    mv "$BIN_PATH" "$INSTALL_DIR/obscura"
    chmod +x "$INSTALL_DIR/obscura"
    echo -e "    ${MUTED}obscura       ${RESET} ${GREEN}✓ ${OBS_TAG} installed (${INSTALL_DIR}/obscura)${RESET}"
}

if [ "${SKIP_OBSCURA:-0}" = "1" ]; then
    echo -e "    ${MUTED}obscura       ${RESET} ${MUTED}skipped (SKIP_OBSCURA=1)${RESET}"
else
    install_obscura || true
fi

echo -e "  ${BORDER}────────────────────────────────────────${RESET}"

# Check PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo -e "  ${YELLOW}⚠ ${INSTALL_DIR} is not currently in your PATH.${RESET}"
        echo -e "  ${MUTED}Add to ~/.bashrc or ~/.zshrc:${RESET} ${BOLD}export PATH=\"\$PATH:$INSTALL_DIR\"${RESET}"
        ;;
esac

echo -e "  ${GREEN}${BOLD}✨ Run 'minicode' to launch the TUI, or 'minicode --help' for CLI options.${RESET}\n"
