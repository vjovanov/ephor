#!/usr/bin/env bash
set -euo pipefail

# Publication gate for §FS-002-release.4: refuse to ship while the tree still
# violates §FS-001-forge-interface.5 (no site-specific data in the repository).
#
# Two checks:
#   1. No employer/vendor identifier appears in shipped source or tests.
#   2. The crate package carries no real registry or feed configuration —
#      `cargo package --list` must contain nothing under config/ or docs/
#      beyond examples.
#
# Run it directly (`bash scripts/check-no-site-specific.sh`) or via
# `.github/workflows/pre-release-checks.yml`. §RM-001-forge-interface tracks the
# work that makes this pass; until then it is expected to fail, and it is what
# stops a release from going out early.

status=0

echo "==> 1/2  no private words in the tree"
# The words live on your machine, not here: a committed list of the names you
# are keeping out of a public repository publishes them. See the script.
bash "$(dirname "$0")/check-private-words.sh" || status=1

echo "==> 2/2  the crate package carries no real configuration"
if ! packaged="$(cargo package --list --allow-dirty 2>/dev/null)"; then
  echo "error: cargo package --list failed; cannot verify package contents" >&2
  exit 1
fi

# Only *.example.* configuration and the generic AGENTS.md templates may ship.
# Anything else under config/ or docs/ is either a person's real registry or the
# inherited documentation set.
leaked="$(printf '%s\n' "$packaged" \
  | grep -E '^(config|docs)/' \
  | grep -vE '\.example\.|^config/templates/' || true)"
if [ -n "$leaked" ]; then
  echo "error: the crate package would ship non-example configuration/docs:" >&2
  printf '  %s\n' $leaked >&2
  echo "       add them to Cargo.toml 'exclude', or replace them with examples" >&2
  status=1
else
  echo "ok: package contains no real configuration or inherited docs"
fi

exit "$status"
