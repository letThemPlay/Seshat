#!/usr/bin/env bash
# Runs every automated check. Run inside `nix develop`.
#   scripts/check.sh          static checks, tests, Wayland verification
#   scripts/check.sh --ui     ...plus the sandboxed UI end-to-end test
set -euo pipefail
cd "$(dirname "$0")/.."

step() { echo; echo "── $*"; }

step "format";        cargo fmt --check
step "clippy";        cargo clippy --all-targets --all-features -- -D warnings
step "architecture";  scripts/check-architecture.sh
step "tests";         cargo test --workspace
step "wayland";       scripts/verify-wayland.sh
if [[ "${1:-}" == "--ui" ]]; then
    step "ui";        scripts/ui-session.sh scripts/ui/file-operations.sh
fi
echo; echo "ALL CHECKS PASSED"
