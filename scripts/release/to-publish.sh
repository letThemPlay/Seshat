#!/usr/bin/env bash
# Decides whether a push to main is a release (docs/development/releasing.md):
# prints the version to publish when the push changed the workspace version
# and that version has no tag yet, and nothing otherwise. So only a merged
# release PR publishes, and running it twice publishes once.
#
#   scripts/release/to-publish.sh BEFORE
#
# BEFORE is the commit main pointed to before the push. Needs the history
# back to BEFORE.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/release/common.sh

before=${1:?usage: scripts/release/to-publish.sh BEFORE}
if [[ "$before" =~ ^0+$ ]] || ! git cat-file -e "$before^{commit}" 2>/dev/null; then
    echo "no previous commit to compare with ($before): nothing to publish" >&2
    exit 0
fi

now=$(workspace_version)
was=$(git show "$before:Cargo.toml" | sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p')
if [[ "$now" == "$was" ]]; then
    echo "the version is still $now: nothing to publish" >&2
    exit 0
fi
if git rev-parse -q --verify "refs/tags/v$now" >/dev/null; then
    echo "v$now is already tagged: nothing to publish" >&2
    exit 0
fi
scripts/release/check-versions.sh >&2
echo "$now"
