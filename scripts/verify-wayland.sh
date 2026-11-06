#!/usr/bin/env bash
# Verifies that Seshat runs as a native Wayland client.
#
# Starts an isolated headless Weston compositor (no X server, no XWayland),
# then:
#   1. runs `seshat --check-backend`, which inspects the actual GdkDisplay type;
#   2. launches the full UI with WAYLAND_DEBUG=1 and confirms it creates an
#      xdg_toplevel surface on the Wayland connection;
#   3. confirms the check fails when no Wayland compositor is reachable.
#
# Run inside `nix develop`. Exits non-zero on any failure.
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build -q -p seshat
BIN="$PWD/target/debug/seshat"

RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$RUNTIME_DIR"
SOCKET="seshat-verify-$$"
LOG_DIR="$RUNTIME_DIR/logs"
mkdir -p "$LOG_DIR"

cleanup() {
    [[ -n "${WESTON_PID:-}" ]] && kill "$WESTON_PID" 2>/dev/null || true
    rm -rf "$RUNTIME_DIR"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }

XDG_RUNTIME_DIR="$RUNTIME_DIR" weston --backend=headless --socket="$SOCKET" \
    --idle-time=0 >"$LOG_DIR/weston.log" 2>&1 &
WESTON_PID=$!

for _ in $(seq 50); do
    [[ -S "$RUNTIME_DIR/$SOCKET" ]] && break
    sleep 0.1
done
[[ -S "$RUNTIME_DIR/$SOCKET" ]] || { cat "$LOG_DIR/weston.log"; fail "weston did not start"; }

# Isolated environment: no X11 display, Wayland only.
run() {
    env -i PATH="$PATH" HOME="$HOME" XDG_RUNTIME_DIR="$RUNTIME_DIR" \
        WAYLAND_DISPLAY="$SOCKET" XDG_SESSION_TYPE=wayland GDK_BACKEND=wayland \
        XDG_DATA_DIRS="${XDG_DATA_DIRS:-}" RUST_LOG=info "$@"
}

echo "== 1. backend check"
run "$BIN" --check-backend | tee "$LOG_DIR/check.txt"
grep -q "display backend: wayland (native, $SOCKET)" "$LOG_DIR/check.txt" \
    || fail "backend is not native Wayland"

echo "== 2. full UI creates an xdg_toplevel"
set +e
run WAYLAND_DEBUG=1 timeout 4 "$BIN" >"$LOG_DIR/ui.log" 2>&1
status=$?
set -e
[[ $status -eq 124 ]] || { tail -40 "$LOG_DIR/ui.log"; fail "UI exited early (status $status)"; }
grep -q "xdg_wm_base#[0-9]*.get_xdg_surface" "$LOG_DIR/ui.log" || fail "no xdg_surface created"
grep -q "get_toplevel" "$LOG_DIR/ui.log" || fail "no xdg_toplevel created"
grep -q "display backend verified" "$LOG_DIR/ui.log" || fail "UI did not verify its backend"
echo "xdg_toplevel created on $SOCKET"

echo "== 3. check fails without a Wayland compositor"
if env -i PATH="$PATH" HOME="$HOME" XDG_RUNTIME_DIR="$RUNTIME_DIR" \
    WAYLAND_DISPLAY="missing-$$" GDK_BACKEND=wayland "$BIN" --check-backend >/dev/null 2>&1; then
    fail "--check-backend succeeded without a compositor"
fi
echo "correctly rejected"

echo "PASS: Seshat runs as a native Wayland client (backend recorded above)"
