# Ephor task runner
#
# `just check` is exactly the CI gate (.github/workflows/ci.yml). Clippy is
# deliberately not part of it — CI does not run clippy, and a local gate that is
# stricter than CI trains you to ignore it. Run `just lint` when you want it.

default: check

# Run the Rust test suite
test:
    cargo test --all-targets --locked

# Run the changelog-script tests
test-python:
    python3 -m unittest discover -s tests -p 'test_*.py'

# Build with warnings as errors, as CI does
build:
    RUSTFLAGS=-Dwarnings cargo build --all-targets --locked

# Format sources
format:
    cargo fmt --all

# Check formatting without modifying
format-check:
    cargo fmt --all -- --check

# Hold the source to the boundary law: no product literal outside its adapter,
# and a core layer that reaches nothing above it (§REQ-001-boundary.5)
boundary:
    python3 scripts/check_boundary.py

# Validate the grund tree (citations resolve, canonical formatting)
grund:
    grund check
    grund fmt --check

# Lint with clippy, warnings are errors (not part of the CI gate)
lint:
    cargo clippy --all-targets -- -D warnings

# Render docs/manual.md as a standalone page (needs pandoc; not part of the gate)
manual-page out="target/manual.html":
    python3 scripts/manual-page.py {{out}}

# Full gate, matching CI: format + build + tests + boundary + grund
check: format-check build test test-python boundary grund

# Everything a release verifies, without publishing anything
pre-release:
    bash scripts/check-no-site-specific.sh
    bash scripts/check-registry-names.sh
    bash scripts/pgo-build.sh

# Install the ephor binary into ~/.cargo/bin
install:
    cargo install --path . --locked

# Link canonical skills into global agent skill directories
link-skills:
    ./ai/link-global-skills.sh
