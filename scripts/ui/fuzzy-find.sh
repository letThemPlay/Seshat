# UI test: Fuzzy Find (docs/specifications/4-FUZZY-FIND.md §17 UI tests).
# Sourced by scripts/ui-session.sh, which provides HOME_DIR and the input
# helpers.
#
# Workspace state is read from the "workspace state" debug line logged on
# every structural change; the overlay logs when it opens and closes.

H="$HOME_DIR"
mkdir -p "$H/Documents/Projects/src" "$H/Downloads/Camera"
echo notes > "$H/notes.txt"
echo code > "$H/Documents/Projects/src/main.rs"
mkdir -p "$H/.secret-stash" "$H/build-cache"
echo x > "$H/build-cache/artifact.bin"
# Settings (Spec 4 §15): hidden names are searched, and one more name is
# ignored besides the defaults.
mkdir -p "$SESSION/config/seshat"
cat > "$SESSION/config/seshat/settings.toml" <<'TOML'
[fuzzy_find]
include_hidden = true
ignore = [".git", "node_modules", "build-cache"]
TOML

log_text() { sed 's/\x1b\[[0-9;]*m//g' "$SESSION/seshat.log"; }
# grep -c reads all its input (see workspace.sh).
log_count() { log_text | grep -c -- "$1" || true; }
log_has() { (($(log_count "$1") > 0)); }
state() { log_text | grep "workspace state" | tail -1; }
state_has() { state | grep -q -- "$1"; }
active_is() { state_has "location=Path(\"$1\")"; }
opened_count() { log_count "fuzzy find opened"; }
last_results() { log_text | grep "fuzzy find results" | tail -1; }
closed_count() { log_count "fuzzy find closed"; }
is() { (($1 == $2)); }
# Search never changes files (Spec 4 §12).
fingerprint() { find "$H" -printf '%P %s %T@\n' | sort | sha256sum; }
files_before=$(fingerprint)
files_unchanged() { [[ "$(fingerprint)" == "$files_before" ]]; }

launch
check "starts at Home"                              active_is "$H"

# Opening: the search box takes the keyboard; the workspace is untouched.
chord ctrl p
check "Ctrl+P opens Fuzzy Find"                     log_has "fuzzy find opened"
check "    without changing the workspace"          state_has "branches=1 panels=1 "
type_text src 1
shot results
check "results appear as the query is typed"        log_has "fuzzy find results"

# Enter on a folder opens it along the path, one panel per level (F5).
press Return 1
check "Enter opens the folder along the path"       active_is "$H/Documents/Projects/src"
check "    one panel per level, one branch"         state_has "branches=1 panels=4 "
check "    and closes the overlay"                  log_has "fuzzy find closed"
chord alt Left; sleep 0.5
check "Back returns to where the search started"    active_is "$H"

# Esc closes without changing anything.
chord ctrl p; type_text notes; press Escape
check "Esc closes Fuzzy Find"                       is "$(closed_count)" 2
check "    and leaves the workspace as it was"      state_has "branches=1 panels=4 "
check "    with Home still active"                  active_is "$H"

# The settings are used: a hidden folder is found, an ignored one is not.
chord ctrl p; type_text stash 1; press Escape
check "include_hidden: hidden folders are searched"  grep -q "results=1 " <<<"$(last_results)"
chord ctrl p; type_text artifact 1; press Escape
check "ignore: ignored folders are skipped"          grep -q "results=0 " <<<"$(last_results)"

# Enter on a file is not tried here: it would start whatever application
# the machine has for the file type. The service test
# a_file_result_is_launched_and_the_workspace_is_untouched covers it, and
# the manual checklist covers the launch itself.

# Focus Mode: navigation works and Focus Mode is kept (Spec 1 §6.3).
type_text zz
check "zz enters Focus Mode"                        state_has "focus_mode=true"
chord ctrl p; type_text camera 1; press Return 1
check "Fuzzy Find works in Focus Mode"              active_is "$H/Downloads/Camera"
check "    Focus Mode is kept"                      state_has "focus_mode=true"
check "    a new branch, nothing duplicated"        state_has "branches=2 panels=6 "
type_text zz

# Tab and Shift+Tab cycle the scope; the overlay stays open with the query
# (F14).
before=$(closed_count)
chord ctrl p; type_text src 1
searched=$(log_count "fuzzy find results")
press Tab 1
check "Tab keeps Fuzzy Find open"                   is "$(closed_count)" "$before"
check "    and moves to Current Branch"             log_has "fuzzy find scope scope=CurrentBranch"
press Tab 1
check "Tab again: Entire Workspace"                 log_has "fuzzy find scope scope=EntireWorkspace"
press Tab 1
check "    and round to Current Directory"          is "$(log_count "fuzzy find scope scope=CurrentDirectory")" 1
chord shift Tab; sleep 1
check "Shift+Tab goes back to Entire Workspace"     is "$(log_count "fuzzy find scope scope=EntireWorkspace")" 2
check "    each switch searches the query again"    is "$(log_count "fuzzy find results")" $((searched + 4))
shot scope-keys
press Tab 1; press Escape
check "    (back to Current Directory)"             is "$(log_count "fuzzy find scope scope=CurrentDirectory")" 2

# Entire Workspace (Alt+3): a result found under the other branch
# reactivates that branch; nothing is duplicated (§6.3, F5).
chord ctrl p; chord alt 3; type_text src 1
check "Alt+3 searches the entire workspace"         is "$(log_count "fuzzy find scope scope=EntireWorkspace")" 3
shot workspace-scope
press Return 1
check "Enter reactivates the branch it was found in" active_is "$H/Documents/Projects/src"
check "    nothing duplicated"                      state_has "branches=2 panels=6 "

# The scope chosen is kept for the next search (F12). Alt+Enter reveals:
# the other branch comes back with the cursor on the result (F7, F9).
chord ctrl p
check "the chosen scope is kept"                    log_has "fuzzy find opened.*EntireWorkspace"
type_text camera 1; chord alt Return; sleep 1
check "Alt+Enter reveals the result in its folder"  active_is "$H/Downloads"
check "    with the cursor on it"                   log_has "cursor placed.*/Downloads/Camera\") select=false"
check "    nothing duplicated"                      state_has "branches=2 panels=6 "
chord ctrl p; chord alt 1; press Escape

# A query starting with ~ or / searches in that folder (F10); Enter with
# nothing after the folder opens it, as the address bar would (D3).
scopes=$(log_count "fuzzy find scope scope=")
chord ctrl p; type_text "~/Documents/" 1; press Tab 1
check "Tab does nothing while a folder is typed"    is "$(log_count "fuzzy find scope scope=")" "$scopes"
press Return 1
check "Enter on ~/Documents/ opens the folder"      active_is "$H/Documents"
before=$(closed_count)
chord ctrl p; type_text "/no/such/folder/" 1; press Return 1
check "a folder that doesn't exist isn't opened"    is "$(closed_count)" "$before"
shot missing-folder
press Escape

# Visual Mode: Ctrl+P is refused, and the selection is kept (F6).
before=$(opened_count)
type_text v; chord ctrl p
check "Ctrl+P does nothing in Visual Mode"          is "$(opened_count)" "$before"
press Escape

check "searching changed no files"                  files_unchanged
swaycmd '[title="^Seshat$"] kill'
check "the app exits"                               wait_exit 10
check "no panics or errors in the log"              not grep -qE "panicked|ERROR" "$SESSION/seshat.log"
