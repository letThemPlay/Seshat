#!/usr/bin/env bash
# Runs a UI test script against Seshat inside an isolated, headless Sway
# compositor.
#
#   scripts/ui-session.sh scripts/ui/file-operations.sh [artifact-dir]
#
# Isolation: the compositor, Seshat and all simulated input share a private
# XDG_RUNTIME_DIR and a scrubbed environment (no DISPLAY, no session D-Bus,
# no access to the real Wayland session). HOME, XDG_CONFIG_HOME and
# XDG_DATA_HOME (and so the Trash) point into a throwaway directory.
# Simulated input (wtype) can only reach this compositor; every helper
# re-checks that before sending anything.
#
# (Weston has no way to inject input, so the UI tests use Sway's headless
# backend; scripts/verify-wayland.sh keeps using Weston.)
#
# Run inside `nix develop`.
set -euo pipefail

DRIVER=${1:?usage: $0 <ui-test-script> [artifact-dir]}
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cargo build -q -p seshat --manifest-path "$ROOT/Cargo.toml"
BIN="$ROOT/target/debug/seshat"

SESSION="$(mktemp -d -t seshat-ui-XXXXXX)"
ARTIFACTS="${2:-$SESSION/artifacts}"
mkdir -p "$SESSION/runtime" "$SESSION/home" "$SESSION/config" "$SESSION/data" "$ARTIFACTS"
chmod 700 "$SESSION/runtime"

cat > "$SESSION/sway.conf" <<'EOF'
output HEADLESS-1 resolution 1280x800 scale 1
default_border none
focus_follows_mouse no
EOF

clean_env=(
    env -i
    PATH="$PATH"
    HOME="$SESSION/home"
    XDG_RUNTIME_DIR="$SESSION/runtime"
    XDG_CONFIG_HOME="$SESSION/config"
    XDG_DATA_HOME="$SESSION/data"
    XDG_DATA_DIRS="${XDG_DATA_DIRS:-}"
    LANG="${LANG:-C.UTF-8}"
)

# setsid: own process group, so cleanup also stops sway's wrapped child.
setsid "${clean_env[@]}" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman \
    sway --config "$SESSION/sway.conf" >"$SESSION/sway.log" 2>&1 &
SWAY_PID=$!
SESHAT_PID=""

cleanup() {
    [[ -n "$SESHAT_PID" ]] && kill "$SESHAT_PID" 2>/dev/null || true
    sleep 0.5
    [[ -n "$SESHAT_PID" ]] && kill -9 "$SESHAT_PID" 2>/dev/null || true
    kill -- -"$SWAY_PID" 2>/dev/null || kill "$SWAY_PID" 2>/dev/null || true
    cp "$SESSION"/*.log "$ARTIFACTS"/ 2>/dev/null || true
    echo "artifacts: $ARTIFACTS"
}
trap cleanup EXIT

for _ in $(seq 100); do
    SOCKET=$(find "$SESSION/runtime" -maxdepth 1 -name 'wayland-*' -type s -print -quit)
    SWAYSOCK=$(find "$SESSION/runtime" -maxdepth 1 -name 'sway-ipc.*' -type s -print -quit)
    [[ -n "$SOCKET" && -n "$SWAYSOCK" ]] && break
    sleep 0.1
done
[[ -n "${SOCKET:-}" ]] || { cat "$SESSION/sway.log"; echo "sway did not start"; exit 1; }
WAYLAND_DISPLAY=$(basename "$SOCKET")

# ── Helpers for test scripts ────────────────────────────────────────────────

FAILURES=0
HOME_DIR="$SESSION/home"
DATA_DIR="$SESSION/data"

# Every input/IPC command goes through here: isolated env, private socket.
in_session() {
    [[ "$SOCKET" == "$SESSION/runtime/"* ]] || { echo "refusing: socket outside session"; exit 2; }
    "${clean_env[@]}" WAYLAND_DISPLAY="$WAYLAND_DISPLAY" SWAYSOCK="$SWAYSOCK" "$@"
}

# Seshat runs in a bubblewrap sandbox where the whole filesystem is
# read-only except the session directory, so a test that goes astray (e.g.
# navigates somewhere unexpected and presses Delete) cannot touch real files.
launch() {
    "${clean_env[@]}" WAYLAND_DISPLAY="$WAYLAND_DISPLAY" GDK_BACKEND=wayland GSK_RENDERER=cairo \
        RUST_LOG="${RUST_LOG:-info,seshat_presentation=debug}" \
        bwrap --ro-bind / / --dev /dev --proc /proc --bind "$SESSION" "$SESSION" \
        --die-with-parent -- "$BIN" >>"$SESSION/seshat.log" 2>&1 &
    SESHAT_PID=$!
    wait_for_window "Seshat"
}

wait_for_window() {
    for _ in $(seq 100); do
        in_session swaymsg -t get_tree 2>/dev/null | grep -q "\"name\": \"$1\"" && { sleep 0.5; return; }
        sleep 0.1
    done
    echo "window '$1' did not appear"; exit 1
}

# The pause lets the compositor apply wtype's keymap before the first key.
type_text() { in_session wtype -s 150 -- "$1"; sleep "${2:-0.4}"; }
press() { in_session wtype -s 150 -k "$1"; sleep "${2:-0.4}"; }
# chord ctrl shift n  →  Ctrl+Shift+N
chord() {
    local args=() mods=() key
    while (($# > 1)); do mods+=("$1"); shift; done
    key=$1
    for m in "${mods[@]}"; do args+=(-M "$m"); done
    if ((${#key} == 1)); then args+=("$key"); else args+=(-k "$key"); fi
    for m in "${mods[@]}"; do args+=(-m "$m"); done
    in_session wtype -s 150 "${args[@]}"
    sleep 0.6
}
swaycmd() { in_session swaymsg "$@" >/dev/null; sleep 0.4; }
shot() { in_session grim "$ARTIFACTS/$1.png"; }
# Headless sway does not refocus a parent after its dialog closes (desktop
# compositors do), so tests refocus the main window explicitly.
focus_main() { swaycmd '[title="^Seshat$"] focus'; }

check() {
    local description=$1; shift
    if "$@"; then
        echo "  ok   $description"
    else
        echo "  FAIL $description"
        FAILURES=$((FAILURES + 1))
    fi
}

seshat_running() { kill -0 "$SESHAT_PID" 2>/dev/null; }
window_open() { in_session swaymsg -t get_tree | grep -q "\"name\": \"$1\""; }
not() { ! "$@"; }

wait_exit() {
    for _ in $(seq $(($1 * 10))); do seshat_running || return 0; sleep 0.1; done
    return 1
}

# Prove the sandbox really is read-only outside the session directory.
if bwrap --ro-bind / / --dev /dev --proc /proc --bind "$SESSION" "$SESSION" \
    -- touch "$ROOT/.seshat-sandbox-probe" 2>/dev/null; then
    rm -f "$ROOT/.seshat-sandbox-probe"
    echo "sandbox is writable outside the session; refusing to run"
    exit 1
fi

echo "UI session: $SESSION (display $WAYLAND_DISPLAY, sandboxed)"
# shellcheck source=/dev/null
source "$DRIVER"

echo
if ((FAILURES == 0)); then
    echo "PASS: all UI checks passed"
else
    echo "FAIL: $FAILURES UI check(s) failed"
    exit 1
fi
