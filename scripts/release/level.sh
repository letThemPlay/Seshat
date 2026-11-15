#!/usr/bin/env bash
# Suggests the next release level from the Conventional Commits since the
# last release tag (docs/development/releasing.md):
#
#   during a pre-release   the same label again (alpha, beta or rc), so
#                          0.2.0-alpha.1 becomes 0.2.0-alpha.2; leaving a
#                          pre-release is always an explicit choice
#   breaking change        minor below 1.0, major from 1.0
#   feat                   minor
#   anything else          patch
#
# Prints the level. Run in the repository.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/release/common.sh

current=$(workspace_version)
if is_prerelease "$current"; then
    label=${current#*-}
    echo "${label%%.*}"
    exit 0
fi

if last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null); then
    range="$last..HEAD"
else
    range=HEAD
fi
log=$(git log --no-merges --format='%s%n%b' "$range")

type='[a-z]+(\([a-z0-9-]+\))?'
if grep -qE "^$type!:|^BREAKING[ -]CHANGE:" <<<"$log"; then
    if [[ "${current%%.*}" == 0 ]]; then echo minor; else echo major; fi
elif grep -qE '^feat(\([a-z0-9-]+\))?:' <<<"$log"; then
    echo minor
else
    echo patch
fi
