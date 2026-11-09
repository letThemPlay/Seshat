#!/usr/bin/env bash
# Creates a throwaway folder tree for the manual verification checklist
# (docs/development/manual-verification.md). Every check that changes files
# works inside this tree, so no real files are involved.
#
#   scripts/manual/verification-fixtures.sh [folder]     # default: ~/seshat-verify
#
# Refuses to touch a folder that already exists; delete it to start over.
set -euo pipefail

DIR="${1:-$HOME/seshat-verify}"
if [[ -e "$DIR" ]]; then
    echo "refusing: $DIR already exists (delete it, or pass another folder)" >&2
    exit 1
fi

mkdir -p "$DIR"/Documents/Projects/src "$DIR"/Documents/Reports "$DIR"/Downloads \
         "$DIR"/Pictures "$DIR/Name with spaces" "$DIR"/.hidden-folder "$DIR"/Archive/2024-photos

echo "notes"               > "$DIR/Documents/notes.txt"
echo "fn main() {}"        > "$DIR/Documents/Projects/src/main.rs"
echo "# Project"           > "$DIR/Documents/Projects/README.md"
echo "quarterly report"    > "$DIR/Documents/Reports/report.txt"
echo "downloaded report"   > "$DIR/Downloads/report.txt"     # same name: a conflict on copy
echo "keep me"             > "$DIR/Downloads/keep.txt"
echo "trash me"            > "$DIR/Downloads/trash-me.txt"
echo "trash me too"        > "$DIR/Documents/trash-me-too.txt"
echo "spaces"              > "$DIR/Name with spaces/file with spaces.txt"
echo "hidden"              > "$DIR/.hidden-folder/hidden.txt"
printf 'not UTF-8\n'       > "$DIR/Downloads/$(printf 'caf\xe9.txt')"

# A file large enough to show copy progress and to cancel (64 MiB).
head -c 64M /dev/urandom > "$DIR/Downloads/large.bin"
# 2,000 small files: a busy folder for scrolling and selection.
mkdir "$DIR/many"
(cd "$DIR/many" && seq -w 2000 | sed 's/^/item-/' | xargs touch)

ln -s Documents/Projects "$DIR/link-to-projects"
ln -s does-not-exist "$DIR/broken-link"

echo "Created $DIR"
