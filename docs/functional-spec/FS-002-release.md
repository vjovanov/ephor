# FS-002-release: ephor releases from a tag, with a changelog entry per change

Versions are semver, and a version exists exactly when a `vX.Y.Z` tag does. The
version in `Cargo.toml` and the tag agree or the release refuses to run. Between
releases the manifest carries the *next* version with a `-dev` suffix, which is
not a version in that sense and is never publishable.

## 1. Changelog

[docs/changelog.md](../changelog.md) holds `## Unreleased` and the most
recent release inline; older releases move one-per-file under
`docs/changelog/` with a one-line pointer, so the common question — what
changed lately — is one file deep. Sections per release are the
Keep-a-Changelog set. Every pull request adds a bullet under `## Unreleased`
naming its own number, checked in CI.

## 2. Cutting a release

Promoting `## Unreleased` into a numbered release, bumping the manifest, and
tagging is done by workflow, not by hand: a patch release on a schedule when
main has shipped observable changes and its CI is green, and a minor release on
demand. Each first runs the whole release on a candidate branch with publishing
disabled, and only fast-forwards main if that dry run passed.

## 3. Artifacts

A release publishes the crate and a self-checked binary per supported target,
each built profile-guided, archived with its `sha256`, and attached to a GitHub
release whose notes are the changelog section for that version. Re-running a
partially-failed release skips what already exists rather than failing.

**Self-checked means the binary did its job, not that it started.** What every
artifact is held to is `doctor`'s self pass ([§FS-010-doctor.3](FS-010-doctor.md#3-two-passes-the-site-and-ephor-itself)): it builds a
project of its own and walks the seams against it, so a binary that links,
prints its version and cannot refresh a source is caught here rather than by
the first person to install it. A check that only asks for `--version` tests
the argument parser.

**And it is what the profile is trained on.** A profile-guided build is only
as good as the workload it profiled, so the training run is the same self
pass rather than a list of commands assembled for the occasion — one workload,
exercising the paths a reader actually waits on. It must also be hermetic: a
training run that reads the registry of whoever is building reads a private
site on one machine and finds nothing on a build runner
([§FS-001-forge-interface.5](FS-001-forge-interface.md#5-no-site-specific-data-in-the-repository)), and a profile gathered from commands that all
exited early is no profile at all.

## 4. Publication is gated on carrying nothing site-specific

No artifact is published while the tree still violates
[§FS-001-forge-interface.5](FS-001-forge-interface.md#5-no-site-specific-data-in-the-repository) or the
literal confinement of [§REQ-001-boundary.5](../requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter). The checks are mechanical and run
before anything is uploaded.

## 5. Between releases, main carries a dev version

A release leaves main holding the version it just published, so every build from
main until the next release reports the tag it is already ahead of. Nothing on
the machine can then tell a binary built from main from the released one, and a
fix that is merged but not installed looks identical to one that is installed.

So the release advances main as its last act: after publishing `X.Y.Z` it
commits `X.Y.(Z+1)-dev`. The suffix is what makes `--version` say which side of
the tag a build came from. It is deliberately not a releasable version — the
release path sets the clean version on its candidate branch and verifies it
there, so a `-dev` manifest can never be published.
