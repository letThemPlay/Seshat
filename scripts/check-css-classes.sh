#!/usr/bin/env bash
# Fails if the stylesheet styles a `.seshat-*` class that no widget uses:
# such rules are dead, and usually mean a class was renamed on one side.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC=crates/seshat-presentation/src
failures=0
for class in $(grep -oE '\.seshat-[a-z0-9-]+' "$SRC/theme/css.rs" | sed 's/^\.//' | sort -u); do
    # Used as a whole word inside a string literal, outside the stylesheet.
    if ! grep -rqE --include='*.rs' --exclude=css.rs "\"([^\"]* )?$class( [^\"]*)?\"" "$SRC"; then
        echo "FAIL .$class is styled but no widget uses it"
        failures=$((failures + 1))
    fi
done
if ((failures > 0)); then
    exit 1
fi
echo "PASS: every styled .seshat-* class is used"
