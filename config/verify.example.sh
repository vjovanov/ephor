#!/usr/bin/env bash
# Run the project's own check, so "fixed" is not the agent's opinion.
#
#     0   the check passed        → done
#     1   it did not              → back to the fix state with this output
#
# What the check *is* belongs to the project, not to ephor: this looks for the
# ways a repository usually says so, in the checkout the ticket names. Replace
# the list with your project's one real command — a check that guesses is a
# check nobody trusts.
set -uo pipefail

: "${REPORT:?the state machine must pass {output.<name>.path}}"

mkdir -p "$(dirname "$REPORT")"

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

for check in "just check" "make check" "npm test" "cargo test --locked" "./check.sh"; do
  tool=${check%% *}
  # `just check` needs a justfile as well as `just`; the same for the rest.
  case "$tool" in
    just)  [ -f justfile ] || [ -f Justfile ] || continue ;;
    make)  [ -f Makefile ] || continue ;;
    npm)   [ -f package.json ] || continue ;;
    cargo) [ -f Cargo.toml ] || continue ;;
    ./*)   [ -x "$tool" ] || continue ;;
  esac
  command -v "${tool#./}" >/dev/null 2>&1 || [ -x "$tool" ] || continue

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

# No check found is not a pass: say so, and let the machine decide what that
# is worth rather than reporting a green nobody earned.
{
  echo "# no check to run"
  echo
  echo "Nothing in $CHECKOUT looked like this project's check. Name the real"
  echo "one in ~/.config/ephor/verify.sh — until then, whether the fix holds"
  echo "is only the agent's word."
} > "$REPORT"
exit 1
