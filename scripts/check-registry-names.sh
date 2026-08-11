#!/usr/bin/env bash
set -euo pipefail

# Pre-release package-name guard for §FS-002-release.
# Every name this project claims must be either still available or already
# owned by this repository — a name taken by someone else between two releases
# is found here rather than by a failing `cargo publish`.
#
# ephor publishes one package today (the `ephor` crate). Add a line per
# registry as bindings land.

ua="ephor-release-name-check/0.1"
repo_pattern='github.com[/:]vjovanov/ephor'
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

http_get() {
  local url="$1"
  local out="$2"
  curl -sS -L -A "$ua" -o "$out" -w '%{http_code}' "$url"
}

metadata_mentions_repo() {
  local file="$1"
  python3 - "$file" "$repo_pattern" <<'PY'
import json
import re
import sys

path, pattern = sys.argv[1], sys.argv[2]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

haystack = json.dumps(data, sort_keys=True).lower()
sys.exit(0 if re.search(pattern, haystack) else 1)
PY
}

check_claimed_json_name() {
  local registry="$1"
  local name="$2"
  local url="$3"
  local out="$tmpdir/${registry}-${name}.json"
  local code

  code="$(http_get "$url" "$out")"
  case "$code" in
    200)
      if metadata_mentions_repo "$out"; then
        echo "ok: $registry/$name is owned by this project"
      else
        echo "error: $registry/$name is already taken by another project" >&2
        echo "       $url" >&2
        return 1
      fi
      ;;
    404)
      echo "ok: $registry/$name is available"
      ;;
    *)
      echo "error: could not query $registry/$name (HTTP $code)" >&2
      echo "       $url" >&2
      return 1
      ;;
  esac
}

check_claimed_json_name "crates.io" "ephor" "https://crates.io/api/v1/crates/ephor"
