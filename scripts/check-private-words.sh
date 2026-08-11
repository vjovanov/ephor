#!/usr/bin/env bash
set -euo pipefail

# Refuse to commit or publish anything containing a word from a private word
# list (§FS-001-forge-interface.5).
#
# The list is deliberately NOT in this repository. A committed list of the
# names you are trying to keep out of a public repository publishes exactly
# those names — the check would leak what it protects. So this script carries
# the mechanism and your machine carries the words:
#
#   $EPHOR_PRIVATE_WORDS   an explicit path
#   .private-words         repo root, gitignored
#   ~/.config/ephor/private-words
#
# One extended-regex pattern per line; `#` comments and blank lines ignored.
# For example (write your own — these are shapes, not names):
#
#   # employer identifiers
#   \bacmecorp\b
#   @acme\.example\.com
#   internal-build-tool
#
# With no list present the check passes and says so: a fresh clone, a
# contributor, and CI have no list and must not fail for lacking one. The
# enforcement is local by design — pre-commit on your machine, and the release
# gate before anything is published.
#
# Usage:
#   check-private-words.sh              scan the whole working tree
#   check-private-words.sh FILE...      scan these files (pre-commit passes them)

find_list() {
  if [ -n "${EPHOR_PRIVATE_WORDS:-}" ]; then
    printf '%s' "$EPHOR_PRIVATE_WORDS"
    return
  fi
  local root
  root="$(git rev-parse --show-toplevel 2>/dev/null || printf '.')"
  for candidate in "$root/.private-words" "${XDG_CONFIG_HOME:-$HOME/.config}/ephor/private-words"; do
    if [ -f "$candidate" ]; then
      printf '%s' "$candidate"
      return
    fi
  done
}

list="$(find_list)"
if [ -z "$list" ]; then
  echo "notice: no private word list found; skipping (see $0 for where it goes)"
  exit 0
fi

# A list inside the repository must be ignored by git. If it is tracked, the
# next commit publishes it, which is the failure this whole script exists to
# prevent.
if git ls-files --error-unmatch "$list" >/dev/null 2>&1; then
  echo "error: the private word list $list is tracked by git — add it to .gitignore" >&2
  exit 1
fi

pattern="$(grep -vE '^\s*(#|$)' "$list" | paste -sd'|' -)"
if [ -z "$pattern" ]; then
  echo "notice: private word list $list is empty; skipping"
  exit 0
fi

if [ "$#" -gt 0 ]; then
  targets=("$@")
else
  # Whole tree: everything git tracks, minus the list itself.
  mapfile -t targets < <(git ls-files)
fi
[ "${#targets[@]}" -gt 0 ] || exit 0

# `-I` skips binaries. The word list is never scanned — it contains the words
# by definition.
matches="$(grep -InE -- "$pattern" "${targets[@]}" 2>/dev/null | grep -v "^${list#./}:" || true)"
if [ -n "$matches" ]; then
  count="$(printf '%s\n' "$matches" | wc -l | tr -d ' ')"
  echo "error: $count line(s) contain a private word:" >&2
  printf '%s\n' "$matches" | head -n 40 >&2
  [ "$count" -gt 40 ] && echo "  … and $((count - 40)) more" >&2
  echo "       these must not reach a public repository (§FS-001-forge-interface.5)" >&2
  exit 1
fi

echo "ok: no private words in ${#targets[@]} file(s)"
