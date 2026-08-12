#!/usr/bin/env bash
# Land what the cause tickets fixed: push, and let the forge run the gate
# again. It commits nothing — every fix commits its own work.
#
# This is the one state that touches the world. It runs only after every cause
# ticket in the plan reached a successful terminal state — a cause that ends
# `cancelled` never satisfies the join, so a half-fixed branch is never pushed.
#
#     0   pushed (or there was nothing to push)   → wait for the new gate
#     2   a dirty tree, the wrong branch, or a     → the ticket parks for a
#         refused push                               person
#     1   the checkout is not there                → a person should look
#
# Restarting individual failed jobs without a new commit is a forge feature and
# has no CLI here; pushing is what makes the gate run, so pushing is the
# restart. If your forge can retry a job, add it under RESTART below.
set -uo pipefail

: "${REPORT:?the state machine must pass {output.<name>.path}}"
mkdir -p "$(dirname "$REPORT")"

# Artifact paths are relative to the plan directory, which is where a program
# starts — so pin it absolute before any `cd`, or every write after the cd
# lands nowhere.
case "$REPORT" in /*) ;; *) REPORT="$PWD/$REPORT" ;; esac

# `{meta.*}` belongs to the ticket ephor dispatched; a ticket the machine wrote
# for itself is given the same fields by plan-join, but resolve anyway.
if [ ! -d "${CHECKOUT:-}" ] || [ -z "${CHECKOUT##*\{*}" ]; then
  CHECKOUT=$(git rev-parse --show-toplevel 2>/dev/null || echo "..")
fi
cd "$CHECKOUT" || { echo "no checkout at $CHECKOUT" > "$REPORT"; exit 1; }

branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
want=${BRANCH:-}

{
  echo "# landing on $branch in $CHECKOUT"
  echo
} > "$REPORT"

# The ticket names a branch; being on a different one means somebody moved the
# checkout under the work, and pushing then pushes the wrong thing.
if [ -n "$want" ] && [ -z "${want##*\{*}" ]; then want=""; fi
if [ -n "$want" ] && [ "$branch" != "$want" ]; then
  {
    echo "NEEDS-HUMAN: the checkout is on '$branch' but the ticket is about '$want'."
    echo
    echo "Nothing was pushed. Put the checkout back on the ticket's branch, or"
    echo "close this ticket if the work moved."
  } >> "$REPORT"
  exit 2
fi

# Nothing is committed here. Each fix commits its own work — that is the
# contract of the state that made the change — so a dirty tree at this point is
# something nobody claimed, and `git add -A` before a push would sweep up
# whatever else was lying in the checkout. Stop and show it instead.
dirty=$(git status --porcelain)
if [ -n "$dirty" ]; then
  {
    echo "NEEDS-HUMAN: the checkout has changes that no ticket committed."
    echo
    echo "Nothing was pushed. Commit them yourself, or discard them, then move"
    echo "this ticket on:"
    echo
    echo '```'
    echo "$dirty"
    echo '```'
  } >> "$REPORT"
  exit 2
fi

ahead=$(git rev-list --count "@{upstream}..HEAD" 2>/dev/null || echo "unknown")
if [ "$ahead" = "0" ]; then
  echo "Nothing to push: the branch matches its upstream." >> "$REPORT"
  exit 0
fi

echo "## push ($ahead commit(s) ahead)" >> "$REPORT"
echo '```' >> "$REPORT"
if ! git push >> "$REPORT" 2>&1; then
  echo '```' >> "$REPORT"
  {
    echo
    echo "NEEDS-HUMAN: the push was refused — see the output above."
    echo "Nothing else was attempted."
  } >> "$REPORT"
  exit 2
fi
echo '```' >> "$REPORT"

# RESTART — where a forge can re-run failed jobs on the same commit, do it
# here. Pushing already makes the gate run on the new commit, so this is only
# for retrying a job that failed on a commit you are not replacing.
if [ -n "${RESTART:-}" ]; then
  echo >> "$REPORT"
  echo "## restart" >> "$REPORT"
  echo '```' >> "$REPORT"
  eval "$RESTART" >> "$REPORT" 2>&1 || echo "(restart command failed)" >> "$REPORT"
  echo '```' >> "$REPORT"
fi

echo >> "$REPORT"
echo "Pushed. The gate runs again on the new head; the next state waits for it." >> "$REPORT"
exit 0
