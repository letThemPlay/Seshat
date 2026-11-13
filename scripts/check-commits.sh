#!/usr/bin/env bash
# Fails if a commit subject in <base>..<head> doesn't follow Conventional
# Commits (docs/development/commits.md). Merge commits are skipped, and
# git's own `Revert "…"` subjects are allowed. An empty range passes.
#   scripts/check-commits.sh <base> [<head>]   (head defaults to HEAD)
set -euo pipefail

if (($# < 1 || $# > 2)); then
    echo "usage: $0 <base> [<head>]" >&2
    exit 2
fi
base=$1
head=${2:-HEAD}

types='feat|fix|refactor|perf|docs|test|build|ci|chore|style|revert'
pattern="^($types)(\([a-z0-9-]+\))?!?: [^ [:upper:]]"
max=72

subjects=$(git log --no-merges --format=%s "$base..$head")
failures=0
total=0
while IFS= read -r subject; do
    [[ -z "$subject" ]] && continue
    total=$((total + 1))
    if [[ "$subject" == 'Revert "'* ]]; then
        continue
    fi
    if ! [[ "$subject" =~ $pattern ]]; then
        echo "FAIL not <type>[(<scope>)][!]: <summary>: $subject"
        failures=$((failures + 1))
    elif [[ "$subject" == *. ]]; then
        echo "FAIL ends with a full stop: $subject"
        failures=$((failures + 1))
    elif ((${#subject} > max)); then
        echo "FAIL longer than $max characters (${#subject}): $subject"
        failures=$((failures + 1))
    fi
done <<<"$subjects"

if ((failures > 0)); then
    echo
    echo "Expected: <type>[(<scope>)][!]: <summary>, at most $max characters"
    echo "  types:   ${types//|/, }"
    echo "  scope:   optional, lowercase kebab-case, e.g. (workspace)"
    echo "  summary: imperative, starts lowercase, no trailing full stop"
    echo "See docs/development/commits.md. Reword with git commit --amend"
    echo "or git rebase, then push with --force-with-lease."
    exit 1
fi
echo "PASS: $total commit subject(s) follow Conventional Commits"
