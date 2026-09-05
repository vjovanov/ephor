#!/usr/bin/env bash
# The second shape a `work.headroom` verb comes in: a metered credential with a
# quota endpoint to ask (§FS-005-dispatch.29). `config/headroom.example.sh` is
# the first — a subscription credential that publishes no number, and answers
# `null` because that is the truth.
#
# Both ship beside ephor rather than inside it (§REQ-001-boundary.5). The
# contract is ephor's; the question is yours, and this file is where you change
# it. Bind it per pool, where a pool is the hand's provider as `ephor caps`
# shows it:
#
#     "work": { "headroom": { "north": "~/.config/ephor/headroom-north.sh" } }
#
# The token lives beside ephor's own secrets and never in the registry or the
# feed configuration, which are both tracked (§REQ-001-boundary.4).
set -uo pipefail

: "${EPHOR_ANSWER:?ephor names the answer file; run this through work.headroom}"

SECRETS="${EPHOR_SECRETS:-$HOME/config/secrets/ephor}"
TOKEN_FILE="$SECRETS/headroom-token"
QUOTA_URL="${HEADROOM_URL:-https://api.example/v1/usage}"

if [ ! -r "$TOKEN_FILE" ]; then
  # Not an error ephor should route around silently: exiting non-zero makes the
  # pool *unknown* with this sentence beside it, which is the degrade rule's
  # second half (§REQ-001-boundary.1). Unknown demotes nobody either way — this
  # only decides whether the reader is told why.
  echo "no credential at $TOKEN_FILE, so this pool cannot be asked" >&2
  exit 1
fi

answered=$(curl --silent --show-error --fail --max-time 20 \
  --header "Authorization: Bearer $(cat "$TOKEN_FILE")" \
  "$QUOTA_URL" 2>&1) || {
  echo "$QUOTA_URL could not be asked: $answered" >&2
  exit 1
}

# Shape the vendor's own document into windows. Change the paths on the right;
# everything else is the contract. `remaining` is a *fraction* of the window
# left, so a vendor reporting used-and-limit is divided here rather than in
# ephor — ephor consumes a report and derives nothing (§DA-009-headroom-vetoes).
#
# `// null` on each field is the load-bearing part: a field the vendor did not
# send is unknown, and unknown is never zero. Do not default it to 0, and do not
# default it to 1 — the first would veto a pool nobody said anything about, and
# the second would hide one that is genuinely spent.
windows=$(jq -c '
  [ .quotas[]? | {
      name:      (.window // "session"),
      remaining: (if (.limit // 0) > 0 and (.used != null)
                  then (1 - (.used / .limit))
                  else null end),
      resets_at: (.resets_at // null)
    } ]' <<<"$answered" 2>/dev/null) || windows=""

if [ -z "$windows" ] || [ "$windows" = "[]" ]; then
  echo "$QUOTA_URL answered nothing this reads as a window" >&2
  exit 1
fi

# The standard envelope, with the payload on `data` and nothing on standard
# output (§FS-006-project-interface.4, §FS-006-project-interface.3).
jq -n --argjson windows "$windows" \
  '{ v: 1, data: { windows: $windows } }' > "$EPHOR_ANSWER"
