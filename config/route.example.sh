#!/usr/bin/env bash
# Turn what an agent wrote into a transition.
#
# An agent cannot choose its own next state — `rhei run` owns transitions — so
# every branch in the machine is a program, and this is that program. It reads
# the first line the agent marked and exits with it:
#
#     2   NEEDS-HUMAN: <question>   → the ticket parks in a gating state until
#                                     a person answers it in the plan
#     3   NOT-OURS: <what died>     → the change was never the problem; restart
#                                     the gate instead of fixing anything
#                                     (§FS-005-dispatch.11)
#     1   VERDICT: unsound          → round again, while the budget lasts
#     0   VERDICT: sound, or no
#         verdict line at all       → carry on
#
# Deliberately literal. A router that interpreted the artifact would be another
# opinion nobody asked for; the point of putting the marker on its own line is
# that acting on it takes no judgement.
set -uo pipefail

: "${ARTIFACT:?the state machine must pass {input.<name>.path}}"

if [ ! -s "$ARTIFACT" ]; then
  echo "nothing written to $ARTIFACT" >&2
  exit 1
fi

# A question for a person outranks a verdict: an agent that asked one has said
# it cannot finish, whatever else it wrote.
if question=$(grep -m1 -E '^[[:space:]]*NEEDS-HUMAN:' "$ARTIFACT"); then
  echo "$question" >&2
  exit 2
fi

# Likewise a failure that is not this change's: there is nothing here to be
# sound or unsound about, so the verdict below does not apply to it.
if foreign=$(grep -m1 -E '^[[:space:]]*NOT-OURS:' "$ARTIFACT"); then
  echo "$foreign" >&2
  exit 3
fi

verdict=$(grep -m1 -E '^[[:space:]]*VERDICT:' "$ARTIFACT" || true)
if [ -z "$verdict" ]; then
  exit 0
fi
echo "$verdict" >&2
grep -qiE '^[[:space:]]*VERDICT:[[:space:]]*sound' <<<"$verdict"
