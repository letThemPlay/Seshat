# shellcheck shell=bash disable=SC2034  # sourced: the variables are used by the scripts
# Shared by the release scripts (docs/development/releasing.md). Sourced,
# not run. Every function reads the files of the repository in the current
# directory.

# The cargo-release series the scripts are tested with. A different one
# stops prepare.sh until scripts/release/test.sh has been re-run with it and
# this line updated, so an upgrade is always a deliberate, reviewed change.
CARGO_RELEASE_SERIES=${SESHAT_CARGO_RELEASE_SERIES:-1.1}

# The versions Seshat releases: X.Y.Z, or X.Y.Z-alpha.N, -beta.N or -rc.N.
VERSION_RE='^[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta|rc)\.[0-9]+)?$'

CHANGELOG=CHANGELOG.md
METAINFO=data/dev.kdmorris.Seshat.metainfo.xml

die() {
    echo "error: $*" >&2
    exit 1
}

# The version in [workspace.package] of Cargo.toml.
workspace_version() {
    sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' Cargo.toml
}

# The newest release heading in the changelog: "## X.Y.Z (YYYY-MM-DD)".
changelog_version() {
    sed -n 's/^## \([0-9][^ ]*\) ([0-9]\{4\}-[0-9]\{2\}-[0-9]\{2\})$/\1/p' "$CHANGELOG" | head -1
}

# The newest <release> in the AppStream metadata (releases are newest first).
metainfo_version() {
    sed -n 's/^ *<release version="\([^"]*\)".*/\1/p' "$METAINFO" | head -1
}

# The text under "## Unreleased", up to the next "## " heading, without
# blank lines.
unreleased_text() {
    awk '/^## Unreleased$/ { inside = 1; next } inside && /^## / { exit } inside && NF' "$CHANGELOG"
}

is_prerelease() {
    [[ "$1" == *-* ]]
}
