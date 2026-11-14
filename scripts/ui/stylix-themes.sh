# Visual check of the Stylix integration (spec 5 §14): Seshat started with
# each theme the Home Manager module generates for the five scenarios, with
# a branch of panels open and a file selected, and a screenshot of each.
# Sourced by scripts/ui-session.sh. The screenshots are for a person to
# review against the checklist in docs/development/verification-results.
#
#   nix build ./nix/tests#stylix-themes -o /tmp/stylix-themes
#   STYLIX_THEMES=/tmp/stylix-themes scripts/ui-session.sh scripts/ui/stylix-themes.sh /tmp/stylix-shots

THEMES=${STYLIX_THEMES:?set STYLIX_THEMES to the output of nix build ./nix/tests#stylix-themes}

mkdir -p "$HOME_DIR/Documents/Notes" "$HOME_DIR/Pictures" "$HOME_DIR/Projects"
echo report > "$HOME_DIR/Documents/report.pdf"
echo todo > "$HOME_DIR/Documents/Notes/todo.txt"
echo ideas > "$HOME_DIR/Documents/Notes/ideas.md"
echo readme > "$HOME_DIR/readme.md"

log_text() { sed 's/\x1b\[[0-9;]*m//g' "$SESSION/seshat.log"; }
applied() { log_text | grep "applying theme" | tail -1 | grep -q -- "$1"; }

for scenario in dark light accent fonts overrides; do
    file="$THEMES/$scenario.toml"
    check "$scenario: the generated theme is accepted" "$ROOT/target/$PROFILE_DIR/seshat" --check-theme "$file"
    # What the Home Manager wrapper does: always pass the theme file.
    printf '#!/usr/bin/env bash\nexec "%s" --theme-file "%s" "$@"\n' \
        "$ROOT/target/$PROFILE_DIR/seshat" "$(readlink -f "$file")" >"$SESSION/seshat-$scenario"
    chmod +x "$SESSION/seshat-$scenario"
    BIN="$SESSION/seshat-$scenario"
    rm -rf "$SESSION/state/seshat" # start from Home each time

    launch
    shot "$scenario-1-home"              # Home selected in the navigation pane
    press Down; press Return 0.8         # Documents: a second panel
    press Down; press Return 0.8         # Notes: a third, with connectors
    press v; press j                     # Visual Mode: select two files
    shot "$scenario-2-panels"
    check "$scenario: Seshat starts with the launch theme" applied "choice=Provided"
    press Escape; press Escape
    kill "$SESHAT_PID"; wait_exit 5 || true
done

check "no panics or errors in the log" not grep -qaE 'panicked|ERROR' "$SESSION/seshat.log"
