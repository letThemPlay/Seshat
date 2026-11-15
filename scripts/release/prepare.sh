#!/usr/bin/env bash
# Prepares a release in the working tree (docs/development/releasing.md):
#
#   1. bumps the shared workspace version in Cargo.toml and Cargo.lock
#      (cargo-release);
#   2. turns the changelog's "## Unreleased" section into the release's
#      section, "## VERSION (DATE)", under a new, empty Unreleased heading;
#   3. adds the release, newest first, to the AppStream metadata
#      (type="development" for a pre-release).
#
# It commits, tags and pushes nothing: the "Prepare release" workflow (or
# you) commits the result as "chore(release): VERSION" in a release PR.
#
#   scripts/release/prepare.sh LEVEL|VERSION [--date YYYY-MM-DD]
#
#   LEVEL    auto, alpha, beta, rc, patch, minor, major or release
#            (auto: scripts/release/level.sh)
#   VERSION  X.Y.Z or X.Y.Z-alpha.N, -beta.N, -rc.N; higher than now
#
# Prints the new version as the last line. Run inside `nix develop`.
set -euo pipefail
cd "$(dirname "$0")/../.."
source scripts/release/common.sh

usage() {
    sed -n 's/^#   //p' "$0" | head -8 >&2
    exit 2
}

target=""
date=$(date -u +%F)
while (($#)); do
    case "$1" in
        --date)
            [[ $# -ge 2 && "$2" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]] || die "--date needs YYYY-MM-DD"
            date=$2
            shift 2
            ;;
        -h | --help) usage ;;
        -*) usage ;;
        *)
            [[ -z "$target" ]] || usage
            target=$1
            shift
            ;;
    esac
done
[[ -n "$target" ]] || usage

# Everything that can be checked before changing a file is checked first,
# so a refused release leaves the tree as it was.
installed=$(cargo release --version 2>/dev/null | sed -n 's/^cargo-release \([0-9]*\.[0-9]*\)\..*/\1/p')
[[ -n "$installed" ]] || die "cargo-release is not available; run inside nix develop"
[[ "$installed" == "$CARGO_RELEASE_SERIES" ]] ||
    die "cargo-release $installed changed (tested with $CARGO_RELEASE_SERIES); re-run scripts/release/test.sh and update CARGO_RELEASE_SERIES in scripts/release/common.sh"

scripts/release/check-versions.sh >/dev/null
current=$(workspace_version)

[[ -n "$(unreleased_text)" ]] ||
    die "the \"## Unreleased\" section of $CHANGELOG is empty: describe the changes before releasing"

case "$target" in
    auto) level=$(scripts/release/level.sh) ;;
    alpha | beta | rc | patch | minor | major | release) level=$target ;;
    *)
        [[ "$target" =~ $VERSION_RE ]] ||
            die "\"$target\" is neither a level nor a version X.Y.Z or X.Y.Z-alpha/beta/rc.N"
        level=$target
        ;;
esac

# cargo-release's dry run (its default) says which version it would set.
if ! plan=$(cargo release version "$level" --no-confirm 2>&1); then
    echo "$plan" >&2
    die "cargo-release cannot bump $current with \"$level\""
fi
version=$(sed -n 's/.*Upgrading seshat from [^ ]* to \([^ ]*\).*/\1/p' <<<"$plan" | head -1)
[[ -n "$version" ]] || die "\"$level\" doesn't give a version higher than $current"
[[ "$version" =~ $VERSION_RE ]] ||
    die "\"$level\" gives $version, which is not X.Y.Z or X.Y.Z-alpha/beta/rc.N; choose a version"
if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then
    die "tag v$version already exists"
fi
grep -q "^## $version (" "$CHANGELOG" && die "$CHANGELOG already has a section for $version"

if ! output=$(cargo release version "$level" --execute --no-confirm 2>&1); then
    echo "$output" >&2
    die "cargo-release could not bump $current with \"$level\""
fi
[[ "$(workspace_version)" == "$version" ]] ||
    die "cargo-release set $(workspace_version), not the planned $version"

# The changelog: the Unreleased section becomes the release's section.
awk -v heading="## $version ($date)" '
    !done && /^## Unreleased$/ { print; print ""; print heading; done = 1; next }
    { print }
' "$CHANGELOG" >"$CHANGELOG.tmp"
mv "$CHANGELOG.tmp" "$CHANGELOG"

# The AppStream metadata: the newest release comes first.
type=""
if is_prerelease "$version"; then type=' type="development"'; fi
awk -v release="    <release version=\"$version\" date=\"$date\"$type/>" '
    { print }
    !done && /^ *<releases>$/ { print release; done = 1 }
' "$METAINFO" >"$METAINFO.tmp"
mv "$METAINFO.tmp" "$METAINFO"

scripts/release/check-versions.sh >/dev/null
echo "$current -> $version ($level)" >&2
echo "$version"
