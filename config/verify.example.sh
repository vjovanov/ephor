#!/usr/bin/env bash
# Run the project's own check, so "fixed" is not the agent's opinion.
#
#     0   the check passed        → done
#     1   it did not              → back to the fix state with this output
#
# What the check *is* belongs to the project, not to ephor: a checkout that
# ships an executable `./check.sh` is taken at its word — that is the one
# hook, and whatever project-specific steps it aggregates live in the project.
# Only a checkout without one falls back to the ways a repository usually
# says so. Replace the fallback list with your project's one real command —
# a check that guesses is a check nobody trusts.
set -uo pipefail

: "${REPORT:?the state machine must pass {output.<name>.path}}"

mkdir -p "$(dirname "$REPORT")"

# Artifact paths are relative to the plan directory, which is where a program
# starts — so pin it absolute before any `cd`, or every write after the cd
# lands nowhere.
case "$REPORT" in /*) ;; *) REPORT="$PWD/$REPORT" ;; esac

# A person was asked something: that outranks running anything.
if [ -n "${FIX_REPORT:-}" ] && [ -s "$FIX_REPORT" ] &&
   question=$(grep -m1 -E '^[[:space:]]*NEEDS-HUMAN:' "$FIX_REPORT"); then
  { echo "# the fix asked a person a question"; echo; echo "$question"; } > "$REPORT"
  exit 2
fi

# `{meta.*}` only exists on the ticket ephor dispatched — a ticket the machine
# opened for itself inherits none, so the checkout is resolved rather than
# assumed. Programs run in the plan directory, and for the default
# `{workspace}/panta` layout the checkout is the git tree enclosing it.
if [ ! -d "${CHECKOUT:-}" ] || [ -z "${CHECKOUT##*\{*}" ]; then
  CHECKOUT=$(git rev-parse --show-toplevel 2>/dev/null || echo "..")
fi
cd "$CHECKOUT" || { echo "no checkout at $CHECKOUT" > "$REPORT"; exit 1; }

# Which verbs run, and in what order, is policy above the interface
# (§FS-006-project-interface.5): $EPHOR_CHECKS names them, newline-separated,
# and this script sequences what it is given. Composition stays here, in
# configuration, rather than inside ephor.
#
# Unset, it falls back to the project's own hook first — `./check.sh` outranks
# every guessed convention below, because the project wrote it to mean exactly
# "check me" — and then to the ways a repository usually says so.
checks=()
if [ -n "${EPHOR_CHECKS:-}" ]; then
  while IFS= read -r verb; do
    [ -n "$verb" ] && checks+=("$verb")
  done <<< "$EPHOR_CHECKS"
else
  checks=("./check.sh" "just check" "make check" "npm test" "cargo test --locked")
fi

for check in "${checks[@]}"; do
  tool=${check%% *}
  # `just check` needs a justfile as well as `just`; the same for the rest.
  # A verb ephor handed over is already the answer to "what checks this",
  # so it is run rather than second-guessed.
  if [ -z "${EPHOR_CHECKS:-}" ]; then
  case "$tool" in
    just)  [ -f justfile ] || [ -f Justfile ] || continue ;;
    make)  [ -f Makefile ] || continue ;;
    npm)   [ -f package.json ] || continue ;;
    cargo) [ -f Cargo.toml ] || continue ;;
    ./*)   [ -x "$tool" ] || continue ;;
  esac
  command -v "${tool#./}" >/dev/null 2>&1 || [ -x "$tool" ] || continue
  fi

  {
    echo "# $check"
    echo
    echo '```'
  } > "$REPORT"
  # Both streams: a test runner puts the failure on whichever it likes.
  if timeout 2h $check >>"$REPORT" 2>&1; then
    echo '```' >> "$REPORT"
    echo >> "$REPORT"
    echo "Passed." >> "$REPORT"
    exit 0
  fi
  echo '```' >> "$REPORT"
  echo >> "$REPORT"
  echo "Failed. The output above is what the check said." >> "$REPORT"
  exit 1
done

# No check found is not a pass, and it is not a failure of the fix either —
# it is a question for a person: what is this project's check? Exit 2 parks
# the ticket on that question rather than spending a retry to learn nothing.
{
  echo "NEEDS-HUMAN: what is this project's check command?"
  echo
  echo "Nothing in $CHECKOUT looked like one, so whether the fix holds is only"
  echo "the agent's word. Name the real command in ~/.config/ephor/verify.sh"
  echo "and move this ticket back to fix, or answer here and close it."
} > "$REPORT"
exit 2
