#!/usr/bin/env bash
# ==============================================================================
# minicode — Local Global Updater Script (Aura Dark + cargo-pretty UI)
#
# Upgrades minicode globally with:
# 1. Automatic safety backup of existing user configs (~/.config/minicode)
# 2. Non-destructive update preserving all custom keys, models, and themes
# 3. Resource-constrained compilation (nice -n 19) with Aura Dark cargo-pretty UI
# ==============================================================================

set -euo pipefail

# ── minicode Official Aura Dark Palette (from src/ui/theme.rs) ────────────────
BOLD="\033[1m"
DIM="\033[2m"
RESET="\033[0m"

PURPLE="\033[38;2;162;119;255m"   # #a277ff (Aura Purple — brand_accent)
GREEN="\033[38;2;97;255;202m"     # #61ffca (Aura Mint Green — success)
CYAN="\033[38;2;130;226;255m"     # #82e2ff (Aura Cyan — info)
YELLOW="\033[38;2;255;202;133m"   # #ffca85 (Aura Warm Orange — warning)
RED="\033[38;2;255;103;103m"      # #ff6767 (Aura Coral Red — destructive)
PINK="\033[38;2;246;148;255m"     # #f694ff (Aura Pink — highlight)
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

# 6-row age-fading palettes in Aura Dark tones
YLW_FADE=(
    "\033[38;2;255;202;133m"
    "\033[38;2;216;174;130m"
    "\033[38;2;177;146;126m"
    "\033[38;2;138;118;120m"
    "\033[38;2;100;88;108m"
    "\033[38;2;68;62;94m"
)
GRN_FADE=(
    "\033[38;2;97;255;202m"
    "\033[38;2;88;218;180m"
    "\033[38;2;79;181;158m"
    "\033[38;2;70;144;136m"
    "\033[38;2;64;105;114m"
    "\033[38;2;61;72;94m"
)

echo -e "${PURPLE}${BOLD}"
echo "   ___ ___                           _     "
echo "  |   Y   | _   ___  _   ___  ___  _| | ___ "
echo "  |.      || | |   || | |  _|| . || . || -_|"
echo "  |. \\_/  ||_| |_|_||_| |___||___||___||___|"
echo -e "${RESET}"
echo -e "  ${CYAN}🔄  Initializing minicode Safe Global Updater...${RESET}"
echo -e "  ${BORDER}────────────────────────────────────────${RESET}"

SPINNER_FRAMES=("⠋" "⠙" "⠹" "⠸" "⠼" "⠴" "⠦" "⠧" "⠇" "⠏")
NOW_MS=0
FMT_DUR="00.00s"

get_now_ms() {
    if [ -n "${EPOCHREALTIME:-}" ]; then
        local sec="${EPOCHREALTIME%.*}"
        local frac="${EPOCHREALTIME#*.}000"
        frac="${frac:0:3}"
        NOW_MS=$(( 10#$sec * 1000 + 10#$frac ))
    else
        NOW_MS=$(date +%s%3N 2>/dev/null || echo "$(( $(date +%s) * 1000 ))")
    fi
}

format_dur_ms() {
    local ms="${1:-0}"
    if [ "$ms" -lt 0 ]; then ms=0; fi
    local secs=$(( ms / 1000 ))
    local cs=$(( (ms % 1000) / 10 ))
    printf -v FMT_DUR "%02d.%02ds" "$secs" "$cs"
}

format_bytes() {
    local bytes="${1:-0}"
    if [ "$bytes" -le 0 ]; then
        echo "0 B"
    elif [ "$bytes" -ge 1073741824 ]; then
        awk -v b="$bytes" 'BEGIN { printf "%.1f GiB", b / 1073741824 }'
    elif [ "$bytes" -ge 1048576 ]; then
        awk -v b="$bytes" 'BEGIN { printf "%.1f MiB", b / 1048576 }'
    elif [ "$bytes" -ge 1024 ]; then
        awk -v b="$bytes" 'BEGIN { printf "%.1f KiB", b / 1024 }'
    else
        echo "${bytes} B"
    fi
}

dir_size_bytes() {
    local dir="$1"
    if [ -d "$dir" ]; then
        du -sb "$dir" 2>/dev/null | awk '{print $1}' || echo 0
    else
        echo 0
    fi
}

BAR_STR=""
render_progress_bar() {
    local pct="$1"
    local tick="${2:-0}"
    local width=28
    local filled=$(( pct * width / 100 ))
    if [ "$pct" -gt 0 ] && [ "$filled" -eq 0 ]; then
        filled=1
    fi
    if [ "$filled" -gt "$width" ]; then filled="$width"; fi
    local bar=""
    local i=0
    local pulse_chars=("▓" "▒" "░" "▒")
    local pulse_char="${pulse_chars[$(( tick % 4 ))]}"
    while [ "$i" -lt "$width" ]; do
        local color="$RAMP_6"
        if [ "$i" -lt 4 ]; then color="$RAMP_0"
        elif [ "$i" -lt 8 ]; then color="$RAMP_1"
        elif [ "$i" -lt 12 ]; then color="$RAMP_2"
        elif [ "$i" -lt 16 ]; then color="$RAMP_3"
        elif [ "$i" -lt 20 ]; then color="$RAMP_4"
        elif [ "$i" -lt 24 ]; then color="$RAMP_5"
        fi

        if [ "$i" -lt "$filled" ]; then
            bar+="${color}█"
        elif [ "$i" -eq "$filled" ] && [ "$pct" -lt 100 ]; then
            bar+="${color}${pulse_char}"
        else
            bar+="${BORDER}░"
        fi
        i=$(( i + 1 ))
    done
    bar+="${RESET}"
    printf -v BAR_STR "%b" "$bar"
}

run_cargo_pretty_build() {
    local jobs="$1"
    local project_name="minicode"
    local cmd_label="cargo build --release -j ${jobs}"
    local is_tty=false
    # Disable cursor-up 12.5fps ANSI animation when invoked inside AI agent PTY harnesses
    # (e.g. Antigravity run_command sets TERM=dumb and ANTIGRAVITY_AGENT=1) so we don't flood
    # the agent's in-memory PTY transcript with 25,000+ redraw lines and spike host RAM.
    if [ -t 1 ] && [ "${TERM:-dumb}" != "dumb" ] && [ -z "${ANTIGRAVITY_AGENT:-}${ANTIGRAVITY_CLI:-}${CI:-}${CLAUDECODE:-}${CODEX_CI:-}${NO_TTY_ANIMATION:-}" ]; then
        is_tty=true
    fi

    echo ""
    echo -e "  ${PURPLE}${BOLD}${cmd_label}${RESET} ${MUTED}(nice -n 19 • Aura Dark cargo-pretty)${RESET}"
    echo -e "  ${BORDER}────────────────────────────────────────${RESET}"

    local total_crates=300
    if [ -f "$SCRIPT_DIR/Cargo.lock" ]; then
        total_crates=$(grep -c '^name = ' "$SCRIPT_DIR/Cargo.lock" 2>/dev/null || echo 300)
        if [ "$total_crates" -lt 1 ]; then total_crates=1; fi
    fi

    local log_file
    log_file=$(mktemp)
    local diag_file
    diag_file=$(mktemp)
    local fifo_file
    fifo_file=$(mktemp -u)
    mkfifo "$fifo_file"

    get_now_ms
    local start_ms="$NOW_MS"
    local last_render_ms=0
    local spin_tick=0

    local max_active="$jobs"
    if [ "$max_active" -lt 1 ]; then max_active=1; fi
    if [ "$max_active" -gt 6 ]; then max_active=6; fi

    # Dynamic viewport height: max_active rows for Compiling + 6 rows for Compiled + 6 header/bar/spacer lines
    local viewport_lines=$(( max_active + 12 ))
    if [ "$is_tty" = true ]; then
        local l=0
        while [ "$l" -lt "$viewport_lines" ]; do
            echo ""
            l=$(( l + 1 ))
        done
    fi

    local seen_crates=0
    local fresh_crates=0
    local compiled_crates=0
    local last_reported_pct=-1
    local last_fresh_crate=""
    local in_root_crate=false
    local root_ver="v0.4.2"
    local root_phase="resolving dependency graph..."
    local root_stage_start_ms="$start_ms"
    local root_stage_base_pct=35
    local root_stage_target_pct=78
    local -a active_crates=()
    local -a done_crates=()

    process_cargo_line() {
        local raw_line="$1"
        printf "%s\n" "$raw_line" >>"$log_file"
        local trimmed="${raw_line#"${raw_line%%[![:space:]]*}"}"

        if [[ "$trimmed" == Fresh\ * ]]; then
            local rest="${trimmed#Fresh }"
            local c_name="${rest%% *}"
            local c_rest="${rest#* }"
            local c_ver="${c_rest%% *}"
            local crate_info="${c_name} ${c_ver}"
            if [ "$crate_info" != "$last_fresh_crate" ]; then
                last_fresh_crate="$crate_info"
                fresh_crates=$(( fresh_crates + 1 ))
                seen_crates=$(( seen_crates + 1 ))
                if [ "$seen_crates" -gt "$total_crates" ]; then
                    total_crates="$seen_crates"
                fi
                done_crates=("${crate_info}|cached" "${done_crates[@]:0:5}")
                root_phase="verifying cached crates (${fresh_crates})"
            fi
        elif [[ "$trimmed" == Compiling\ * ]] || [[ "$trimmed" == Checking\ * ]]; then
            local rest="${trimmed#Compiling }"
            rest="${rest#Checking }"
            local c_name="${rest%% *}"
            local c_rest="${rest#* }"
            local c_ver="${c_rest%% *}"
            local crate_info="${c_name} ${c_ver}"

            if [ "${#active_crates[@]}" -ge "$max_active" ]; then
                local finished_idx=$(( ${#active_crates[@]} - 1 ))
                local finished_entry="${active_crates[$finished_idx]}"
                local fin_name="${finished_entry%|*}"
                local fin_start="${finished_entry#*|}"
                format_dur_ms $(( NOW_MS - fin_start ))
                done_crates=("${fin_name}|${FMT_DUR}" "${done_crates[@]:0:5}")
                unset 'active_crates[finished_idx]'
            fi

            if [ "$c_name" = "$project_name" ]; then
                in_root_crate=true
                root_ver="$c_ver"
                total_crates=$(( seen_crates + 2 ))
                root_stage_start_ms="$NOW_MS"
                root_stage_base_pct=35
                root_stage_target_pct=78
                root_phase="compiling ${project_name} (lib • release)"
                active_crates=("${c_name} ${c_ver} (lib)|${NOW_MS}" "${active_crates[@]:0:$(( max_active - 1 ))}")
            else
                root_phase="building dependencies (${compiled_crates} built)"
                active_crates=("${crate_info}|${NOW_MS}" "${active_crates[@]:0:$(( max_active - 1 ))}")
            fi

            compiled_crates=$(( compiled_crates + 1 ))
            seen_crates=$(( seen_crates + 1 ))
            if [ "$seen_crates" -gt "$total_crates" ]; then
                total_crates="$seen_crates"
            fi
        elif [[ "$trimmed" == Running\ \`*rustc*--crate-name\ minicode* ]]; then
            if [[ "$trimmed" == *--crate-type\ bin* ]]; then
                if [ "${#active_crates[@]}" -gt 0 ]; then
                    local finished_entry="${active_crates[0]}"
                    local fin_name="${finished_entry%|*}"
                    local fin_start="${finished_entry#*|}"
                    if [[ "$fin_name" == *"(lib)"* ]]; then
                        format_dur_ms $(( NOW_MS - fin_start ))
                        done_crates=("${fin_name}|${FMT_DUR}" "${done_crates[@]:0:5}")
                        seen_crates=$(( seen_crates + 1 ))
                        if [ "$seen_crates" -gt "$total_crates" ]; then
                            total_crates="$seen_crates"
                        fi
                    fi
                fi
                root_stage_start_ms="$NOW_MS"
                root_stage_base_pct=78
                root_stage_target_pct=99
                root_phase="compiling & linking ${project_name} (bin • release)"
                active_crates=("${project_name} ${root_ver} (bin)|${NOW_MS}")
            elif [[ "$trimmed" == *--crate-type\ lib* ]]; then
                root_phase="compiling ${project_name} (lib • opt-level=3)"
            fi
        fi
    }

    set +e
    if command -v ionice >/dev/null 2>&1; then
        CARGO_BUILD_JOBS="${jobs}" RAYON_NUM_THREADS=1 ionice -c 3 nice -n 19 cargo build --release -j "${jobs}" -v --color never >"$fifo_file" 2>&1 &
    else
        CARGO_BUILD_JOBS="${jobs}" RAYON_NUM_THREADS=1 nice -n 19 cargo build --release -j "${jobs}" -v --color never >"$fifo_file" 2>&1 &
    fi
    local cargo_pid=$!
    exec 3<>"$fifo_file"
    rm -f "$fifo_file"

    local read_timeout="0.08"
    if [ "$is_tty" != true ]; then
        read_timeout="0.40"
    fi

    while true; do
        local line=""
        local read_rc=0
        IFS= read -r -t "$read_timeout" -u 3 line || read_rc=$?

        get_now_ms
        local elapsed_ms=$(( NOW_MS - start_ms ))
        format_dur_ms "$elapsed_ms"
        local elapsed_sec="$FMT_DUR"

        if [ "$read_rc" -eq 0 ]; then
            process_cargo_line "$line"
            # Drain up to 40 buffered lines per frame so Fresh crates cascade smoothly without blocking animation
            local drained=0
            while [ "$drained" -lt 40 ] && IFS= read -r -t 0.001 -u 3 line; do
                process_cargo_line "$line"
                drained=$(( drained + 1 ))
            done
        else
            if ! kill -0 "$cargo_pid" 2>/dev/null; then
                break
            fi
        fi

        if [ $(( NOW_MS - last_render_ms )) -ge 80 ] || [ "$read_rc" -eq 0 ]; then
            last_render_ms="$NOW_MS"
            spin_tick=$(( spin_tick + 1 ))
            local spin_char="${SPINNER_FRAMES[$(( spin_tick % 10 ))]}"

            local done_count=$(( seen_crates - ${#active_crates[@]} ))
            if [ "$done_count" -lt 0 ]; then done_count=0; fi

            local pct=0
            if [ "$in_root_crate" = true ] && [ "$compiled_crates" -le 3 ]; then
                # Incremental build: smooth progress across (deps 0->35%) -> (lib 35->78%) -> (bin 78->99%)
                local stage_elapsed=$(( NOW_MS - root_stage_start_ms ))
                local span=$(( root_stage_target_pct - root_stage_base_pct ))
                # Asymptotic curve: reaches ~85% of stage span in 25s and approaches target_pct smoothly
                local bonus=$(( span * stage_elapsed / (stage_elapsed + 12000) ))
                pct=$(( root_stage_base_pct + bonus ))
            else
                if [ "$in_root_crate" != true ] && [ "$compiled_crates" -eq 0 ] && [ "$fresh_crates" -gt 0 ]; then
                    pct=$(( fresh_crates * 35 / total_crates ))
                else
                    pct=$(( done_count * 100 / total_crates ))
                fi
            fi
            if [ "$pct" -gt 99 ]; then pct=99; fi
            if [ "$seen_crates" -gt 0 ] && [ "$pct" -lt 1 ]; then pct=1; fi

            if [ "$is_tty" = true ]; then
                printf "\033[%dA" "$viewport_lines"
                printf "  ${PURPLE}${BOLD}Compiling${RESET} ${MUTED}• %s${RESET}\033[K\n" "$root_phase"
                local r=0
                while [ "$r" -lt "$max_active" ]; do
                    if [ "$r" -lt "${#active_crates[@]}" ]; then
                        local item="${active_crates[$r]}"
                        local cname="${item%|*}"
                        local cstart="${item#*|}"
                        format_dur_ms $(( NOW_MS - cstart ))
                        printf "    ${YLW_FADE[$r]}%s %-28s %8s${RESET}\033[K\n" "$spin_char" "${cname:0:28}" "$FMT_DUR"
                    elif [ "$r" -eq 0 ]; then
                        printf "    ${CYAN}%s %-28s %8s${RESET}\033[K\n" "$spin_char" "${root_phase:0:28}" "$elapsed_sec"
                    else
                        printf "\033[K\n"
                    fi
                    r=$(( r + 1 ))
                done
                printf "\033[K\n"
                printf "  ${CYAN}${BOLD}Compiled (%d/%d)${RESET}\033[K\n" "$done_count" "$total_crates"
                r=0
                while [ "$r" -lt 6 ]; do
                    if [ "$r" -lt "${#done_crates[@]}" ]; then
                        local ditem="${done_crates[$r]}"
                        local dname="${ditem%|*}"
                        local dtime="${ditem#*|}"
                        printf "    ${GRN_FADE[$r]}✓ %-28s %8s${RESET}\033[K\n" "${dname:0:28}" "$dtime"
                    else
                        printf "\033[K\n"
                    fi
                    r=$(( r + 1 ))
                done
                printf "\033[K\n"
                render_progress_bar "$pct" "$spin_tick"
                printf "  ${MUTED}Build        ${RESET} %b  ${TEXT}${BOLD}%3d%%${RESET}\033[K\n" "$BAR_STR" "$pct"
                printf "  ${MUTED}Elapsed      ${RESET} ${CYAN}%s${RESET}  ${PURPLE}%s${RESET}\033[K\n" "$elapsed_sec" "$spin_char"
            else
                local bucket=$(( pct / 15 * 15 ))
                if [ "$bucket" -gt "$last_reported_pct" ] || [ "$seen_crates" -eq 1 ]; then
                    last_reported_pct="$bucket"
                    render_progress_bar "$pct" 0
                    printf "  ${MUTED}Build        ${RESET} %b  ${TEXT}${BOLD}%3d%%${RESET} ${MUTED}(%d/%d • %s • %s)${RESET}\n" \
                        "$BAR_STR" "$pct" "$done_count" "$total_crates" "$elapsed_sec" "$root_phase"
                fi
            fi
        fi
    done

    exec 3>&-
    wait "$cargo_pid"
    local exit_code=$?
    set -e

    get_now_ms
    local end_ms="$NOW_MS"
    local total_ms=$(( end_ms - start_ms ))
    format_dur_ms "$total_ms"
    local total_sec="$FMT_DUR"

    # Move any remaining active crate into done_crates so settled view shows 100% completed list
    if [ "$exit_code" -eq 0 ] && [ "${#active_crates[@]}" -gt 0 ]; then
        local rem_entry="${active_crates[0]}"
        local rem_name="${rem_entry%|*}"
        local rem_start="${rem_entry#*|}"
        format_dur_ms $(( end_ms - rem_start ))
        done_crates=("${rem_name}|${FMT_DUR}" "${done_crates[@]:0:5}")
    fi

    if [ "$is_tty" = true ]; then
        printf "\033[%dA\033[J" "$viewport_lines"
    fi

    local libs_compiled
    libs_compiled=$( { grep -E '^[[:space:]]*(Compiling|Checking) ' "$log_file" 2>/dev/null || true; } | wc -l | awk '{print $1}' )
    local warning_count
    warning_count=$( { grep -E '^warning(\[|: )' "$log_file" 2>/dev/null | grep -v 'generated .* warning' || true; } | wc -l | awk '{print $1}' )

    local bin_bytes=0
    if [ -f "$SCRIPT_DIR/target/release/minicode" ]; then
        bin_bytes=$(stat -c %s "$SCRIPT_DIR/target/release/minicode" 2>/dev/null || echo 0)
    fi
    local disk_str
    disk_str=$(format_bytes "$bin_bytes")

    if [ "$exit_code" -eq 0 ]; then
        local final_total="$seen_crates"
        if [ "$final_total" -lt 1 ]; then final_total="$total_crates"; fi
        printf "  ${CYAN}${BOLD}Compiled (%d/%d)${RESET}\n" "$final_total" "$final_total"
        local r=0
        while [ "$r" -lt "${#done_crates[@]}" ] && [ "$r" -lt 6 ]; do
            local ditem="${done_crates[$r]}"
            local dname="${ditem%|*}"
            local dtime="${ditem#*|}"
            printf "    ${GRN_FADE[$r]}✓ %-28s %8s${RESET}\n" "${dname:0:28}" "$dtime"
            r=$(( r + 1 ))
        done
        echo ""
        render_progress_bar 100 0
        printf "  ${MUTED}Build        ${RESET} %b  ${TEXT}${BOLD}100%%${RESET}\n\n" "$BAR_STR"
        if [ "$warning_count" -gt 0 ]; then
            echo -e "  ${YELLOW}${BOLD}⚠ ${project_name} built in ${total_sec} (${warning_count} warning(s))${RESET}"
        else
            echo -e "  ${GREEN}${BOLD}✓ ${project_name} built in ${total_sec}${RESET}"
        fi
        echo -e "    ${MUTED}libs compiled ${RESET} ${CYAN}${libs_compiled} built • ${fresh_crates} cached${RESET}"
        echo -e "    ${MUTED}disk used     ${RESET} ${CYAN}${disk_str}${RESET}"
    else
        echo -e "  ${RED}${BOLD}✗ ${project_name} build failed after ${total_sec}${RESET}"
        echo -e "    ${MUTED}libs compiled ${RESET} ${CYAN}${libs_compiled}${RESET}"
        echo ""
        grep -v -E '^[[:space:]]*(Compiling|Checking|Fresh|Dirty|Running `|Downloading|Downloaded|Locking|Updating) ' "$log_file" >"$diag_file" || true
        cat "$diag_file"
        rm -f "$log_file" "$diag_file"
        return "$exit_code"
    fi

    rm -f "$log_file" "$diag_file"
    return 0
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

INSTALL_DIR="${HOME}/.local/bin"
TARGET_BIN="$INSTALL_DIR/minicode"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/minicode"
BACKUP_DIR="$CONFIG_DIR/backups"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")

# Parse options
CLEAN_MODE="debug" # default: auto-purge target/debug to keep release cache fast while saving 3+ GB
BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

while [ $# -gt 0 ]; do
    case "$1" in
        --clean|-c)
            CLEAN_MODE="all"
            shift
            ;;
        --no-clean)
            CLEAN_MODE="none"
            shift
            ;;
        -j|--jobs)
            BUILD_JOBS="$2"
            shift 2
            ;;
        -j*)
            BUILD_JOBS="${1#-j}"
            shift
            ;;
        --jobs=*)
            BUILD_JOBS="${1#*=}"
            shift
            ;;
        -h|--help)
            echo "Usage: ./localupdate.sh [OPTIONS]"
            echo "Options:"
            echo "  -c, --clean     Purge all build artifacts (cargo clean) after installing to maximize free disk space"
            echo "  --no-clean      Keep all build caches intact (skip automatic target/debug cleanup)"
            echo "  -j, --jobs N    Number of parallel cargo build jobs (default: 1, run via nice -n 19 to prevent laptop lag)"
            echo "  -h, --help      Display this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Run './localupdate.sh --help' for usage."
            exit 1
            ;;
    esac
done

# 1. Safety Backup of Existing Configuration
mkdir -p "$BACKUP_DIR"
BACKED_UP=0

for cfg_file in "config.toml" ".env" "mcp.json"; do
    if [ -f "$CONFIG_DIR/$cfg_file" ]; then
        cp "$CONFIG_DIR/$cfg_file" "$BACKUP_DIR/${cfg_file}.bak_${TIMESTAMP}"
        BACKED_UP=$(( BACKED_UP + 1 ))
    fi
done

if [ "$BACKED_UP" -gt 0 ]; then
    echo -e "  ${GREEN}✔ Safety backup${RESET}  ${MUTED}${BACKED_UP} file(s) snapshotted to ${BACKUP_DIR}${RESET}"
else
    echo -e "  ${CYAN}ℹ Safety backup${RESET}  ${MUTED}Fresh environment (${CONFIG_DIR})${RESET}"
fi

# Clean up older backups, keeping only the 10 most recent
if [ -d "$BACKUP_DIR" ]; then
    ls -1t "$BACKUP_DIR"/config.toml.bak_* 2>/dev/null | tail -n +11 | xargs -r rm -f || true
    ls -1t "$BACKUP_DIR"/.env.bak_* 2>/dev/null | tail -n +11 | xargs -r rm -f || true
    ls -1t "$BACKUP_DIR"/mcp.json.bak_* 2>/dev/null | tail -n +11 | xargs -r rm -f || true
fi

# 2. Resource-Constrained Recompilation with Aura Dark cargo-pretty live UI (nice -n 19)
run_cargo_pretty_build "$BUILD_JOBS"

SOURCE_BIN="$SCRIPT_DIR/target/release/minicode"

if [ ! -f "$SOURCE_BIN" ]; then
    echo -e "  ${RED}${BOLD}✗ Build failed: $SOURCE_BIN not found.${RESET}"
    exit 1
fi

# 3. Atomically Replace Installed Binary & Seed Local Stack Database
mkdir -p "$INSTALL_DIR"
install -m 755 "$SOURCE_BIN" "$TARGET_BIN"
BIN_SIZE=$(ls -lh "$TARGET_BIN" | awk '{print $5}')
echo -e "    ${MUTED}binary size   ${RESET} ${CYAN}${BIN_SIZE}${RESET}"
echo -e "    ${MUTED}installed to  ${RESET} ${GREEN}${TARGET_BIN}${RESET}"

"$TARGET_BIN" stack list --json >/dev/null 2>&1 || true
STACK_DB_COUNT=$(ls -1 "$CONFIG_DIR/stacks/"*.json 2>/dev/null | wc -l || echo 0)
echo -e "    ${MUTED}stack db      ${RESET} ${GREEN}${STACK_DB_COUNT} templates (${CONFIG_DIR}/stacks)${RESET}"

# 4. Storage Optimization & Cache Cleanup
if [ "$CLEAN_MODE" = "all" ]; then
    cargo clean 2>/dev/null || rm -rf "$SCRIPT_DIR/target"
    echo -e "    ${MUTED}target cache  ${RESET} ${GREEN}0 B (100% reclaimed via --clean)${RESET}"
elif [ "$CLEAN_MODE" = "debug" ]; then
    if [ -d "$SCRIPT_DIR/target/debug" ]; then
        rm -rf "$SCRIPT_DIR/target/debug"
    fi
    # Also purge any large standalone test binaries in target/release/deps/minicode-* (keeps .rlib/.d intact)
    find "$SCRIPT_DIR/target/release/deps" -maxdepth 1 -type f -name 'minicode-*' -executable -delete 2>/dev/null || true
    echo -e "    ${MUTED}target cache  ${RESET} ${CYAN}release cached (debug & test binaries purged)${RESET}"
else
    echo -e "    ${MUTED}target cache  ${RESET} ${CYAN}preserved${RESET}"
fi

# 5. Verify Installed Version and Configuration Integrity
VER_STR=$("$TARGET_BIN" --version 2>/dev/null | grep -m1 'minicode v' | sed -E 's/^[•[:space:]]*//' || echo "minicode")
echo -e "    ${MUTED}version       ${RESET} ${PURPLE}${BOLD}${VER_STR}${RESET}"
echo -e "  ${BORDER}────────────────────────────────────────${RESET}"
echo -e "  ${GREEN}${BOLD}✨ minicode is up to date and ready to use globally!${RESET}\n"
