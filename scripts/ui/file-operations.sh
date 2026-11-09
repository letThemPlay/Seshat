# UI test: file operations end to end. Sourced by scripts/ui-session.sh,
# which provides HOME_DIR, DATA_DIR and the input/check helpers.

H="$HOME_DIR"
mkdir -p "$H/Notes" "$H/Projects"
for f in file1.txt file2.txt report.pdf; do echo "content of $f" > "$H/$f"; done
mkdir "$H/many"; (cd "$H/many" && head -c 60M /dev/urandom | split -b 2k -a 5 - part-)   # ~30k files: cannot be reflinked in one go

# Search → results → copy → paste
launch
check "D-Bus service is published"            ipc_published
type_text /   # search starts with / (decision D1)
type_text file1
press Return                      # leave the search box for the results
chord ctrl c
chord ctrl l; type_text "$H/Notes"; press Return 1
chord ctrl v; sleep 1.5
check "paste copies the file"                 cmp -s "$H/file1.txt" "$H/Notes/file1.txt"

# Pasting again conflicts; the default answer is Skip
echo "changed" > "$H/file1.txt"
chord ctrl v; sleep 1.5
shot conflict-dialog
press Return 1; focus_main
check "conflict skip keeps the existing file" grep -q "content of file1.txt" "$H/Notes/file1.txt"
check "skip does not create a copy"           test ! -e "$H/Notes/file1 (2).txt"

# New folder, then rename it (it is selected after creation)
chord ctrl shift N; shot new-folder-dialog
press Return 1.5; focus_main
check "new folder created"                    test -d "$H/Notes/New Folder"
press F2 0.8
chord ctrl a; type_text Renamed; press Return 1.5; focus_main
check "rename applied"                        test -d "$H/Notes/Renamed"
shot after-rename
chord shift F10; shot context-menu; press Escape

# Move to Trash (isolated XDG_DATA_HOME)
type_text /; type_text file1; press Return
shot before-trash; press Delete 1.5
check "trashed file left the folder"          test ! -e "$H/Notes/file1.txt"
check "trashed file is in the Trash"          test -e "$DATA_DIR/Trash/files/file1.txt"
press Escape

# Permanent delete: the confirmation defaults to Cancel
type_text /; type_text Renamed; press Return
chord shift Delete; shot delete-confirmation
press Return 1; focus_main
check "Return cancels permanent delete"       test -d "$H/Notes/Renamed"
chord shift Delete; shot delete-confirm-2
press Tab; press space 1.5; focus_main
check "confirmed permanent delete"            test ! -e "$H/Notes/Renamed"
check "deleted folder is not in the Trash"    test ! -e "$DATA_DIR/Trash/files/Renamed"

# Large copy with progress; closing the window must not abort it
chord ctrl l; type_text "$H"; press Return 1
type_text /; type_text many; press Return
chord ctrl c
chord ctrl l; type_text "$H/Projects"; press Return 1
chord ctrl v; sleep 0.5
shot progress
swaycmd '[title="^Seshat$"] kill'
check "process exits when the copy finishes"  wait_exit 120
check "close was deferred while copying"     grep -q "finishing operations in the background" "$SESSION/seshat.log"
check "app quit after the background copy"   grep -q "background operations finished; exiting" "$SESSION/seshat.log"
check "large copy is complete and identical"  diff -rq "$H/many" "$H/Projects/many"
check "no temporary files left"               bash -c "! find '$H' -name '*.seshat-part-*' | grep -q ."

# Escape dismisses dialogs. A modal dialog keeps its window open (GTK
# semantics); once dismissed, closing the window exits the app.
launch
chord ctrl shift N; press Escape
check "Escape closes the dialog"              not window_open "New Folder"
focus_main; chord ctrl shift N
swaycmd '[title="^Seshat$"] kill'
check "a modal dialog keeps the window open"  window_open "Seshat"
press Escape
swaycmd '[title="^Seshat$"] kill'
check "closing after dismissing exits"        wait_exit 5

check "no panics or errors in the log"        not grep -qE "panicked|ERROR" "$SESSION/seshat.log"
