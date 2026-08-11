#!/usr/bin/env bash
set -euo pipefail

# Profile-guided-optimization build of the `ephor` release binary (§FS-002-release.3).
# This is a release tool, not part of the normal development build or the
# push/PR CI loop.
#
# Three phases: build an instrumented binary (`-Cprofile-generate`), run the hot
# read paths to produce `.profraw` profiles, merge them, then rebuild the
# release binary with `-Cprofile-use`. The training workload is this repository's
# own registry and cached feed — a representative real input — exercised through
# the commands that do the work: registry parse and validation, the feed render,
# and the JSON surfaces. Nothing in the training run touches the network:
# `refresh` is deliberately absent, so a training build never calls a forge.
#
# Output: target/release/ephor, optimized against the merged profile.
# Requires: the `llvm-tools-preview` rustup component (`llvm-profdata`).
#
# `cargo install ephor` does not run this — a plain `cargo build --release` has
# no profile to use and is LTO-only and behaviour-identical.

cd "$(dirname "$0")/.."
repo="$PWD"
pgo_dir="$repo/target/pgo-data"
profdata="$pgo_dir/merged.profdata"
state_dir="$repo/target/pgo-state"
host="$(rustc -vV | awk '/^host:/ { print $2 }')"

rustc_path() {
  local path="$1"
  if [[ "$host" == *windows* ]] && command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$path"
  else
    printf '%s\n' "$path"
  fi
}

# llvm-profdata ships in the active toolchain's llvm-tools-preview component.
llvm_profdata="$(find "$(rustc --print sysroot)" -type f -name 'llvm-profdata*' | head -n1)"
if [ -z "$llvm_profdata" ]; then
  echo "error: llvm-profdata not found — run: rustup component add llvm-tools-preview" >&2
  exit 1
fi

rm -rf "$pgo_dir" "$state_dir"
mkdir -p "$pgo_dir" "$state_dir"

pgo_dir_rustc="$(rustc_path "$pgo_dir")"
profdata_rustc="$(rustc_path "$profdata")"

echo "==> 1/3  build instrumented binary (-Cprofile-generate)"
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-generate=$pgo_dir_rustc" cargo build --release --locked

exe_suffix=""
case "$host" in
  *windows*) exe_suffix=".exe" ;;
esac
ephor="$repo/target/release/ephor$exe_suffix"

echo "==> 2/3  training run — the hot read paths against this repo"
# Exit codes are irrelevant here (`validate` exits 2 when a registry root is not
# checked out on this machine); we only want the code paths exercised. State is
# redirected into target/ so a training run never reads or writes the real cache.
set +e
for _ in 1 2 3; do
  EPHOR_HOME="$repo" XDG_STATE_HOME="$state_dir" "$ephor" list           >/dev/null 2>&1
  EPHOR_HOME="$repo" XDG_STATE_HOME="$state_dir" "$ephor" validate       >/dev/null 2>&1
  EPHOR_HOME="$repo" XDG_STATE_HOME="$state_dir" "$ephor" status --cached >/dev/null 2>&1
  EPHOR_HOME="$repo" XDG_STATE_HOME="$state_dir" "$ephor" status --cached --json >/dev/null 2>&1
  EPHOR_HOME="$repo" XDG_STATE_HOME="$state_dir" "$ephor" feed           >/dev/null 2>&1
  EPHOR_HOME="$repo" XDG_STATE_HOME="$state_dir" "$ephor" feed --json    >/dev/null 2>&1
done
set -e

# Fail loudly if the training loop produced no profiles — every command above
# was wrapped in `set +e`, so a totally-broken '$ephor' invocation would
# otherwise be hidden until `llvm-profdata` errored on an empty input.
shopt -s nullglob
profraws=("$pgo_dir"/*.profraw)
if [ ${#profraws[@]} -eq 0 ]; then
  echo "error: PGO training produced no .profraw files in $pgo_dir" >&2
  echo "       (the instrumented '$ephor' did not run successfully)" >&2
  exit 1
fi

"$llvm_profdata" merge -o "$profdata" "${profraws[@]}"

echo "==> 3/3  rebuild optimized (-Cprofile-use)"
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Cprofile-use=$profdata_rustc -Cllvm-args=-pgo-warn-missing-function" cargo build --release --locked

echo "==> done: $ephor"
"$ephor" --version
