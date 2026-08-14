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

# llvm-profdata ships in the active toolchain's llvm-tools-preview component,
# at a known path under the sysroot. Looked for there rather than searched for:
# a distro toolchain's sysroot is `/usr`, and a `find` over it walks the whole
# system, prints permission errors from every directory it cannot read, and
# may return an `llvm-profdata` that does not match this compiler.
sysroot="$(rustc --print sysroot)"
llvm_profdata=""
for candidate in "$sysroot"/lib/rustlib/*/bin/llvm-profdata*; do
  if [ -x "$candidate" ]; then
    llvm_profdata="$candidate"
    break
  fi
done
# A toolchain without the component can still be profiled where the system
# carries an `llvm-profdata` of the same LLVM major — the profile format is
# tied to the major release — so say what was found rather than refusing.
if [ -z "$llvm_profdata" ] && command -v llvm-profdata >/dev/null 2>&1; then
  rustc_llvm="$(rustc -vV | awk '/^LLVM version:/ { split($3, v, "."); print v[1] }')"
  system_llvm="$(llvm-profdata --version | awk '/LLVM version/ { split($3, v, "."); print v[1] }')"
  if [ -n "$rustc_llvm" ] && [ "$rustc_llvm" = "$system_llvm" ]; then
    llvm_profdata="$(command -v llvm-profdata)"
    echo "note: using the system llvm-profdata (LLVM $system_llvm, matching rustc)"
  fi
fi
if [ -z "$llvm_profdata" ]; then
  echo "error: no llvm-profdata matching this toolchain." >&2
  echo "  rustup: rustup component add llvm-tools-preview" >&2
  echo "  otherwise: install an llvm-profdata of the same LLVM major as rustc" >&2
  echo "  (rustc reports LLVM $(rustc -vV | awk '/^LLVM version:/ { print $3 }'))" >&2
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

echo "==> 2/3  training run — the self pass, which is the workload"
# `ephor doctor --self-only` is the same walk the release self-checks with
# (§FS-002-release.3): it builds a project of its own in a temporary place and
# runs the binary against it, so the profile is gathered from refresh, matter
# merging, the summons executor, git, dispatch and the plan reader rather than
# from whatever commands happened to be listed here.
#
# It is also the only workload that is hermetic. The commands this used to run
# read `~/.config/ephor` — the *building* machine's own registry — which is a
# private site on a laptop and, on a release runner, a file that is not there:
# every command exited early and the profile was gathered from error paths
# (§FS-001-forge-interface.5). The self pass reads nothing of anybody's.
#
# Each run spawns the binary many times over, and every child is instrumented
# too: `-Cprofile-generate` writes one `.profraw` per process into the same
# directory, so the children's profiles are part of the merge.
set +e
for _ in 1 2 3; do
  XDG_STATE_HOME="$state_dir" "$ephor" doctor --self-only >/dev/null 2>&1
  # The pure-read surfaces the self pass does not reach, and a parse of the
  # published schemas — cheap, and they need nothing on disk.
  XDG_STATE_HOME="$state_dir" "$ephor" schema registry >/dev/null 2>&1
  XDG_STATE_HOME="$state_dir" "$ephor" schema manifest >/dev/null 2>&1
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
