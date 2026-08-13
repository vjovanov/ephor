#!/usr/bin/env bash
# Restart the jobs that failed for reasons that were never this change's, and
# every gate left red underneath them (§FS-005-dispatch.11).
#
# A runner died, a mirror was unreachable, a dependency shipped something
# broken. There is nothing to diagnose and nothing to commit — those jobs need
# running again. So this state commits nothing, touches no checkout, and asks
# no model anything: it is handed the list and it runs it.
#
#     0   something was restarted            → wait for the new run
#     3   nothing needed restarting          → wait anyway; the gate moved on
#     2   a person is needed: no list, a      → the ticket parks in the gating
#         line that is not in the format,       state until somebody answers
#         no restart command, the budget
#         spent, or the forge refused
#     1   the gate could not be read at all  → a person should look
#
# THE LIST. `$JOBS` is written by whichever state concluded the failure was not
# this change's — triage, or analyze and critique through `NOT-OURS:` — and its
# format is exact, because a program that guesses at a job name restarts the
# wrong thing. One job per line, two whitespace-separated fields:
#
#     <repo> <job-key>      that one job, in that repository's gate
#     <repo> -              that repository's whole gate
#
# Blank lines and lines whose first character is `#` are ignored. Anything else
# is refused by line number — a malformed list is not a list to improvise
# around, and the ticket parks for a person rather than restarting a guess.
#
# DOWNSTREAM. A gate that spans several repositories fails downward: the one
# whose job died takes the tree with it, and the gates below never start. So
# after the named jobs, every repository the forge reports as not green — and
# that the list did not already name — has its whole gate restarted too.
# A repository whose jobs are still running is left alone: it is already
# running.
#
# HOW. There is no forge-neutral way to re-run a job, so the command is yours:
# set GATE_RESTART to a shell command and it is run once per line of work with
#
#     EPHOR_GATE_PROJECT   the project, as the registry names it
#     EPHOR_GATE_REPO      the repository whose gate is being restarted
#     EPHOR_GATE_JOB       the job key, or empty for a whole-gate restart
#     EPHOR_GATE_NUMBER    the pull request number
#     EPHOR_GATE_ITEM      the item id
#
# in its environment. For a Bitbucket-hosted gate driven through gdev-cli:
#
#     GATE_RESTART: 'gdev-cli casablanca review --platform bitbucket
#                      --project G --repo "$EPHOR_GATE_REPO"
#                      --pr "$EPHOR_GATE_NUMBER"'
#
# Left unset, this exits 2 and says so in the report rather than pretending the
# gate was restarted — which is the honest answer, and the one that gets the
# command configured.
set -uo pipefail

: "${PROJECT:?the state machine must pass {meta.project}}"
: "${ITEM:?the state machine must pass {meta.id}}"
: "${JOBS:?the state machine must pass {input.jobs.path}}"
: "${REPORT:?the state machine must pass {output.<name>.path}}"

mkdir -p "$(dirname "$REPORT")"
case "$REPORT" in /*) ;; *) REPORT="$PWD/$REPORT" ;; esac

max=${MAX_RESTARTS:-2}

{
  echo "# restart the gate on $ITEM"
  echo
} > "$REPORT"

# ── the budget ───────────────────────────────────────────────────────────────
# Counted from the plan rather than from `visits`, because each round opens a
# *new* ticket and a per-state visit counter resets with it. The plan is the
# only thing that remembers across rounds — which is the reason a restart gets
# a ticket of its own in the first place.
plan="${RHEI_PLAN_PATH:-}"
if [ -n "$plan" ] && [ -f "$plan" ]; then
  spent=$(awk '
    /^[[:space:]]*(```|~~~)/ { fence = !fence; next }
    fence { next }
    /^[[:space:]]*\*\*State:\*\*[[:space:]]*(restart-gate|restarted)[[:space:]]*\r?$/ { n++ }
    END { print n + 0 }
  ' "$plan")
  if [ "$spent" -gt "$max" ]; then
    {
      echo "NEEDS-HUMAN: the gate has been restarted $((spent - 1)) time(s) already and keeps failing."
      echo
      echo "Restarting it again is not going to be the fix. Something is wrong"
      echo "with the infrastructure itself — look at it, then move this ticket"
      echo "on or close the plan."
    } >> "$REPORT"
    exit 2
  fi
  echo "Restarts on this item so far, this one included: $spent of $max." >> "$REPORT"
  echo >> "$REPORT"
fi

# ── the list ─────────────────────────────────────────────────────────────────
# Declared `optional` on the state so a ticket that arrives without one is
# refused *here*, with a message, rather than by a transition nobody can read.
if [ ! -s "$JOBS" ]; then
  {
    echo "NEEDS-HUMAN: nothing said which jobs to restart."
    echo
    echo "The state that decided this failure was not the change's fault must"
    echo "leave the list at \`$JOBS\`, one job per line:"
    echo
    echo "    <repo> <job-key>      that one job"
    echo "    <repo> -              that repository's whole gate"
    echo
    echo "Restarting a gate on a guess is worse than not restarting it."
  } >> "$REPORT"
  exit 2
fi

named=()      # "repo job" pairs, in the order the list gave them
named_repos=" "
while IFS= read -r line || [ -n "$line" ]; do
  lineno=$((${lineno:-0} + 1))
  trimmed=${line%%$'\r'}
  trimmed=${trimmed#"${trimmed%%[![:space:]]*}"}
  case "$trimmed" in ''|'#'*) continue ;; esac

  # Exactly two fields. A third is not a job key with a space in it — it is a
  # line nobody checked, and this is the state that must not improvise.
  read -r repo job extra <<<"$trimmed"
  if [ -z "$repo" ] || [ -z "$job" ] || [ -n "$extra" ]; then
    {
      echo "NEEDS-HUMAN: line $lineno of the job list is not in the format."
      echo
      echo "    $line"
      echo
      echo "Every line is \`<repo> <job-key>\`, or \`<repo> -\` for a whole gate."
      echo "Nothing was restarted."
    } >> "$REPORT"
    exit 2
  fi
  named+=("$repo $job")
  case "$named_repos" in *" $repo "*) ;; *) named_repos="$named_repos$repo " ;; esac
done < "$JOBS"

if [ ${#named[@]} -eq 0 ]; then
  {
    echo "NEEDS-HUMAN: the job list at \`$JOBS\` names no jobs."
    echo
    echo "It has lines, but every one of them is blank or a comment."
  } >> "$REPORT"
  exit 2
fi

# ── and what is red underneath them ──────────────────────────────────────────
# As the gate is *now*, not as it was when the ticket was written: triage ran
# minutes ago and the forge has kept moving.
ephor refresh "$PROJECT" --quiet >/dev/null 2>&1 || true
gate=$(ephor feed --json --project "$PROJECT" 2>/dev/null |
       jq -c --arg id "$ITEM" 'map(select(.id == $id)) | .[0].raw.gate // empty')

if [ -z "$gate" ]; then
  {
    echo "The feed has no gate for $ITEM, so there is nothing to aim a restart at."
    echo "Either the item is gone or the refresh failed."
  } >> "$REPORT"
  exit 1
fi

# Not green: something failed, or nothing ran at all because whatever it waits
# on failed first. Both want the same treatment.
red=" "
while IFS= read -r repo; do
  [ -n "$repo" ] || continue
  red="$red$repo "
done < <(jq -r '
  .repos[]?
  | select((.failed // 0) > 0
           or ((.passed // 0) + (.failed // 0) + (.running // 0)) == 0)
  | .repo
' <<<"$gate")

known=" "
while IFS= read -r repo; do
  [ -n "$repo" ] || continue
  known="$known$repo "
done < <(jq -r '.repos[]?.repo' <<<"$gate")

# Minutes pass between the state that wrote the list and this one. A named job
# whose repository is green by now has already been re-run — by somebody, or by
# the forge itself — and restarting it would turn a passing gate red for
# another hour.
work=()
stale=()
unknown=()
for entry in "${named[@]}"; do
  read -r repo _ <<<"$entry"
  case "$red" in
    *" $repo "*) work+=("$entry"); continue ;;
  esac
  # Green now is a job somebody already re-ran. A repository the gate does not
  # mention at all is a different thing — the list does not match the forge,
  # and there is nothing to aim at.
  case "$known" in
    *" $repo "*) stale+=("$entry") ;;
    *) unknown+=("$entry") ;;
  esac
done
running=${#work[@]}

downstream=()
for repo in $red; do
  case "$named_repos" in *" $repo "*) continue ;; esac
  downstream+=("$repo -")
done
[ ${#downstream[@]} -gt 0 ] && work+=("${downstream[@]}")

{
  echo "## the gate now"
  echo
  jq -r '.repos[]? | "- \(.repo): \(.passed // 0) passed, \(.failed // 0) failed, \(.running // 0) running"' <<<"$gate"
  echo
  echo "Named by the list: ${#named[@]}, of which $running still red."
  echo "Gates red underneath them and not named: ${#downstream[@]}."
  if [ ${#stale[@]} -gt 0 ]; then
    echo
    echo "Green again since the list was written, so left alone:"
    printf '    %s\n' "${stale[@]}"
  fi
  if [ ${#unknown[@]} -gt 0 ]; then
    echo
    echo "Named by the list but not in the gate at all:"
    printf '    %s\n' "${unknown[@]}"
  fi
  echo
} >> "$REPORT"

if [ ${#work[@]} -eq 0 ]; then
  if [ ${#unknown[@]} -gt 0 ]; then
    {
      echo "NEEDS-HUMAN: the job list names nothing the gate reports."
      echo
      echo "Every repository in it is one the forge does not mention, so there"
      echo "is nothing to restart and no way to tell what was meant. The list"
      echo "was written from a report that no longer matches the gate."
    } >> "$REPORT"
    exit 2
  fi
  {
    echo "Nothing is red any more. Whatever failed has already been re-run, or"
    echo "the gate moved on by itself. Nothing was restarted."
  } >> "$REPORT"
  exit 3
fi

if [ -z "${GATE_RESTART:-}" ]; then
  {
    echo "NEEDS-HUMAN: no GATE_RESTART command is configured, so nothing can restart the gate."
    echo
    echo "Re-running a job is a forge feature with no neutral CLI. Set"
    echo "\`GATE_RESTART\` on the \`restart-gate\` state to the command your forge"
    echo "wants — it is run once per line below, with EPHOR_GATE_PROJECT,"
    echo "EPHOR_GATE_REPO, EPHOR_GATE_JOB, EPHOR_GATE_NUMBER and"
    echo "EPHOR_GATE_ITEM in its environment. Until then, restart these by hand:"
    echo
    printf '    %s\n' "${work[@]}"
  } >> "$REPORT"
  exit 2
fi

# `{meta.*}` belongs to the ticket ephor dispatched; a ticket the machine wrote
# for itself is given the same fields by plan-join, but a placeholder that never
# resolved is worse than an empty one — it would be handed to the forge as text.
for name in NUMBER REPO; do
  value=${!name:-}
  [ -z "${value##*\{*}" ] && printf -v "$name" '%s' ""
done

export EPHOR_GATE_PROJECT="$PROJECT"
export EPHOR_GATE_NUMBER="${NUMBER:-}"
export EPHOR_GATE_ITEM="$ITEM"

echo "## restarting" >> "$REPORT"
echo >> "$REPORT"

restarted=0
refused=()
for entry in "${work[@]}"; do
  read -r repo job <<<"$entry"
  [ "$job" = "-" ] && job=""
  export EPHOR_GATE_REPO="$repo"
  export EPHOR_GATE_JOB="$job"

  echo "### $repo ${job:-(whole gate)}" >> "$REPORT"
  echo '```' >> "$REPORT"
  if eval "$GATE_RESTART" >> "$REPORT" 2>&1; then
    restarted=$((restarted + 1))
    echo '```' >> "$REPORT"
  else
    echo '```' >> "$REPORT"
    refused+=("$entry")
  fi
  echo >> "$REPORT"
done

# A refusal is not a flake to shrug at — the restart is the whole point of this
# state, and one that half worked leaves the gate half red.
if [ ${#refused[@]} -gt 0 ]; then
  {
    echo "NEEDS-HUMAN: the forge refused the restart for:"
    echo
    printf '    %s\n' "${refused[@]}"
    echo
    echo "Restarted before it: $restarted. The output of each attempt is above."
  } >> "$REPORT"
  exit 2
fi

{
  echo "Restarted $restarted: $running named job(s), ${#downstream[@]} gate(s) red underneath."
  echo
  echo "Nothing was committed and nothing was pushed — the change was not the"
  echo "problem. The next round waits for the run this started."
} >> "$REPORT"
exit 0
