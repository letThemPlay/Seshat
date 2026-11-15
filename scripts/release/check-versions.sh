#!/usr/bin/env bash
# Checks that the version is recorded consistently: Cargo.toml's workspace
# version, the newest release in CHANGELOG.md and the newest <release> in
# the AppStream metadata must be the same, and the changelog must have one
# "## Unreleased" section for the next release (docs/development/releasing.md).
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/release/common.sh

cargo=$(workspace_version)
changelog=$(changelog_version)
metainfo=$(metainfo_version)

[[ "$cargo" =~ $VERSION_RE ]] || die "Cargo.toml version \"$cargo\" is not X.Y.Z or X.Y.Z-alpha/beta/rc.N"
[[ "$changelog" == "$cargo" ]] ||
    die "$CHANGELOG's newest release is \"$changelog\", but Cargo.toml says $cargo"
[[ "$metainfo" == "$cargo" ]] ||
    die "$METAINFO's newest release is \"$metainfo\", but Cargo.toml says $cargo"
unreleased=$(grep -c '^## Unreleased$' "$CHANGELOG" || true)
[[ "$unreleased" == 1 ]] ||
    die "$CHANGELOG needs exactly one \"## Unreleased\" heading, found $unreleased"

echo "PASS: version $cargo in Cargo.toml, $CHANGELOG and $METAINFO"
