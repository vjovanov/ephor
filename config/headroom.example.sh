#!/usr/bin/env bash
# Report what one provider pool has left, for `work.headroom`
# (§FS-005-dispatch.29).
#
# This is the seam's worked example, and it ships beside ephor rather than
# inside it on purpose (§REQ-001-boundary.5): how any one vendor is asked for
# its usage is volatile — a REPL slash-command today, an endpoint tomorrow,
# nothing at all for a subscription credential — and a literal compiled into
# ephor would go stale in ephor's release cycle rather than in yours. What ephor
# fixes is the contract below; the questions are yours.
#
# Bind one command per pool, where a pool is the hand's provider as the roster
# shows it (`ephor caps`), or its agent id where the profile names no provider:
#
#     "work": {
#       "headroom": {
#         "north": "~/.config/ephor/headroom-north.sh",
#         "south": "~/.config/ephor/headroom-south.sh"
#       }
#     }
#
# ephor runs it under `refresh`, on the same freshness discipline as every other
# source, and never in front of a dispatch. It is summoned exactly as every
# other command this interface names (§FS-006-project-interface.3):
#
#     in     the EPHOR_* environment, and $EPHOR_ANSWER naming a file to write
#     out    the exit code, and that file — never standard output, which is
#            yours to log in (§AR-002-summons.3)
#
# The answer is the standard envelope with the payload on `data`
# (§FS-006-project-interface.4):
#
#     { "v": 1, "data": { "windows": [
#         { "name": "session", "remaining": 0.12, "resets_at": "…" },
#         { "name": "weekly",  "remaining": null, "resets_at": "…" } ] } }
#
#     name         what the provider calls this window. Free text.
#     remaining    the fraction of it left, 0.0 to 1.0. **Null or absent is
#                  UNKNOWN, and unknown is never zero.** A pool's effective
#                  remaining is the least of the windows that named a number,
#                  and an unknown window is simply not among them.
#     resets_at    RFC 3339, when the window refills.
#
# Say `null` when you do not know. It is the honest answer and it is also the
# safe one: unknown demotes nobody, while a guessed `0` would pass over a hand
# the person chose deliberately. Where a vendor publishes no number at all,
# bind nothing — the ledger channel still records a refusal ephor's own spawn
# was given, with no configuration at all.
#
# Anything short of a clean answer degrades to unknown with the reason kept
# beside the pool, and never stops a dispatch (§REQ-001-boundary.1): a non-zero
# exit, output that will not parse, an empty `windows`, an unbound pool.
#
# Copy this, point `work.headroom` at your copy, and replace `ask_the_vendor`
# with whatever your credential can actually be asked. The shape is the point.
set -uo pipefail

: "${EPHOR_ANSWER:?ephor names the answer file; run this through work.headroom}"

# Where the vendor's own numbers come from. Two shapes are usual:
#
#   * a command that prints usage as JSON — parse it here with `jq`;
#   * a metered API with a quota endpoint — `curl` it with a token from
#     ~/config/secrets/ephor/, which is where ephor keeps its own.
#
# A subscription credential signed in through a vendor's OAuth usually has
# neither, and shows its usage only to a person in a REPL. That is the ordinary
# case this seam is built for, and the answer for it is `null` below.
ask_the_vendor() {
  # Replace this. Print one line per window: "<name> <remaining|null> <reset>".
  printf 'session null %s\n' "$(date -u -d '+5 hours' +%Y-%m-%dT%H:%M:%SZ)"
}

# One JSON window per line the vendor gave us. `remaining` is emitted unquoted
# so that the literal `null` stays null rather than becoming the string "null",
# which would be a number ephor could not read rather than an honest silence.
windows() {
  local name remaining reset first=1
  while read -r name remaining reset; do
    [ -n "$name" ] || continue
    [ "$first" = 1 ] || printf ',\n'
    first=0
    printf '      { "name": "%s", "remaining": %s, "resets_at": "%s" }' \
      "$name" "${remaining:-null}" "$reset"
  done
  printf '\n'
}

reported=$(ask_the_vendor | windows)

# An empty report is unknown, and ephor reads it as unknown either way — but
# exiting non-zero says *this could not be asked* rather than *this was asked
# and had nothing to say*, and the reason is what shows up beside the pool.
if [ -z "${reported//[[:space:]]/}" ]; then
  echo "the vendor could not be asked for its usage" >&2
  exit 1
fi

cat > "$EPHOR_ANSWER" <<ENVELOPE
{ "v": 1,
  "data": {
    "windows": [
$reported
    ]
  }
}
ENVELOPE
