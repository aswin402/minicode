#!/usr/bin/env bash
# ==============================================================================
# minicode — Universal Single-Script Installer (Linux, macOS, WSL & Git Bash)
# Repository: https://github.com/aswin402/minicode
#
# Quick Install (Linux / macOS / WSL / Git Bash):
#   curl -fsSL https://raw.githubusercontent.com/aswin402/minicode/main/install.sh | bash
#
# Windows PowerShell:
#   irm https://raw.githubusercontent.com/aswin402/minicode/main/install.ps1 | iex
#
# Options / Environment Variables:
#   --version <tag>       Pin a release tag (e.g. v0.5.87)  [env: MINICODE_VERSION]
#   --dir <path>          Install directory                 [env: MINICODE_INSTALL_DIR]
#   --from-source         Build from source with cargo      [env: MINICODE_FROM_SOURCE=1]
#   --no-path             Do not edit shell rc PATH files   [env: MINICODE_NO_PATH=1]
#   --skip-obscura        Skip optional Obscura helper      [env: SKIP_OBSCURA=1]
#   --dry-run             Show resolved target & URL only   [env: MINICODE_DRY_RUN=1]
# ==============================================================================

set -euo pipefail

REPO="aswin402/minicode"
INSTALL_DIR="${MINICODE_INSTALL_DIR:-$HOME/.local/bin}"
REQUESTED_VERSION="${MINICODE_VERSION:-}"
FROM_SOURCE="${MINICODE_FROM_SOURCE:-0}"
NO_PATH="${MINICODE_NO_PATH:-0}"
SKIP_OBSCURA="${SKIP_OBSCURA:-0}"
DRY_RUN="${MINICODE_DRY_RUN:-0}"

while [ $# -gt 0 ]; do
    case "$1" in
        --version|-v)
            REQUESTED_VERSION="${2:-}"
            shift 2
            ;;
        --dir|-d)
            INSTALL_DIR="${2:-}"
            shift 2
            ;;
        --from-source)
            FROM_SOURCE="1"
            shift
            ;;
        --no-path)
            NO_PATH="1"
            shift
            ;;
        --skip-obscura)
            SKIP_OBSCURA="1"
            shift
            ;;
        --dry-run)
            DRY_RUN="1"
            shift
            ;;
        --help|-h)
            sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "Unknown option: $1 (try --help)" >&2
            exit 1
            ;;
    esac
done

# ── minicode Official Aura Dark Palette (from src/ui/theme.rs) ────────────────
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    BOLD="\033[1m"
    RESET="\033[0m"
    PURPLE="\033[38;2;162;119;255m"   # #a277ff (Aura Purple)
    GREEN="\033[38;2;97;255;202m"     # #61ffca (Aura Mint)
    CYAN="\033[38;2;130;226;255m"     # #82e2ff (Aura Cyan)
    YELLOW="\033[38;2;255;202;133m"   # #ffca85 (Aura Warm Orange)
    RED="\033[38;2;255;103;103m"      # #ff6767 (Aura Coral Red)
    TEXT="\033[38;2;237;236;238m"     # #edecee (Aura Bright)
    MUTED="\033[38;2;138;138;147m"    # #8a8a93 (Aura Comment)
    BORDER="\033[38;2;61;55;94m"      # #3d375e (Aura Line)
    RAMP_0="\033[38;2;162;119;255m"
    RAMP_1="\033[38;2;195;130;255m"
    RAMP_2="\033[38;2;246;148;255m"
    RAMP_3="\033[38;2;188;187;255m"
    RAMP_4="\033[38;2;130;226;255m"
    RAMP_5="\033[38;2;113;240;228m"
    RAMP_6="\033[38;2;97;255;202m"
else
    BOLD="" RESET="" PURPLE="" GREEN="" CYAN="" YELLOW="" RED="" TEXT="" MUTED="" BORDER=""
    RAMP_0="" RAMP_1="" RAMP_2="" RAMP_3="" RAMP_4="" RAMP_5="" RAMP_6=""
fi

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

printf "%b" "${PURPLE}${BOLD}"
cat <<'BANNER'
   /\_/\     ___ ___                           _
  ( o.o )   |   Y   | _   ___  _   ___  ___  _| | ___
   > ^ <    |.      || | |   || | |  _|| . || . || -_|
            |. \_/  ||_| |_|_||_| |___||___||___||___|
BANNER
printf "%b\n" "${RESET}"
printf "  %b⚡ Installing minicode (%s)%b\n" "${CYAN}" "${REPO}" "${RESET}"
printf "  %b────────────────────────────────────────────────────%b\n" "${BORDER}" "${RESET}"

START_SEC=$(date +%s)

# ── Detect OS & Architecture ──────────────────────────────────────────────────
RAW_OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
ARCHIVE_EXT="tar.gz"
BIN_EXT=""

case "$RAW_OS" in
    linux*)
        OS="linux"
        case "$ARCH" in
            x86_64|amd64)
                TARGET="x86_64-unknown-linux-gnu"
                ;;
            aarch64|arm64)
                TARGET="aarch64-unknown-linux-gnu"
                ;;
            *)
                printf "  %b✗ Unsupported architecture: %s on Linux%b\n" "${RED}${BOLD}" "$ARCH" "${RESET}" >&2
                exit 1
                ;;
        esac
        ;;
    darwin*)
        OS="darwin"
        # Prefer native Apple Silicon binary when invoked under Rosetta 2
        if [ "$ARCH" = "x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
            ARCH="arm64"
        fi
        case "$ARCH" in
            x86_64|amd64)
                TARGET="x86_64-apple-darwin"
                ;;
            arm64|aarch64)
                TARGET="aarch64-apple-darwin"
                ;;
            *)
                printf "  %b✗ Unsupported architecture: %s on macOS%b\n" "${RED}${BOLD}" "$ARCH" "${RESET}" >&2
                exit 1
                ;;
        esac
        ;;
    mingw*|msys*|cygwin*)
        OS="windows"
        TARGET="x86_64-pc-windows-msvc"
        ARCHIVE_EXT="zip"
        BIN_EXT=".exe"
        ;;
    *)
        printf "  %b✗ Unsupported operating system: %s%b\n" "${RED}${BOLD}" "$RAW_OS" "${RESET}" >&2
        printf "  %b  Tip: On Windows PowerShell run: irm https://raw.githubusercontent.com/%s/main/install.ps1 | iex%b\n" "${MUTED}" "$REPO" "${RESET}" >&2
        exit 1
        ;;
esac

printf "    %btarget        %b %b%s%b\n" "${MUTED}" "${RESET}" "${CYAN}" "${TARGET}" "${RESET}"
printf "    %binstall dir   %b %b%s%b\n" "${MUTED}" "${RESET}" "${TEXT}" "${INSTALL_DIR}" "${RESET}"

build_from_source() {
    if ! command -v cargo >/dev/null 2>&1; then
        printf "  %b✗ Prebuilt binary unavailable and 'cargo' is not installed.%b\n" "${RED}${BOLD}" "${RESET}" >&2
        printf "    %bInstall Rust first:%b https://rustup.rs\n" "${MUTED}" "${RESET}" >&2
        printf "    %bOr check release builds:%b https://github.com/%s/releases\n" "${MUTED}" "${RESET}" "$REPO" >&2
        exit 1
    fi
    printf "  %b⚙ Building minicode from source via cargo...%b\n" "${YELLOW}" "${RESET}"
    cargo install --git "https://github.com/$REPO.git" --locked minicode
    local end_sec elapsed
    end_sec=$(date +%s)
    elapsed=$(( end_sec - START_SEC ))
    printf "  %b✓ minicode built & installed via cargo in %ss%b\n" "${GREEN}${BOLD}" "$elapsed" "${RESET}"
    exit 0
}

if [ "$FROM_SOURCE" = "1" ]; then
    if [ "$DRY_RUN" = "1" ]; then
        printf "    %bmode          %b %bcargo --from-source (dry-run)%b\n" "${MUTED}" "${RESET}" "${YELLOW}" "${RESET}"
        exit 0
    fi
    build_from_source
fi

# ── Resolve Release Tag (Redirect-first to avoid GitHub API 60/hr rate limit) ─
resolve_latest_tag() {
    local effective_url tag
    effective_url=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest" 2>/dev/null || true)
    tag="${effective_url##*/}"
    if [ -n "$tag" ] && [ "$tag" != "latest" ] && [ "$tag" != "releases" ]; then
        printf "%s" "$tag"
        return 0
    fi
    # Fallback to GitHub Releases REST API
    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
        | grep -m1 '"tag_name":' \
        | sed -E 's/.*"([^"]+)".*/\1/' || true
}

if [ -n "$REQUESTED_VERSION" ]; then
    case "$REQUESTED_VERSION" in
        v*) LATEST_TAG="$REQUESTED_VERSION" ;;
        *)  LATEST_TAG="v$REQUESTED_VERSION" ;;
    esac
else
    if [ "$DRY_RUN" = "1" ]; then
        LATEST_TAG=$(resolve_latest_tag)
        [ -z "$LATEST_TAG" ] && LATEST_TAG="latest"
    else
        LATEST_TAG=$(resolve_latest_tag)
    fi
fi

if [ -z "$LATEST_TAG" ]; then
    printf "  %b⚠ Could not resolve latest release tag — falling back to cargo source build...%b\n" "${YELLOW}" "${RESET}"
    build_from_source
fi

ASSET_NAME="minicode-${TARGET}.${ARCHIVE_EXT}"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${LATEST_TAG}/${ASSET_NAME}"
SHA_URL="${DOWNLOAD_URL}.sha256"

printf "    %brelease       %b %b%s%b\n" "${MUTED}" "${RESET}" "${PURPLE}${BOLD}" "${LATEST_TAG}" "${RESET}"

if [ "$DRY_RUN" = "1" ]; then
    printf "    %basset         %b %b%s%b\n" "${MUTED}" "${RESET}" "${CYAN}" "${ASSET_NAME}" "${RESET}"
    printf "    %burl           %b %b%s%b\n" "${MUTED}" "${RESET}" "${MUTED}" "${DOWNLOAD_URL}" "${RESET}"
    printf "  %b✓ Dry run complete (no files modified).%b\n" "${GREEN}${BOLD}" "${RESET}"
    exit 0
fi

if ! command -v curl >/dev/null 2>&1; then
    printf "  %b✗ 'curl' is required to download minicode.%b\n" "${RED}${BOLD}" "${RESET}" >&2
    exit 1
fi

BAR_40=$(render_progress_bar 40)
printf "  %bDownload     %b %b  %b 40%%%b %b(%s)%b\n" "${MUTED}" "${RESET}" "$BAR_40" "${TEXT}${BOLD}" "${RESET}" "${MUTED}" "${ASSET_NAME}" "${RESET}"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$ASSET_NAME"; then
    printf "  %b⚠ Prebuilt archive not yet published for %s (%s).%b\n" "${YELLOW}" "${TARGET}" "${LATEST_TAG}" "${RESET}"
    build_from_source
fi

# ── Verify SHA-256 checksum if published ──────────────────────────────────────
verify_sha256() {
    local file="$1"
    local sha_file="$2"
    local expected actual
    expected=$(awk '{print $1}' "$sha_file" | tr '[:upper:]' '[:lower:]' | head -n 1)
    if [ -z "$expected" ] || [ "${#expected}" -ne 64 ]; then
        return 0
    fi
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$file" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')
    else
        return 0
    fi
    if [ "$expected" != "$actual" ]; then
        printf "  %b✗ SHA-256 checksum mismatch!%b\n" "${RED}${BOLD}" "${RESET}" >&2
        printf "    expected: %s\n    actual:   %s\n" "$expected" "$actual" >&2
        exit 1
    fi
    printf "    %bchecksum      %b %b✓ sha256 verified%b\n" "${MUTED}" "${RESET}" "${GREEN}" "${RESET}"
}

if curl -fsSL "$SHA_URL" -o "$TMP_DIR/${ASSET_NAME}.sha256" 2>/dev/null; then
    verify_sha256 "$TMP_DIR/$ASSET_NAME" "$TMP_DIR/${ASSET_NAME}.sha256"
fi

# ── Extract & Install Binaries ────────────────────────────────────────────────
mkdir -p "$TMP_DIR/extract" "$INSTALL_DIR"
if [ "$ARCHIVE_EXT" = "zip" ]; then
    if command -v unzip >/dev/null 2>&1; then
        unzip -qo "$TMP_DIR/$ASSET_NAME" -d "$TMP_DIR/extract"
    else
        printf "  %b✗ 'unzip' is required to extract %s.%b\n" "${RED}${BOLD}" "$ASSET_NAME" "${RESET}" >&2
        exit 1
    fi
else
    tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR/extract"
fi

INSTALLED_BINS=()
for bin_name in minicode minidoctor minibrowse; do
    full_name="${bin_name}${BIN_EXT}"
    found_path=$(find "$TMP_DIR/extract" -type f -name "$full_name" | head -n 1 || true)
    if [ -n "$found_path" ]; then
        mv -f "$found_path" "$INSTALL_DIR/$full_name"
        chmod +x "$INSTALL_DIR/$full_name" 2>/dev/null || true
        INSTALLED_BINS+=("$full_name")
    fi
done

if [ "${#INSTALLED_BINS[@]}" -eq 0 ]; then
    printf "  %b✗ Archive did not contain a minicode binary.%b\n" "${RED}${BOLD}" "${RESET}" >&2
    exit 1
fi

BAR_100=$(render_progress_bar 100)
printf "  %bInstall      %b %b  %b100%%%b\n\n" "${MUTED}" "${RESET}" "$BAR_100" "${TEXT}${BOLD}" "${RESET}"

END_SEC=$(date +%s)
ELAPSED=$(( END_SEC - START_SEC ))
BIN_SIZE=$(ls -lh "$INSTALL_DIR/minicode${BIN_EXT}" | awk '{print $5}')

printf "  %b✓ minicode %s installed in %ss%b\n" "${GREEN}${BOLD}" "${LATEST_TAG}" "${ELAPSED}" "${RESET}"
printf "    %bbinaries      %b %b%s%b\n" "${MUTED}" "${RESET}" "${GREEN}" "${INSTALLED_BINS[*]}" "${RESET}"
printf "    %bbinary size   %b %b%s%b\n" "${MUTED}" "${RESET}" "${CYAN}" "${BIN_SIZE}" "${RESET}"
printf "    %binstalled to  %b %b%s%b\n" "${MUTED}" "${RESET}" "${GREEN}" "${INSTALL_DIR}" "${RESET}"

# ── Optional: Obscura headless engine (Linux / macOS) ─────────────────────────
install_obscura() {
    if [ "$OS" = "windows" ]; then
        return 0
    fi
    if command -v obscura >/dev/null 2>&1; then
        printf "    %bobscura       %b %balready installed (%s)%b\n" "${MUTED}" "${RESET}" "${GREEN}" "$(command -v obscura)" "${RESET}"
        return 0
    fi

    local obs_arch obs_os asset file_url obs_tag bin_path
    case "$ARCH" in
        x86_64|amd64)    obs_arch="x86_64" ;;
        aarch64|arm64)   obs_arch="aarch64" ;;
        *) return 0 ;;
    esac
    case "$OS" in
        linux)  obs_os="linux" ;;
        darwin) obs_os="macos" ;;
        *) return 0 ;;
    esac

    local variant="${OBSCURA_VARIANT:-stealth}"
    local eff
    eff=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/h4ckf0r0day/obscura/releases/latest" 2>/dev/null || true)
    obs_tag="${eff##*/}"
    if [ -z "$obs_tag" ] || [ "$obs_tag" = "latest" ] || [ "$obs_tag" = "releases" ]; then
        printf "    %bobscura       %b %bskipped (using MiniBrowse + Chrome/Firefox CDP)%b\n" "${MUTED}" "${RESET}" "${MUTED}" "${RESET}"
        return 0
    fi

    asset="obscura-${obs_arch}-${obs_os}-${variant}.tar.gz"
    file_url="https://github.com/h4ckf0r0day/obscura/releases/download/${obs_tag}/${asset}"
    if ! curl -fsSL "$file_url" -o "$TMP_DIR/$asset" 2>/dev/null; then
        printf "    %bobscura       %b %bskipped (using MiniBrowse + Chrome/Firefox CDP)%b\n" "${MUTED}" "${RESET}" "${MUTED}" "${RESET}"
        return 0
    fi

    mkdir -p "$TMP_DIR/obscura_extract"
    tar -xzf "$TMP_DIR/$asset" -C "$TMP_DIR/obscura_extract" 2>/dev/null || return 0
    bin_path=$(find "$TMP_DIR/obscura_extract" -type f -name 'obscura' | head -n 1 || true)
    if [ -z "$bin_path" ]; then
        return 0
    fi
    mv -f "$bin_path" "$INSTALL_DIR/obscura"
    chmod +x "$INSTALL_DIR/obscura"
    printf "    %bobscura       %b %b✓ %s installed (%s/obscura)%b\n" "${MUTED}" "${RESET}" "${GREEN}" "${obs_tag}" "${INSTALL_DIR}" "${RESET}"
}

if [ "$SKIP_OBSCURA" = "1" ]; then
    printf "    %bobscura       %b %bskipped (SKIP_OBSCURA=1)%b\n" "${MUTED}" "${RESET}" "${MUTED}" "${RESET}"
else
    install_obscura || true
fi

# ── Ensure INSTALL_DIR is on PATH (idempotent) ────────────────────────────────
ensure_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) return 0 ;;
    esac

    if [ "$NO_PATH" = "1" ]; then
        printf "  %b⚠ %s is not in PATH. Add manually:%b export PATH=\"%s:\$PATH\"\n" "${YELLOW}" "$INSTALL_DIR" "${RESET}" "$INSTALL_DIR"
        return 0
    fi

    local shell_name rc_file line updated=0
    shell_name="$(basename "${SHELL:-sh}")"
    case "$shell_name" in
        zsh)  rc_file="$HOME/.zshrc" ;;
        fish) rc_file="$HOME/.config/fish/config.fish" ;;
        bash)
            if [ "$OS" = "darwin" ] && [ -f "$HOME/.bash_profile" ]; then
                rc_file="$HOME/.bash_profile"
            else
                rc_file="$HOME/.bashrc"
            fi
            ;;
        *)    rc_file="$HOME/.profile" ;;
    esac

    if [ "$shell_name" = "fish" ]; then
        line="fish_add_path \"$INSTALL_DIR\""
    else
        line="export PATH=\"$INSTALL_DIR:\$PATH\""
    fi

    mkdir -p "$(dirname "$rc_file")"
    if [ ! -f "$rc_file" ] || ! grep -Fq "$INSTALL_DIR" "$rc_file" 2>/dev/null; then
        printf "\n# Added by minicode installer\n%s\n" "$line" >> "$rc_file"
        updated=1
    fi

    if [ "$updated" = "1" ]; then
        printf "    %bshell PATH    %b %b✓ added to %s%b\n" "${MUTED}" "${RESET}" "${GREEN}" "$rc_file" "${RESET}"
        printf "  %b↻ Restart your shell or run:%b %bsource %s%b\n" "${CYAN}" "${RESET}" "${BOLD}" "$rc_file" "${RESET}"
    else
        printf "  %b⚠ %s is not in your current PATH (restart your terminal or run: export PATH=\"%s:\$PATH\")%b\n" "${YELLOW}" "$INSTALL_DIR" "$INSTALL_DIR" "${RESET}"
    fi
}

ensure_path

printf "  %b────────────────────────────────────────────────────%b\n" "${BORDER}" "${RESET}"
if VERSION_OUT=$("$INSTALL_DIR/minicode${BIN_EXT}" --version 2>/dev/null); then
    printf "    %bverified      %b %b%s%b\n" "${MUTED}" "${RESET}" "${CYAN}" "$VERSION_OUT" "${RESET}"
fi
printf "  %b✨ Next steps:%b\n" "${GREEN}${BOLD}" "${RESET}"
printf "     %b1.%b Run %bminicode setup%b   — configure your LLM provider & API key\n" "${PURPLE}" "${RESET}" "${BOLD}" "${RESET}"
printf "     %b2.%b Run %bminicode%b         — launch the interactive TUI\n" "${PURPLE}" "${RESET}" "${BOLD}" "${RESET}"
printf "     %b3.%b Run %bminicode doctor%b  — audit codebase health (writes .minicode/minidoctor/NEEDEDFIX.md)\n\n" "${PURPLE}" "${RESET}" "${BOLD}" "${RESET}"
