# UI test: the spatial workspace, following the definition of done of
# docs/specifications/1-SPATIAL-FILE-MANAGER.md §19. Sourced by
# scripts/ui-session.sh, which provides HOME_DIR and the input helpers.
#
# Workspace state is read from the "workspace state" debug line Seshat logs
# on every structural change (active panel, branch and panel counts, Focus
# Mode). It contains locations only, never listings.

H="$HOME_DIR"
mkdir -p "$H/Documents/Projects" "$H/Downloads"
echo notes > "$H/notes.txt"
echo code > "$H/Documents/Projects/x.rs"
echo iso > "$H/Downloads/big.iso"

log_text() { sed 's/\x1b\[[0-9;]*m//g' "$SESSION/seshat.log"; }
# grep -c reads all its input: grep -q would stop early and, under the
# harness's pipefail, fail the pipeline with a broken pipe on long logs.
log_has() { log_text | grep -c -- "$1" >/dev/null; }
state() { log_text | grep "workspace state" | tail -1; }
state_has() { state | grep -q -- "$1"; }
layout_now() { log_text | grep "workspace layout" | tail -1 | grep -q -- "$1"; }
# The latest "workspace geometry" line, logged when a reveal settles.
geometry_field() {
    log_text | grep "workspace geometry" | tail -1 | grep -o "$1=-\?[0-9]*" | cut -d= -f2
}
# The active panel lies entirely inside the visible workspace.
active_in_view() {
    local left right width
    left=$(geometry_field active_left); right=$(geometry_field active_right); width=$(geometry_field viewport)
    [[ -n "$left" && -n "$right" && -n "$width" ]] && ((left >= 0 && right <= width))
}
# Every shown panel header has the same height (a fork button or branch
# badge must not make one taller).
headers_equal() {
    local heights
    heights=$(log_text | grep "workspace geometry" | tail -1 | grep -o 'headers="[0-9,]*"' |
        sed 's/headers="//; s/"//' | tr ',' '\n' | grep -v '^0$' | sort -u | wc -l)
    ((heights == 1))
}
active_is() { state_has "location=Path(\"$1\")"; }

launch
check "starts at Home"                              active_is "$H"

# 1–3. Home → Documents → Projects (Home lists Documents, Downloads, notes.txt)
type_text gg; press Return 1
check "Enter opens Documents as a child panel"      active_is "$H/Documents"
type_text gg; press Return 1
check "Enter opens Projects inside Documents"       active_is "$H/Documents/Projects"
check "a single branch so far"                      state_has "branches=1 "

# 4. Back to Home: the existing panel, nothing created or destroyed
type_text hh
check "h returns to the existing Home panel"        active_is "$H"
check "moving left creates no panels"               state_has "panels=3 "

# 5–6. Downloads from Home: a second branch, Projects kept
type_text j; press Return 1
check "Downloads opens"                             active_is "$H/Downloads"
check "6. both branches coexist"                    state_has "branches=2 "
check "Downloads is not a child of Projects"        state_has "panels=4 "
shot two-branches
sleep 1   # let the reveal settle
check "panel headers are all the same height"       headers_equal
check "the active panel is fully in view"           active_in_view

# Side by side: Home spans both rows; three connectors; the other branch's
# panels are cards. A short window falls back to one row (decisions L2, L4).
check "branches are laid out side by side"          log_has "workspace layout rows=2 columns=3 hidden=0 cards=2 connectors=3"
swaycmd '[title="^Seshat$"] floating enable, resize set 1100 360'; sleep 1
check "a short window shows one branch row"         log_has "workspace layout rows=1 columns=2 hidden=1"
shot short-window
swaycmd '[title="^Seshat$"] floating disable'; sleep 1
check "a tall window shows both rows again"         layout_now "rows=2 "

# 7–8. Switch to the Projects branch (left at Home) and keep navigating it
type_text "["
check "[ restores the other branch where it was"    active_is "$H"
type_text ll
check "8. the other branch navigates independently" active_is "$H/Documents/Projects"
sleep 1
check "    the rightmost panel is fully in view"    active_in_view

# 9–11. Focus Mode, with spatial navigation and branch switching
type_text zz
check "9. zz enters Focus Mode"                     state_has "focus_mode=true"
shot focus-mode
type_text h
check "10. h moves to a neighbour while focused"    active_is "$H/Documents"
check "    Focus Mode is kept"                      state_has "focus_mode=true"
type_text "]"
check "11. ] switches branch while focused"         active_is "$H/Downloads"
check "    Focus Mode is kept across branches"      state_has "focus_mode=true"

# 12. Back to the multi-panel workspace, nothing lost
type_text zz
check "12. zz restores the workspace"               state_has "focus_mode=false"
check "    all panels and branches remain"          state_has "branches=2 panels=4 "
shot workspace-restored

# 13. Select files in two branches: notes.txt in Home, x.rs in Projects
type_text h; type_text G; type_text v; press Escape
type_text "["; type_text ll; type_text gg; type_text v; press Escape
shot cross-branch-selection

# 14. Copy them into Downloads, named explicitly; the selection spans two
# panels, so a confirmation lists the sources (Cancel is the default).
type_text :; type_text "copy ~/Downloads"; press Return 1
shot transfer-confirmation
press Tab; press space 1.5; focus_main
check "14. notes.txt copied from the Home panel"    test -e "$H/Downloads/notes.txt"
check "    x.rs copied from the Projects branch"    test -e "$H/Downloads/x.rs"
check "    sources are untouched"                   test -e "$H/Documents/Projects/x.rs"

# 15. The operation did not disturb the workspace
check "15. the active panel is unchanged"           active_is "$H/Documents/Projects"
check "    both branches remain"                    state_has "branches=2 "

# Persistence: quit and start again
swaycmd '[title="^Seshat$"] kill'
check "the app exits"                               wait_exit 10
check "the workspace was saved"                     test -s "$SESSION/state/seshat/workspace.toml"
launch
check "the workspace is restored on launch"         log_has "restored=true panels=4 branches=2"
check "the active panel is restored"                active_is "$H/Documents/Projects"
type_text "]"
check "restored branches keep their position"       active_is "$H"   # left at Home in step 13
type_text l
check "and their path"                              active_is "$H/Downloads"
shot restored

# Path completion on the command line. Without it, ~/Doc would open a
# folder called "Doc".
type_text :; type_text "open ~/Doc"; press Tab; shot completion-unique
press Return 1
check "Tab completes a unique folder"               active_is "$H/Documents"
# Two matches: Tab fills in "Do", then cycles Documents/, Downloads/.
type_text :; type_text "open ~/D"; press Tab; shot completion-matches
press Tab; press Tab; press Return 1
check "repeated Tab cycles through the matches"     active_is "$H/Downloads"
# A folder no panel shows is read in the background.
mkdir -p "$H/Archive/2024-photos"
type_text :; type_text "open ~/Archive/20"; press Tab 1; press Return 1
check "folders that are not open complete too"      active_is "$H/Archive/2024-photos"
swaycmd '[title="^Seshat$"] kill'
check "the app exits again"                         wait_exit 10

check "no panics or errors in the log"              not grep -qE "panicked|ERROR" "$SESSION/seshat.log"
