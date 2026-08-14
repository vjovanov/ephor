#!/usr/bin/env bash
# The `restart` verb of ephor's gate (§FS-006-project-interface.6): re-run the
# failing gate and everything downstream of it, committing nothing
# (§FS-005-dispatch.11).
#
# A runner died, a cache was unreachable, a dependency shipped something broken.
# There is nothing to diagnose and nothing to commit — the change was never the
# problem — so this touches no checkout and asks no model anything. It re-runs
# the failed jobs of every workflow run that has one, which is what carries the
# jobs that never started along with them: a workflow whose first job died
# never reached the rest, and `--failed` starts those too.
#
# In:  EPHOR_REPO, EPHOR_NUMBER, EPHOR_BRANCH name the matter; with none of
#      them, the branch this checkout is standing on.
# Out: the seam's exit semantics — 0 asked for, 75 still running (ask again
#      later, and not a retry spent), non-zero refused.
set -uo pipefail

# shellcheck source=scripts/gate-lib.sh
. "$(dirname "$0")/gate-lib.sh"

PARKED=75

repo=$(gate_repo)
if [ -z "$repo" ]; then
  gate_unreachable "this checkout" "nothing says which repository, and it has no GitHub remote"
  exit 1
fi

ref=$(gate_ref "$repo")
subject=$(gate_subject "$repo" "$ref")
if [ -z "$ref" ]; then
  gate_unreachable "$subject" "nothing says which branch or commit"
  exit 1
fi

if ! runs=$(gate_runs "$repo" "$ref"); then
  gate_unreachable "$subject" "the forge did not answer"
  exit 1
fi

counts=$(printf '%s' "$runs" | gate_counts)
failed=$(jq -r .failed <<<"$counts")
running=$(jq -r .running <<<"$counts")

# Still running is not a failed restart, and a loop that treated it as one
# would spend a retry to learn nothing (§FS-005-dispatch.11).
if [ "$failed" -eq 0 ] && [ "$running" -gt 0 ]; then
  echo "$subject: $running job(s) still running, nothing has failed yet."
  jq -n --arg subject "$subject" --arg running "$running" \
    '{v: 1, summary: ($running + " job(s) of " + $subject + " are still running")}' | gate_answer
  exit "$PARKED"
fi

if [ "$failed" -eq 0 ]; then
  echo "$subject: nothing failed, so there is nothing to restart."
  jq -n --arg subject "$subject" \
    '{v: 1, summary: ("the gate of " + $subject + " is green; nothing to restart")}' | gate_answer
  exit 0
fi

# One workflow run carries many jobs, and `--failed` re-runs all of that run's
# failures at once — so the work is per run, not per job, and a run named twice
# would start its jobs twice.
workflow_runs=()
while IFS= read -r run; do
  [ -n "$run" ] && workflow_runs+=("$run")
done < <(
  printf '%s' "$runs" | gate_failing |
    jq -r '.[] | .url | capture("/actions/runs/(?<run>[0-9]+)") | .run' | sort -u
)

if [ "${#workflow_runs[@]}" -eq 0 ]; then
  echo "$subject: $failed job(s) failed, but none of them names a workflow run to restart." >&2
  jq -n --arg subject "$subject" \
    '{v: 1, summary: ("nothing in the gate of " + $subject + " names a workflow run to restart")}' |
    gate_answer
  exit 1
fi

asked=0
refused=()
for run in "${workflow_runs[@]}"; do
  if gh run rerun "$run" --repo "$repo" --failed; then
    asked=$((asked + 1))
  else
    refused+=("$run")
  fi
done

# Half a restart is not one: the gate stays red where the forge said no, and
# reporting the whole thing as asked for would leave the next round waiting on
# a run that was never started.
if [ "${#refused[@]}" -gt 0 ]; then
  printf 'The forge refused to restart run %s\n' "${refused[@]}" >&2
  jq -n --arg subject "$subject" --arg asked "$asked" --argjson refused "$(printf '%s\n' "${refused[@]}" | jq -R . | jq -s .)" \
    '{v: 1,
      summary: ("the forge refused " + ($refused | length | tostring) +
                " of the restarts on " + $subject + "; " + $asked + " asked for"),
      data: {refused: $refused}}' | gate_answer
  exit 1
fi

echo "$subject: restarted the failed jobs of $asked workflow run(s). Nothing was committed."
jq -n --arg subject "$subject" --arg asked "$asked" \
  '{v: 1, summary: ("restarted the failed jobs of " + $asked + " workflow run(s) on " + $subject)}' |
  gate_answer
exit 0
