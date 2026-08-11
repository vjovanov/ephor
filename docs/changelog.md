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

- **Work**: what ephor watches, it can hand to an agent runtime
  ([§FS-005-dispatch](../requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)).
  An item plus a **recipe** becomes a ticket in a
  [rhei](https://github.com/vjovanov/rhei) plan, written into the checkout the
  item's branch resolves to. The ticket carries the **dossier** rather than a
  link — state, branch, the gate's counts per repository and the forge's own
  reasons, and the conversation quoted as messages — because all of it was
  fetched during a refresh already, and a ticket that says "look at pull
  request 42" has handed the whole job back. Four recipes ship and apply with
  no configuration: `fix-gate`, `answer`, `review`, `implement`. Dispatch
  writes files and nothing else: no comment, no push, no pull request.
- **A moved item reopens its own work.** ephor fingerprints an item when it
  dispatches — last activity, state, gate, how much conversation — and
  `ephor work sync` appends a ticket to the same plan saying what changed,
  ordered after the last one. What is asked for is chosen against the item as
  it is now: a pull request whose gate went green and whose reviewer asked a
  question is no longer a red gate, and reopening it as one would hand the work
  a ticket about a problem that is not there.
- `ephor work` — `list` (the ledger, with each ticket's state read back out of
  its plan), `dispatch`, `sync`, `run`, `forget`, and `states`. Every one that
  writes answers `--dry-run`, and `dispatch` is the sweep: every item in every
  project that matches a recipe and has no work yet, bounded by
  `--updated-within`. `run` names the plans ephor opened rather than the
  directory holding them, so a runtime project kept in the same checkout for
  other work is not swept in.
- `systemd/ephor-work-sync.{service,timer}`: refresh, then reopen everything
  whose item has moved. It writes tickets and runs nothing.
- **The work screen** (`w`): what has been asked about an item and what it
  reached — each ticket's state and the verdict its review left — whether the
  item has moved under it, and the recipes that apply now with the exact words
  each would send. `1`-`9` opens one, `s` reopens, `R` hands the root to the
  runtime, `e` reads the plan. Item rows carry the same as a badge.
- Forge interface: one set of types reachable two ways — implement `Forge` in
  Rust, or write an executable named `ephor-forge-<name>` answering
  `capabilities` / `pull-requests` / `issues` / `failures` / `react` with JSON. The types
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
- **Quick actions**: the source that produced an item offers what it knows to
  do about it, so the action menu is worth pressing before anyone has
  configured it. A pull request whose gate is red gets `✗ see the CI failures`,
  and it leads the menu, ahead of the configured actions
  ([§FS-004-quick-actions](../requirements.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)).
  The condition is the red gate rather than the kind of item, so the action is
  on the row that shows the red count whichever source reported it: `github-prs`
  and `github-ci` page the failing job's log through `gh`, and a forge that
  answers the new `failures` capability is asked directly.
- **`failures`, a forge capability**: what actually failed under a red gate —
  each failure as a job, a link to its log, and the error text where the forge
  can extract one. Asked on demand rather than during a refresh, since it is
  the expensive question and nobody asks it of a green gate. `ephor failures`
  is the command behind the menu entry; it collapses jobs that failed
  identically into one entry that says how many, because a gate fans one
  compile error across every job that built the file.
- **The gate screen** (`c`): the per-repository counts spelled out, and the
  forge's own reasons for refusing the merge, verbatim.
- **Recent** category: finished work — closed, merged, done, resolved, declined
  — leaves its category and stays visible for `defaults.recent_days` (7 by
  default), then leaves the feed. An issue closed without a reply is visible
  precisely because closing it was the answer.

### Removed

- The `bitbucket-prs` and `jira` providers, which called a vendor CLI from
  core. They are replaced by a forge extension living outside this repository;
  ephor now names no forge, tracker, or vendor tool anywhere in its source, and
  `scripts/check-no-site-specific.sh` passes.

### Fixed

- A provider block's `timeout_seconds` was read from the configuration and
  then ignored: every provider ran under the shared
  `defaults.provider_timeout_seconds`. A forge behind a VPN, configured with
  the longer ceiling it needs, timed out on every refresh and its whole
  section of the feed stayed empty
  ([§FS-001-forge-interface.6](../requirements.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
- An out-of-process forge whose `capabilities` probe failed was recorded as
  having declared no capabilities, which described an unreachable host or a
  crashed extension as a malformed one. The probe's own error is now reported.
- `unavailable (missing tool or secret)` never said what was missing. An
  extension is resolved from the provider's *name*, so the executable it looked
  for appears nowhere the reader can check; the diagnostic now names it.
- A failed command was quoted by the *first* line of its stderr, which is the
  stream tools narrate their progress on: opening a red gate reported
  `Requesting RCA for pull request …` and dropped the error four lines below
  it. The line reported is now the one that reads as a diagnosis, stripped of
  its colour codes, and the command is named once rather than three times
  ([§FS-001-forge-interface.6](../requirements.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).

### Changed

- **A gate now carries the forge's verdict, not only its counts.** A pull
  request whose every job passed can still be refused — on an approval, on a
  downstream repository, on jobs the gate never started — and a row showing
  `✓118` read as finished work. The row now says `⊘ blocked` beside the counts
  and the reasons are one keystroke away
  ([§FS-001-forge-interface.1](../requirements.md#1-capabilities)).
- **A refresh that lost any provider now exits non-zero** (`4`; `3` still means
  every provider failed) and reports each failure as `error:` naming the
  project and provider. A partial refresh used to exit 0, so a source could
  stay dark indefinitely behind a timer that saw nothing wrong
  ([§FS-001-forge-interface.6](../requirements.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
- Destinations that cannot be reached — DNS failure, refused connection, no
  route, a downed VPN — are classified as **unreachable** and reported as such
  in the refresh output, in `ephor status`, and in the interactive header. The
  distinction is between a network that will heal itself and a configuration
  the reader has to go and change.
- The interactive header names the providers that failed instead of counting
  them: "Refreshed with 6 provider warnings" reads the same whether an
  extension has been uninstalled for months or a laptop is briefly off the VPN.
- `ephor status` says what a failed provider cost — `NO DATA`, or that the
  items shown are the last good ones.

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
