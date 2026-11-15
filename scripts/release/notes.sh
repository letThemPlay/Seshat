#!/usr/bin/env bash
# Prints the changelog section of VERSION, without its heading: the notes
# of the release PR and of the GitHub Release (docs/development/releasing.md).
#
#   scripts/release/notes.sh VERSION
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/release/common.sh

version=${1:?usage: scripts/release/notes.sh VERSION}
grep -qF "## $version (" "$CHANGELOG" || die "$CHANGELOG has no section for $version"
awk -v heading="## $version (" '
    index($0, heading) == 1 { inside = 1; next }
    inside && /^## / { exit }
    inside { print }
' "$CHANGELOG" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
