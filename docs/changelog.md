# Changelog

Records every notable change to `ephor`. Versions follow semver
([§FS-002-release](../requirements.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change));
the **latest release is inline** in this file, and **older releases live
one-per-file under `docs/changelog/`** so a reader — human or agent — only
loads the history they ask for.

## 1. Conventions

### 1.1 Sections per release

`Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security` — the
Keep-a-Changelog set; omit any with no entries.

### 1.2 Entry style

One bullet per change, present tense, leading with the affected area. Every
pull request adds a bullet naming its own number (`PR #12`), which CI checks
([§FS-002-release.1](../requirements.md#1-changelog)).

### 1.3 Progressive discovery

Only **Unreleased** and the most recent release are inline. When a new release
ships, the previous "latest" section moves verbatim to
`docs/changelog/<version>.md` and a one-line link is added under
[§3 Older releases](#3-older-releases).

## Unreleased

### Added

- Forge interface: one set of types reachable two ways — implement `Forge` in
  Rust, or write an executable named `ephor-forge-<name>` answering
  `capabilities` / `pull-requests` / `issues` / `react` with JSON. The types
  derive serde, so the wire format is their JSON and the transports cannot
  drift. Policy — pending threads, answered citations, `needs_response` — lives
  above the interface, which is what keeps an implementation small enough to be
  a shell script.
- ephor is now a library as well as a binary, for in-process implementations
  outside this repository.
- `github-issues` provider: the user's GitHub issues by role — the ones they
  opened, and the ones they take part in without having opened. With no `repos`
  it searches the whole forge rather than a configured list, so an issue filed
  against a stranger's project is followed like any other; `updated_within_days`
  bounds the search instead of a repository list, and the comment fetch is
  skipped for issues that have none.
- Issues are their own feed category, split by role into **My Issues** and
  **Participating** like pull requests, and reachable as `--kind issue`. The
  forge interface's `Issue` gained a `role`, defaulting to author, so an
  existing extension keeps working unchanged.
- **Recent** category: finished work — closed, merged, done, resolved, declined
  — leaves its category and stays visible for `defaults.recent_days` (7 by
  default), then leaves the feed. An issue closed without a reply is visible
  precisely because closing it was the answer.

### Removed

- The `bitbucket-prs` and `jira` providers, which called a vendor CLI from
  core. They are replaced by a forge extension living outside this repository;
  ephor now names no forge, tracker, or vendor tool anywhere in its source, and
  `scripts/check-no-site-specific.sh` passes.

## 2. [0.1.0] — 2026-08-11

First version. Not yet tagged or published — publication is gated on
[§RM-001-forge-interface](roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface).

### Added

- Registry engine ported from the `automation` repo's Python `dev/projects`
  tool: `list`, `validate`, `ensure-agents`, and `update`, with the registry
  JSON Schema embedded in the binary and `required_branch_ids` replacing the
  previously hardcoded release-branch check.
- Per-project status feed with pluggable providers, cached under
  `~/.local/state/ephor/`. A failing provider keeps its last-good items marked
  `(stale)` rather than blanking the feed.
- Two-screen TUI (`ephor tui`): a navigator organized per organization, project,
  type, and branch, and a thread screen rendering a item's conversation with
  reactions. Item actions run configured commands in the item's checkout with
  the `EPHOR_*` context exported.
- Gate status on every pull request row — passed, failed, and running job
  counts, totalled across every repository the gate covers, with a per-repo
  breakdown when it spans more than one.
- `grund` tree: [§FS-001-forge-interface](../requirements.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
  and [§FS-002-release](../requirements.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change),
  with [§RM-001-forge-interface](roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface)
  sequencing the work that has to land before anything ships.
- Release pipeline: tag-triggered publication, profile-guided release binaries
  per target, a scheduled patch release and an on-demand minor release, and a
  pre-release gate that refuses to publish while the tree still carries
  site-specific configuration.

### Changed

- Renamed from `hub` to `ephor`, including the `EPHOR_*` environment contract,
  the state and secrets directories, and the systemd units.

## 3. Older releases

_None yet._
