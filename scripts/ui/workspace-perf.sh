# Performance run of the spatial workspace (spatial spec §14, §16.8).
# Not part of scripts/check.sh: timings are noisy. Run on an idle machine:
#
#   SESHAT_RELEASE=1 SESHAT_STALL_MONITOR=1 scripts/ui-session.sh scripts/ui/workspace-perf.sh [artifact-dir]
#
# A saved workspace of 50 branches and 211 panels (1,000 files in each
# branch tip) is restored, then the script switches branches, toggles Focus
# Mode, moves between panels and reloads a 100,000-entry folder while keys
# are pressed. Timings come from Seshat's debug log (counts and durations,
# never file names). The summary is printed and saved as perf-summary.txt.
# Targets and results: docs/review/performance.md.

H="$HOME_DIR"
SUMMARY="$ARTIFACTS/perf-summary.txt"
: > "$SUMMARY"

log_text() { sed 's/\x1b\[[0-9;]*m//g' "$SESSION/seshat.log"; }
mark() { log_text | wc -l; }
report() { echo "$1" | tee -a "$SUMMARY"; }

# stats <label> <from-line> <message> <field>: count, median, p95, max (ms).
stats() {
    log_text | tail -n +"$2" | grep -- "$3" | grep -o "$4=[0-9]*" | cut -d= -f2 | sort -n |
        awk -v label="$1" '
            { v[NR] = $1 }
            END {
                if (NR == 0) { printf "%-34s no samples\n", label; exit }
                p50 = v[int((NR - 1) * 0.50) + 1]; p95 = v[int((NR - 1) * 0.95) + 1]
                printf "%-34s n=%-4d median %8.2f ms   p95 %8.2f ms   max %8.2f ms\n",
                       label, NR, p50 / 1000, p95 / 1000, v[NR] / 1000
            }' | tee -a "$SUMMARY"
}
count() { log_text | tail -n +"$1" | grep -c -- "$2" || true; }
memory() {
    local pid; pid=$(pgrep -x seshat | head -1)
    report "$(printf '%-34s %s' "memory, $1" "$(grep VmRSS "/proc/$pid/status" | awk '{printf "%.0f MiB", $2 / 1024}')")"
}

# ── Fixtures ────────────────────────────────────────────────────────────
mkdir -p "$H/big"
(cd "$H/big" && seq 100000 | sed 's/^/file-/' | xargs touch)

declare -a PATHS PARENTS TIPS
declare -A KIDS
NEXT=1
add() {
    LAST=$NEXT; NEXT=$((NEXT + 1))
    PATHS[$LAST]="$1"; PARENTS[$LAST]="$2"
    [[ -n "$2" ]] && KIDS[$2]+="$LAST, "
    mkdir -p "$1"
}
add "$H/w" ""; root=$LAST
for i in {0..9}; do
    add "$H/w/a$i" "$root"; a=$LAST
    for j in {0..4}; do
        add "$H/w/a$i/b$j" "$a"; p=$LAST
        for k in 0 1 2; do add "${PATHS[$p]}/t$k" "$p"; p=$LAST; done
        TIPS+=("$p")
        (cd "${PATHS[$p]}" && seq 1000 | sed 's/^/f/' | xargs touch)
    done
done
STATE="$SESSION/state/seshat"; mkdir -p "$STATE"
{
    echo "version = 1"
    echo "active_branch = $NEXT"
    echo "focus_mode = false"
    echo "retain_focus_on_branch_switch = true"
    echo "roots = [$root]"
    for ((id = 1; id < NEXT; id++)); do
        echo "[[panels]]"
        echo "id = $id"
        echo "location = { path = \"${PATHS[$id]}\" }"
        [[ -n "${PARENTS[$id]:-}" ]] && echo "parent = ${PARENTS[$id]}"
        kids="${KIDS[$id]:-}"; echo "children = [${kids%, }]"
    done
    n=0
    for tip in "${TIPS[@]}"; do
        echo "[[branches]]"
        echo "id = $((NEXT + n))"
        echo "tip = $tip"
        echo "cursor = $tip"
        echo "created = $((n + 1))"
        echo "last_active = $((n + 1))"
        n=$((n + 1))
    done
} > "$STATE/workspace.toml"

# ── Run ─────────────────────────────────────────────────────────────────
report "Seshat workspace performance run, $(date -u +%Y-%m-%dT%H:%MZ), $(nproc) CPUs, build: $([[ -n "${SESHAT_RELEASE:-}" ]] && echo release || echo debug)"
launch
sleep 2
check "the large workspace is restored"        bash -c "sed 's/\x1b\[[0-9;]*m//g' '$SESSION/seshat.log' | grep -q 'restored=true panels=211 branches=50'"
memory "restored, idle"

m=$(mark)
type_text "$(printf ']%.0s' {1..50})" 2
stats "render, branch switch (])"     "$m" "workspace rendered" elapsed_us
report "$(printf '%-34s %s' "folder reads, 50 switches" "$(count "$m" "listing requested")")"
memory "after visiting every branch"

m=$(mark)
type_text "$(printf ']%.0s' {1..50})" 2
stats "render, branch switch, 2nd round" "$m" "workspace rendered" elapsed_us
report "$(printf '%-34s %s' "folder reads, 2nd round" "$(count "$m" "listing requested")")"

m=$(mark)
type_text "$(printf 'z%.0s' {1..40})" 2
stats "render, Focus Mode toggle (zz)" "$m" "workspace rendered" elapsed_us
focus_reads=$(count "$m" "listing requested")
report "$(printf '%-34s %s' "folder reads, 20 Focus toggles" "$focus_reads")"
check "Focus Mode reads no folders"            test "$focus_reads" -eq 0

m=$(mark)
type_text "$(printf 'hl%.0s' {1..20})" 2
stats "render, h and l"                "$m" "workspace rendered" elapsed_us

m=$(mark)
type_text "$(printf 'j%.0s' {1..40})" 2
stats "render, j (cursor in 1,000 rows)" "$m" "workspace rendered" elapsed_us

stats "save, 211 panels"               1 "workspace saved" elapsed_us

# A 100,000-entry folder: first display, then live reloads while typing.
m=$(mark)
type_text :; type_text "open ~/big"; press Return 6
stats "model swap, 100,000 rows"       "$m" "panel view updated rows=100000 " elapsed_us
memory "with the 100,000-entry folder"
m=$(mark)
( for n in {1..15}; do touch "$H/big/new-$n"; sleep 0.3; done ) &
toucher=$!
for n in {1..20}; do press j 0.25; done
wait "$toucher"   # only the toucher: Seshat runs in the background too
sleep 4
stats "reload swap, 100,000 rows"      "$m" "panel view updated rows=1000[0-9][0-9] " elapsed_us
report "$(printf '%-34s %s' "reloads while typing" "$(count "$m" "panel view updated rows=1000[0-9][0-9] ")")"
stalls=$(log_text | tail -n +"$m" | grep -c "main loop stalled" || true)
report "$(printf '%-34s %s' "main-loop stalls > 100 ms" "$stalls")"
longest=$(log_text | tail -n +"$m" | grep "main loop stalled" | grep -o "gap_ms=[0-9]*" | cut -d= -f2 | sort -n | tail -1)
report "$(printf '%-34s %s' "longest stall" "${longest:-0} ms")"

swaycmd '[title="^Seshat$"] kill'
check "the app exits"                          wait_exit 10
check "no panics or errors in the log"         not grep -qE "panicked|ERROR" "$SESSION/seshat.log"
