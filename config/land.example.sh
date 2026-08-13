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
# Pushing is what makes the gate run on a *new* commit. Re-running a gate that
# failed for reasons that were never the change's is a different move, with a
# ticket and a state of its own — see `config/restart-gate.example.sh`
# (§FS-005-dispatch.11) — so it does not belong here. The RESTART hook below
# stays for the odd forge that wants a nudge after a push lands.
#
# FORCE_WITH_LEASE=1 lands a branch that was replayed rather than added to
# (§FS-004-quick-actions.6): a rebase rewrites the commits the remote already
# has, so the push cannot fast-forward. It is `--force-with-lease` and never
# `--force` — the lease is what refuses when somebody else pushed in the
# meantime, which is the whole difference between replacing your own commits
# and replacing theirs. Only the state that follows a rebase sets it.
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

# Every repository under the checkout, not just the one at its root: a
# multi-repo workspace holds several, and the fix for a pull request lands in
# whichever one owns the code. Pushing the root alone pushes nothing.
repos=()
for candidate in . */; do
  [ -e "$candidate/.git" ] || continue
  repos+=("${candidate%/}")
done
if [ ${#repos[@]} -eq 0 ]; then
  echo "no git repository in $CHECKOUT" >> "$REPORT"
  exit 1
fi

{
  echo "# landing from $CHECKOUT"
  echo
  echo "Repositories: ${repos[*]}"
  echo
} > "$REPORT"

# Nothing is committed here. Each fix commits its own work — that is the
# contract of the state that made the change — so a dirty tree at this point is
# something nobody claimed, and `git add -A` before a push would sweep up
# whatever else was lying in the checkout. Stop and show it instead.
dirty=""
for repo in "${repos[@]}"; do
  changes=$(git -C "$repo" status --porcelain)
  [ -n "$changes" ] && dirty="$dirty
--- $repo
$changes"
done
if [ -n "$dirty" ]; then
  {
    echo "NEEDS-HUMAN: the checkout has changes that no ticket committed."
    echo
    echo "Nothing was pushed. Commit them yourself, or discard them, then move"
    echo "this ticket on:"
    echo '```'
    echo "$dirty"
    echo '```'
  } >> "$REPORT"
  exit 2
fi

# A replayed branch is not an addition to what the remote has, so its push has
# to say so. The lease is what keeps this from being a `--force`.
push_args=()
lease=""
if [ "${FORCE_WITH_LEASE:-0}" = "1" ]; then
  push_args=(--force-with-lease)
  lease=" (--force-with-lease)"
  echo "Pushing with --force-with-lease: this branch was replayed." >> "$REPORT"
  echo >> "$REPORT"
fi

pushed=0
for repo in "${repos[@]}"; do
  branch=$(git -C "$repo" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
  ahead=$(git -C "$repo" rev-list --count "@{upstream}..HEAD" 2>/dev/null || echo "none")

  # The ticket names a branch. A repository sitting on a different one is not
  # this work — the branch a poly-repo workspace tracks is the same in each.
  if [ -n "${BRANCH:-}" ] && [ -z "${BRANCH##*\{*}" ]; then BRANCH=""; fi
  if [ -n "${BRANCH:-}" ] && [ "$branch" != "$BRANCH" ]; then
    echo "- $repo: on '$branch', not the ticket's '$BRANCH' — left alone" >> "$REPORT"
    continue
  fi
  if [ "$ahead" = "none" ]; then
    echo "- $repo: no upstream for '$branch' — left alone" >> "$REPORT"
    continue
  fi
  if [ "$ahead" = "0" ]; then
    echo "- $repo: already up to date with its upstream" >> "$REPORT"
    continue
  fi

  echo >> "$REPORT"
  echo "## $repo — push $ahead commit(s) on $branch$lease" >> "$REPORT"
  echo '```' >> "$REPORT"
  if ! git -C "$repo" push "${push_args[@]}" >> "$REPORT" 2>&1; then
    echo '```' >> "$REPORT"
    {
      echo
      echo "NEEDS-HUMAN: pushing $repo was refused — see the output above."
      echo "Repositories pushed before it: $pushed."
      if [ -n "$lease" ]; then
        echo "The lease refused it: the remote branch moved since this checkout"
        echo "last fetched it, so forcing now would replace somebody else's work."
      else
        echo "Nothing was forced."
      fi
    } >> "$REPORT"
    exit 2
  fi
  echo '```' >> "$REPORT"
  pushed=$((pushed + 1))
done

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
if [ "$pushed" -eq 0 ]; then
  echo "Nothing needed pushing." >> "$REPORT"
else
  echo "Pushed $pushed repositor$([ $pushed -eq 1 ] && echo y || echo ies)." >> "$REPORT"
  echo "The gate runs again on the new heads; the next state waits for it." >> "$REPORT"
fi
exit 0
