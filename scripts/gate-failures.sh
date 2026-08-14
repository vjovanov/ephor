#!/usr/bin/env bash
# The `failures` verb of ephor's gate (§FS-006-project-interface.6): what
# actually failed, as the failing job, its log, and the error where it can be
# had. This is the expensive question — one request per failing job — so it is
# asked on demand and never as part of a refresh (§FS-001-forge-interface.1).
#
# In:  EPHOR_REPO, EPHOR_NUMBER, EPHOR_BRANCH name the matter; with none of
#      them, the branch this checkout is standing on.
# Out: exit 0 and failures[] in $EPHOR_ANSWER — empty where the gate is green,
#      which is an answer and not a failure of the verb. Non-zero only where
#      the forge could not be asked.
set -uo pipefail

# shellcheck source=scripts/gate-lib.sh
. "$(dirname "$0")/gate-lib.sh"

# How much of a failing job's log is the error. A gate log is mostly the build
# that worked; what a reader wants is the end of it, and what an agent can hold
# in a dossier is less than that (§FS-004-quick-actions.4).
TRACE_LINES=${EPHOR_GATE_TRACE_LINES:-40}

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

failing=$(printf '%s' "$runs" | gate_failing)
count=$(jq -r 'length' <<<"$failing")

if [ "$count" -eq 0 ]; then
  echo "$subject: nothing failed."
  jq -n --arg subject "$subject" \
    '{v: 1, summary: ("the gate of " + $subject + " has no failing jobs"), failures: []}' |
    gate_answer
  exit 0
fi

# The log of each failing job, trimmed to its tail. `--log-failed` already
# drops the steps that passed; the two leading columns repeat the job and step
# name on every line, the first token of what is left is the forge's timestamp
# rather than anything the error said, and the colour a compiler wrote for a
# terminal is noise in a dossier nobody will read in one.
esc=$(printf '\033')
failures='[]'
while IFS=$'\t' read -r id name url; do
  trace=$(gh run view --repo "$repo" --job "$id" --log-failed 2>/dev/null |
          cut -f3- | sed -e 's/^[^ ]*Z //' -e "s/${esc}\[[0-9;]*[a-zA-Z]//g" |
          tail -n "$TRACE_LINES")
  failures=$(jq -c --arg job "$name" --arg repo "$repo" --arg url "$url" --arg trace "$trace" \
    '. + [{job: $job, repo: $repo, url: $url} +
          (if ($trace | length) > 0 then {trace: $trace} else {} end)]' <<<"$failures")
done < <(jq -r '.[] | [(.id | tostring), .name, .url] | @tsv' <<<"$failing")

jq -n --argjson failures "$failures" --arg subject "$subject" --arg count "$count" \
  '{v: 1,
    summary: ($count + " failing job(s) in the gate of " + $subject),
    failures: $failures}' | gate_answer

echo "$subject: $count failing job(s)."
jq -r '.[] | "  - " + .job + "  " + .url' <<<"$failures"
exit 0
