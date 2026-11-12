# Performance run of Fuzzy Find with the overlay (Spec 4 §11).
# Not part of scripts/check.sh: timings are noisy. Run on an idle machine:
#
#   SESHAT_RELEASE=1 SESHAT_STALL_MONITOR=1 scripts/ui-session.sh scripts/ui/fuzzy-perf.sh [artifact-dir]
#
# Searches a small folder (200 files) and a large tree (100 folders of
# 1,000 files: 100,100 entries) by typing into the overlay, as a user
# would (wtype delivers each query in one go, so each query is one
# search). Times are measured inside Seshat from starting a search to the
# results being on screen, so they leave out the search field's 50 ms
# typing delay. The summary is printed and saved as fuzzy-perf-summary.txt.
# Results: docs/architecture/search.md.

H="$HOME_DIR"
SUMMARY="$ARTIFACTS/fuzzy-perf-summary.txt"
: > "$SUMMARY"

log_text() { sed 's/\x1b\[[0-9;]*m//g' "$SESSION/seshat.log"; }
mark() { log_text | wc -l; }
report() { echo "$1" | tee -a "$SUMMARY"; }
# grep -c reads all its input (see workspace.sh).
log_has_since() { (($(log_text | tail -n +"$1" | grep -c -- "$2" || true) > 0)); }

# stats <label> <from-line> <message> <field>: count, median, max (ms).
stats() {
    log_text | tail -n +"$2" | grep -- "$3" | grep -o "$4=[0-9]*" | cut -d= -f2 | sort -n |
        awk -v label="$1" '
            { v[NR] = $1 }
            END {
                if (NR == 0) { printf "%-40s no samples\n", label; exit }
                p50 = v[int((NR - 1) * 0.50) + 1]
                printf "%-40s n=%-3d median %8.2f ms   max %8.2f ms\n",
                       label, NR, p50 / 1000, v[NR] / 1000
            }' | tee -a "$SUMMARY"
}
stalls() {
    local gaps
    gaps=$(log_text | tail -n +"$1" | grep "main loop stalled" | grep -o "gap_ms=[0-9]*" | cut -d= -f2 | sort -n || true)
    report "$(printf '%-40s %s' "main-loop stalls over 100 ms" \
        "$(if [[ -z "$gaps" ]]; then echo none; else echo "$(wc -l <<<"$gaps"), longest $(tail -1 <<<"$gaps") ms"; fi)")"
}
memory() {
    local pid; pid=$(pgrep -x seshat | head -1)
    report "$(printf '%-40s %s' "memory, $1" "$(grep VmRSS "/proc/$pid/status" | awk '{printf "%.0f MiB", $2 / 1024}')")"
}

# ── Fixtures ────────────────────────────────────────────────────────────
mkdir -p "$H/small" "$H/big"
(cd "$H/small" && seq 200 | sed 's/^/file-/' | xargs touch)
for d in $(seq -w 0 99); do
    mkdir "$H/big/d$d"
    (cd "$H/big/d$d" && seq -w 0 999 | sed 's/^/f/' | xargs touch)
done

launch
check "Seshat starts"                                  seshat_running
memory "at start"

# ── A small folder ──────────────────────────────────────────────────────
chord ctrl p; type_text "~/small/" 1; press Return 1
from=$(mark)
for query in f12 file-19 le-7; do
    chord ctrl p; sleep 0.5; type_text "$query" 1; press Escape
done
report ""
report "Small folder (200 files), three queries:"
stats "  first results on screen" "$from" "first results shown" elapsed_us
stats "  search finished" "$from" "fuzzy find results" elapsed_us
check "small folder: results appeared"                 log_has_since "$from" "first results shown"

# ── A large tree ────────────────────────────────────────────────────────
chord ctrl p; type_text "~/big/" 1; press Return 1
from=$(mark)
# Opening the overlay walks the tree; the query is typed once it has.
chord ctrl p; sleep 2; type_text "d42f999" 1
report ""
report "Large tree (100,100 entries):"
stats "  walk when the overlay opens" "$from" "fuzzy find walked" elapsed_us
stats "  first results on screen" "$from" "first results shown" elapsed_us
stats "  rank and show the query" "$from" "fuzzy find results" elapsed_us
memory "after walking 100,100 entries"
cached=$(mark)
# The walk is cached for the session: new queries only rank.
press BackSpace 1; press BackSpace 1; press BackSpace 1; type_text 12 1
report ""
report "Large tree, new queries on the cached entries:"
stats "  rank and show" "$cached" "fuzzy find results" elapsed_us
press Escape
stalls "$from"

check "large tree: results appeared"                   log_has_since "$from" "fuzzy find results"
swaycmd '[title="^Seshat$"] kill'
check "the app exits"                                  wait_exit 10
check "no panics or errors in the log"                 not grep -qE "panicked|ERROR" "$SESSION/seshat.log"
