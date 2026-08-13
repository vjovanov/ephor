#!/usr/bin/env bash
# Collect what a red gate actually failed on, as an artifact for the agent that
# works the ticket after it (§FS-005-dispatch.8).
#
# ephor writes the item into the plan as metadata, so a state machine hands
# this script `{meta.project}`, `{meta.source}` and the rest as environment;
# `{output.<name>.path}` names where the answer goes. rhei picks the next state
# from the exit code:
#
#     0   the failures are in $REPORT      → work the ticket
#     3   the gate is green                → nothing to fix
#     75  the gate is still running        → wait and ask again (EX_TEMPFAIL)
#     1   the forge could not say          → work the ticket anyway; $REPORT
#                                            says why, and the agent can look
#
# Copy it, point a state's `program.command` at it, and change what it asks the
# forge — the shape is the point, not the questions.
set -uo pipefail

: "${PROJECT:?the state machine must pass {meta.project}}"
: "${SOURCE:?the state machine must pass {meta.source}}"
: "${ITEM:?the state machine must pass {meta.id}}"
: "${REPORT:?the state machine must pass {output.<name>.path}}"

mkdir -p "$(dirname "$REPORT")"

# The gate as it is now, not as it was when the ticket was written: this runs
# minutes or hours later, and on a poll it runs repeatedly.
ephor refresh "$PROJECT" --quiet >/dev/null 2>&1 || true
gate=$(ephor feed --json --project "$PROJECT" 2>/dev/null |
       jq -c --arg id "$ITEM" 'map(select(.id == $id)) | .[0].raw.gate // {}')
running=$(jq -r '[.repos[]?.running] | add // 0' <<<"$gate")
failed=$(jq  -r '[.repos[]?.failed]  | add // 0' <<<"$gate")

if [ "$failed" -eq 0 ] && [ "$running" -gt 0 ]; then
  echo "$ITEM: $running job(s) still running, nothing failed yet" >&2
  exit 75
fi

if [ "$failed" -eq 0 ]; then
  {
    echo "# $ITEM: nothing failed"
    echo
    echo "The gate has no failing jobs. Whatever is holding this change up is"
    echo "not something a checkout can fix."
    jq -r '.blockers[]? | "- " + .' <<<"$gate"
  } > "$REPORT"
  exit 3
fi

# The expensive question, asked once, by the source that reported the item.
if ephor failures --project "$PROJECT" --source "$SOURCE" \
                  --repo "${REPO:-}" --number "${NUMBER:-}" > "$REPORT" 2>/tmp/ephor-failures.$$; then
  rm -f "/tmp/ephor-failures.$$"
  # This is also the shipped `failures` verb of the gate seam
  # (§FS-006-project-interface.6), so where it is run as a summons it answers
  # in structure as well as in prose: the report stays what a reader reads,
  # and $EPHOR_ANSWER is what a program reads
  # (§FS-006-project-interface.4).
  if [ -n "${EPHOR_ANSWER:-}" ]; then
    printf '{"v":1,"summary":%s,"failures":[{"job":%s,"repo":%s,"log":%s}]}' \
      "\"$failed failing job(s)\"" \
      "\"the gate of ${REPO:-the project}\"" \
      "\"${REPO:-}\"" \
      "\"$REPORT\"" > "$EPHOR_ANSWER"
  fi
  exit 0
fi

{
  echo
  echo "## the forge could not say what failed"
  echo
  echo "\`ephor failures\` did not answer. What it said:"
  echo
  sed 's/^/    /' "/tmp/ephor-failures.$$"
  echo
  echo "The gate reports $failed failing job(s). Find them yourself before"
  echo "changing anything."
} >> "$REPORT"
rm -f "/tmp/ephor-failures.$$"
exit 1
