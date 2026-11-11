#!/usr/bin/env bash
# Enforces the Clean Architecture dependency rules (docs/architecture/overview.md).
# Fails if a crate depends on something its layer must not know about.
set -euo pipefail
cd "$(dirname "$0")/.."

failures=0

# Direct, normal (non-dev) dependencies of a workspace crate.
deps() {
    cargo tree -q -p "$1" -e normal --depth 1 --prefix none \
        | awk 'NR > 1 {print $1}' | sort -u
}

forbid() {
    local crate=$1; shift
    local found
    found=$(deps "$crate" | grep -xE "$(IFS='|'; echo "$*")" || true)
    if [[ -n "$found" ]]; then
        echo "FAIL $crate must not depend on: $(echo $found)"
        failures=$((failures + 1))
    else
        echo "ok   $crate"
    fi
}

UI="gtk4|gdk4|gdk4-wayland|gdk4-x11|relm4|glib|gio"
RUNTIMES="tokio|async-std|smol|zbus|async-channel"
# Fuzzy matching sits behind the FuzzyMatcher port (Spec 4 §13).
MATCHERS="nucleo|nucleo-matcher|fuzzy-matcher|skim"

# Domain: entities and rules only.
forbid seshat-domain "$UI" "$RUNTIMES" "$MATCHERS" "seshat-.*" "serde|toml|rustix"
# Application: use cases and ports; no toolkit, IPC or concrete I/O.
forbid seshat-application "$UI" "$RUNTIMES" "$MATCHERS" "seshat-infrastructure|seshat-presentation" "serde|toml|rustix"
# Infrastructure implements ports; it never knows the UI.
forbid seshat-infrastructure "gtk4|gdk4|relm4" "seshat-presentation"
# Presentation consumes application functionality, not infrastructure.
forbid seshat-presentation "seshat-infrastructure" "$RUNTIMES" "$MATCHERS" "rustix"
# The example client speaks only the protocol.
forbid seshat-progress "seshat-.*"

# Presentation never performs filesystem I/O itself.
if grep -rnE '\bstd::fs\b|\bfs::(read|write|remove|create|rename|copy)' crates/seshat-presentation/src; then
    echo "FAIL seshat-presentation uses std::fs directly (see above)"
    failures=$((failures + 1))
else
    echo "ok   seshat-presentation performs no direct filesystem I/O"
fi

if ((failures > 0)); then
    echo "FAIL: $failures architecture rule(s) violated"
    exit 1
fi
echo "PASS: architecture rules hold"
