#!/usr/bin/env bash
# Tests the release scripts on a throwaway copy of the repository, with the
# cargo-release of the dev shell: a full alpha -> beta -> rc -> release
# sequence, patch and minor releases, explicit versions, the automatic
# level, and every refusal. Run inside `nix develop`; CI runs it on every
# pull request, so a cargo-release upgrade (flake.lock) that changes how
# versions are bumped fails here first (docs/development/releasing.md).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d -t seshat-release-test-XXXXXX)
[[ -n "$WORK" && -d "$WORK" ]] || exit 1
trap 'rm -rf -- "$WORK"' EXIT

# The working tree as it is, including uncommitted changes to the scripts.
mkdir "$WORK/repo"
(cd "$ROOT" && git ls-files --cached --others --exclude-standard -z |
    tar --null --ignore-failed-read -T - -cf - 2>/dev/null) | tar -xf - -C "$WORK/repo"
cd "$WORK/repo"
git init -q
git config user.name test
git config user.email test@example.invalid
git config commit.gpgsign false
git config tag.gpgsign false
git add -A
git commit -qm "chore: start"

source scripts/release/common.sh
DATE=2026-01-02
FAILURES=0
N=0

check() {
    local description=$1
    shift
    if "$@"; then
        echo "  ok   $description"
    else
        echo "  FAIL $description"
        FAILURES=$((FAILURES + 1))
    fi
}
not() { ! "$@"; }

prepare() { scripts/release/prepare.sh "$@" --date "$DATE" >/dev/null 2>"$WORK/err"; }
refused() {
    local reason=$1
    shift
    ! prepare "$@" && grep -q -- "$reason" "$WORK/err" && [[ -z "$(git status --porcelain)" ]]
}
note() {
    N=$((N + 1))
    awk -v line="- test change $N" '{ print } /^## Unreleased$/ { print ""; print line }' \
        "$CHANGELOG" >"$CHANGELOG.tmp"
    mv "$CHANGELOG.tmp" "$CHANGELOG"
    commit "docs: change $N"
}
commit() {
    git add -A
    git commit -qm "$1" --allow-empty
}
is_version() { [[ "$(workspace_version)" == "$1" ]]; }
locked() { grep -A1 '^name = "seshat-domain"$' Cargo.lock | grep -qx "version = \"$1\""; }
newest_section() {
    [[ "$(grep -m2 '^## ' "$CHANGELOG" | tail -1)" == "## $1 ($DATE)" ]]
}
newest_release() { [[ "$(grep -m1 '<release ' "$METAINFO" | sed 's/^ *//')" == "$1" ]]; }
level_is() { [[ "$(scripts/release/level.sh)" == "$1" ]]; }
released() {
    is_version "$1" && locked "$1" && newest_section "$1" && scripts/release/check-versions.sh >/dev/null
}

echo "Release scripts on a copy in $WORK/repo"
check "the copy's versions agree" scripts/release/check-versions.sh

echo "-- a pre-release cycle"
check "an explicit first pre-release, 0.2.0-alpha.1" prepare 0.2.0-alpha.1
check "  Cargo.toml, Cargo.lock and the changelog say so" released 0.2.0-alpha.1
check "  AppStream marks it as a development release" \
    newest_release '<release version="0.2.0-alpha.1" date="2026-01-02" type="development"/>'
check "  Unreleased is empty again" test -z "$(unreleased_text)"
commit "chore(release): 0.2.0-alpha.1"
check "nothing new in Unreleased: refused, nothing changed" refused "is empty" alpha
check "auto stays on the alpha label" level_is alpha
note
check "alpha: 0.2.0-alpha.2" prepare alpha
check "  recorded everywhere" released 0.2.0-alpha.2
commit "chore(release): 0.2.0-alpha.2"
note
check "beta: 0.2.0-beta.1" prepare beta
check "  recorded everywhere" released 0.2.0-beta.1
commit "chore(release): 0.2.0-beta.1"
note
check "rc: 0.2.0-rc.1" prepare rc
check "  recorded everywhere" released 0.2.0-rc.1
commit "chore(release): 0.2.0-rc.1"
note
check "release: 0.2.0" prepare release
check "  recorded everywhere" released 0.2.0
check "  AppStream lists it as a stable release" \
    newest_release '<release version="0.2.0" date="2026-01-02"/>'
check "  every release so far is in AppStream, newest first" \
    test "$(grep -c '<release ' "$METAINFO")" = 6
commit "chore(release): 0.2.0"

echo "-- publishing (what the Publish release workflow runs)"
publishes() { [[ "$(scripts/release/to-publish.sh "$1" 2>/dev/null)" == "$2" ]]; }
check "the merged release PR publishes 0.2.0" publishes HEAD~1 0.2.0
check "its notes are the changelog section" \
    test "$(scripts/release/notes.sh 0.2.0)" = "- test change 4"
check "a pre-release's notes too" test "$(scripts/release/notes.sh 0.2.0-beta.1)" = "- test change 2"
check "no notes for a version that wasn't released" not scripts/release/notes.sh 0.9.0
git tag v0.2.0
check "once tagged, nothing more to publish" publishes HEAD~1 ""
check "a push that keeps the version publishes nothing" publishes HEAD ""
check "a new branch (no previous commit) publishes nothing" \
    publishes 0000000000000000000000000000000000000000 ""

echo "-- stable releases and the automatic level"
check "auto after a release with no feat: patch" level_is patch
commit "feat(workspace): something new"
check "auto after a feat: minor" level_is minor
commit "refactor!: something breaking"
check "auto after a breaking change, below 1.0: minor" level_is minor
note
check "patch: 0.2.1" prepare patch
check "  recorded everywhere" released 0.2.1
commit "chore(release): 0.2.1"
git tag v0.2.1
note
check "minor: 0.3.0" prepare minor
check "  recorded everywhere" released 0.3.0
commit "chore(release): 0.3.0"
git tag v0.3.0
commit "fix: something fixed"
note
check "auto with only a fix: 0.3.1" prepare auto
check "  recorded everywhere" released 0.3.1
commit "chore(release): 0.3.1"

echo "-- refusals leave everything as it was"
note
check "the current version" refused "doesn't give a version higher" 0.3.1
check "a lower version" refused "cannot bump" 0.3.0
check "an unknown pre-release label" refused "neither a level nor a version" 0.4.0-preview.1
check "an unknown level" refused "neither a level nor a version" huge
git tag v0.5.0
check "a version that is already tagged" refused "already exists" 0.5.0
other_series() {
    ! SESHAT_CARGO_RELEASE_SERIES=0.0 scripts/release/prepare.sh patch >/dev/null 2>"$WORK/err" &&
        grep -q "changed (tested with 0.0)" "$WORK/err" && [[ -z "$(git status --porcelain)" ]]
}
check "another cargo-release series than the tested one" other_series
check "explicit pre-release of the next minor: 0.4.0-beta.1" prepare 0.4.0-beta.1
check "  recorded everywhere" released 0.4.0-beta.1
commit "chore(release): 0.4.0-beta.1"

echo "-- the consistency check"
sed -i 's/<release version="0.4.0-beta.1"/<release version="9.9.9"/' "$METAINFO"
check "AppStream out of step: fails" not scripts/release/check-versions.sh
git checkout -q -- "$METAINFO"
sed -i 's/^## 0.4.0-beta.1 /## 0.4.0-beta.9 /' "$CHANGELOG"
check "the changelog out of step: fails" not scripts/release/check-versions.sh
git checkout -q -- "$CHANGELOG"
sed -i 's/^## Unreleased$/## Next/' "$CHANGELOG"
check "no Unreleased section: fails" not scripts/release/check-versions.sh
git checkout -q -- "$CHANGELOG"
check "back in step: passes" scripts/release/check-versions.sh

echo
if ((FAILURES == 0)); then
    echo "PASS: release scripts"
else
    echo "FAIL: $FAILURES release script check(s) failed"
    exit 1
fi
