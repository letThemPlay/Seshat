#!/usr/bin/env bash
# Runs every automated check. Run inside `nix develop`.
#   scripts/check.sh          static checks, tests, Wayland verification
#   scripts/check.sh --ui     ...plus the sandboxed UI end-to-end tests
set -euo pipefail
cd "$(dirname "$0")/.."

step() { echo; echo "── $*"; }

step "format";        cargo fmt --check
step "clippy";        cargo clippy --all-targets --all-features -- -D warnings
step "architecture";  scripts/check-architecture.sh
step "css classes";   scripts/check-css-classes.sh
if git rev-parse --verify --quiet origin/main >/dev/null; then
    step "commits";   scripts/check-commits.sh origin/main
else
    step "commits";   echo "skipped: no origin/main"
fi
step "tests";         cargo test --workspace
step "wayland";       scripts/verify-wayland.sh
if [[ "${1:-}" == "--ui" ]]; then
    step "ui";        scripts/ui-session.sh scripts/ui/file-operations.sh
    step "ui: workspace"; scripts/ui-session.sh scripts/ui/workspace.sh
    step "ui: fuzzy find"; scripts/ui-session.sh scripts/ui/fuzzy-find.sh
fi
echo; echo "ALL CHECKS PASSED"
