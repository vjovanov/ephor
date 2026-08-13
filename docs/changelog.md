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

- **Being asked is now a reason a pull request is yours**
  ([§FS-001-forge-interface.1](../requirements.md#1-capabilities)). `github-prs`
  searched `--author`, `--commenter`, and `--mentions` — all three of which
  find pull requests you have *already spoken in*. A review requested of you and
  a pull request assigned to you leave nothing behind in the conversation, so
  they looked exactly like work that was none of your business. Both are now
  searched (`--review-requested`, `--assignee`), every reason a pull request is
  yours rides on the item as `raw.reasons`, and a review asked for and not yet
  given needs a response on its own — no thread rule can find that one.
- **`github-notifications`: the source whose job is to be exhaustive**
  ([§FS-001-forge-interface.1](../requirements.md#1-capabilities), manual §5.2).
  Every other source asks a question you composed, and a question never asked
  looks on screen exactly like one answered "nothing". This one asks nothing —
  it reads GitHub's own notification list and reports what is on it: team
  mentions (`@acme/reviewers` names you, and no search qualifier returns that),
  discussions, releases, advisories, invitations, and pull requests in
  repositories you never configured. One paginated call per refresh. It is the
  difference between an empty feed meaning "nothing is waiting" and meaning
  "nothing is waiting in what I was told to look at". `reasons` keeps it
  readable — the default is the set that means somebody is waiting on you by
  name, and `assign` is off because on a busy repository assignment is a bulk
  mechanism and what is genuinely assigned already arrives through `github-prs`
  and `github-issues` with its conversation attached.
- **The forge interface gains a `notices` capability**, in both transports: an
  out-of-process forge answers `notices` alongside `pull-requests` and `issues`.
  GitLab's todos map onto it almost one for one.
- **One subject is one row, however many sources reported it**
  ([§FS-003-feed-categories.5](../requirements.md#5-one-subject-is-one-row-however-many-sources-reported-it)).
  Sources are meant to overlap now, so the overlap is merged rather than shown:
  the report carrying the conversation, the gate, and the role wins the row, and
  what only the thinner one knew — the reason GitHub gave for telling you —
  comes with it. Identity is the subject the forge stated, never the title.
- **The rebase ephor already knew you needed**
  ([§FS-004-quick-actions.6](../requirements.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
  The inbox has always said `3 behind` on a branch row and then left you to go
  elsewhere about it. Now a pull request whose branch workspace is on disk and
  trails its `main_branch` is offered **`⤴ rebase onto <main> (N behind)`** in
  its action menu, and `ephor rebase` is the command behind it: fetch and
  replay every repository in the checkout, an answer per repository, no forge
  and no vendor CLI anywhere in it. Uncommitted work is reported and left
  alone rather than stashed.
- **A conflict becomes work, and nothing else does**
  ([§FS-005-dispatch.12](../requirements.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
  Replaying a branch is a fetch and a rebase; paying a model to type those is
  paying for something a script does the same way every time. So `ephor
  rebase` runs first and exits `3` where it stopped, leaving the repository
  mid-rebase — the state resolving it needs — and `--dispatch` opens the
  ticket about that conflict on the spot, carrying which files and which two
  sides. A clean replay opens no ticket at all. The shipped `rebase` recipe
  and a new `behind` recipe selector go with it, and
  `config/ci-green.example.states.yaml` wires `rebase → resolve-conflicts →
  verify-rebase → land-rebase`, where landing forces `--force-with-lease`
  because a replayed branch cannot fast-forward.

- **[docs/manual.md](manual.md)**: the whole surface in one document — install
  and resolution order, the vocabulary, both configuration files field by
  field, every provider's options, the categories, every key of every screen,
  actions and their environment, work end to end, automation, extension points,
  and a troubleshooting section keyed by the message you actually get. The
  README is now the tour and points at it.
- **Work**: what ephor watches, it can hand to an agent runtime
  ([§FS-005-dispatch](../requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)).
  An item plus a **recipe** becomes a ticket in a
  [rhei](https://github.com/vjovanov/rhei) plan, written into the checkout the
  item's branch resolves to. The ticket carries the **dossier** rather than a
  link — state, branch, the gate's counts per repository and the forge's own
  reasons, and the conversation quoted as messages — because all of it was
  fetched during a refresh already, and a ticket that says "look at pull
  request 42" has handed the whole job back. Five recipes ship and apply with
  no configuration: `fix-gate`, `answer`, `review`, `implement`, `rebase`.
  Dispatch writes files and nothing else: no comment, no push, no pull request.
- **A moved item reopens its own work.** ephor fingerprints an item when it
  dispatches — last activity, state, gate, how much conversation — and
  `ephor work sync` appends a ticket to the same plan saying what changed,
  ordered after the last one. What is asked for is chosen against the item as
  it is now: a pull request whose gate went green and whose reviewer asked a
  question is no longer a red gate, and reopening it as one would hand the work
  a ticket about a problem that is not there.
- **A ticket carries the item as data** (§FS-005-dispatch.8), not only as
  prose: every ticket's identifiers — project and source, kind and id, repo and
  number, branch and ticket, url and state, and the checkout — are written into
  the plan's frontmatter under the same names a shell action gets in its
  environment. A state machine hands them to a program as `{meta.*}`, which is
  what lets a **script run in front of the agent**: ask the forge what actually
  failed, write it as an artifact, and let the agent state declare that
  artifact as an input. The metadata is merged rather than rewritten, because
  the runtime keeps its own per-task bookkeeping in the same block.
  `config/ci-failures.example.sh` and
  `config/ephor-work-ci.example.states.yaml` are a working pair, including the
  exit codes that pick the next state — `75` when the gate is still running,
  so a ticket waits on a poll instead of spending an agent on a half-finished
  gate.
- Work runs **from the checkout it is about**. The runtime finds the agent's
  working directory by looking for the git repository enclosing the plan; a
  multi-repo workspace holds several and may be none, and the fallback is then
  whatever directory the runtime was started in — which `ephor work run` had
  left as wherever it was typed. The checkout is now recorded in the ledger at
  dispatch and both `work run` and the interface's `R` run there, so the root
  of a multi-repo project is the directory its repositories sit in whether or
  not that directory happens to be a repository.
- `config/ci-green.example.states.yaml` gained the half that closes the loop:
  a program authors a **join ticket** whose `**Prior:**` names every cause
  ticket, so it runs only once all of them have finished, and a `land` state
  then pushes and lets the gate run again. Two things make it safe — the
  give-up state is named `cancelled`, which is the only final state that does
  *not* satisfy a prerequisite, so nothing is pushed while a cause is
  unresolved; and `land` commits nothing, because every fix commits its own
  work and `git add -A` before a push sweeps up whatever else was in the tree.
- **A workspace that is not there is offered the checkout**
  (§FS-004-quick-actions.7). ephor knew the branch was not on disk — it
  computes the directory from the project's own template and looks — and then
  refused every action that needed it with "no 'checkout' command is
  configured", which is knowing what is wrong and sending the reader to a
  configuration file about it. There is now `ephor checkout`, and the menu
  offers it unasked: the registry already holds every input it takes, so the
  project's `branch_root_template` says where the workspace goes, its type says
  which repositories it holds, and its `main_branch` says what a new branch
  grows from. It is git and nothing else, and it has the rebase's shape — a
  working tree per repository, since a poly-repo workspace is several
  repositories sharing one branch name: the branch itself where that repository
  has it, and a new branch of the same name off the base where it does not,
  which is what a change touching one repository of a tree looks like on disk.
  A repository whose branch another working tree is holding is reported and
  left alone rather than worked around, one already there is reported as
  already there, and the same command runs whether the reader presses the key
  or a state machine calls it (§FS-005-dispatch.12). A project that wants its
  own checkout command still configures one and it still wins; the difference
  is only whether anybody expects to want their own.
- **A failure that was never the change's fault is restarted, not fixed**
  (§FS-005-dispatch.11). The loop could recognize a dead runner or a flake in
  two places and act on it in neither: triage was told to open no ticket, which
  ended the plan with the gate still red and nothing to make it run again, and
  an analysis that concluded "not ours" carried no marker `route.sh` knew, so
  it exited `0` and the ticket walked into propose → critique → fix and spent
  two passes fixing something that was never broken. There is now a
  `restart-gate` state and a `NOT-OURS:` marker that reaches it. It is a
  program, not an agent: it is handed the list of jobs as a declared input —
  `<repo> <job-key>` per line, exactly, or `<repo> -` for a whole gate — and it
  restarts those plus every gate the forge still reports red underneath them,
  which is what a gate spanning several repositories needs, since it fails
  downward and the gates below never start. Nothing is committed; the change
  was not the problem. A job that went green while the ticket waited is left
  alone rather than re-redding a passing gate, the restart gets a ticket of its
  own so the next round can see the failure was already retried, and the budget
  is counted out of the plan — past two restarts on one item the work stops for
  a person, because at that point the infrastructure is what is wrong.
  `config/restart-gate.example.sh` is the worked script; `GATE_RESTART` is
  where a forge's own re-run command goes, since there is no neutral one.
- **Work can stop for a person, and say so where you are looking**
  (§FS-005-dispatch.9). Where a ticket sits in a state the runtime will not
  leave on its own — a gating state — ephor reads that out of the machine and
  leads the item's badge with `⚠ waiting on you`, ahead of anything else the
  work is doing, since it is the one part nobody else will move. The work
  screen prints the ticket and the `rhei transition` that resumes it. The
  question and its answer stay in the plan: an agent writes `NEEDS-HUMAN:` as
  the first line of its artifact, a program routes on it, and the person
  answers beside the question rather than in a chat window the next round
  cannot read. `config/ci-green.example.states.yaml` is a worked machine that
  uses it — collect, triage on a cheap model, then analyze → propose →
  critique → fix → verify per failure, with the escalation available at every
  step.
- **Asking by hand** (§FS-005-dispatch.10): `a` on the work screen types one
  line and it becomes an ordinary ticket — same dossier, same plan, same
  order — with the reader's words as the brief, and `ephor work ask --item ID`
  does it from a script or from stdin. It is refused for nothing but being
  unrunnable: selectors say what ephor volunteers, not what a person may ask
  for, so a merged pull request or an item no recipe covers is fair. The action
  menu gained the same at the command level — a last entry, `⌨ run a command
  here…`, that runs what is typed exactly as a configured action does — and it
  now opens even when nothing is configured, which is when it is most useful.
- `ephor work` — `list` (the ledger, with each ticket's state read back out of
  its plan), `dispatch`, `ask`, `sync`, `run`, `forget`, and `states`. Every one that
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

- **Tasks**: a forge that tracks tasks — a checklist item, a blocker comment, a
  review task — reports each one's state on the message carrying it, and ephor
  draws it as the box it is. `t` on the thread screen ticks the selected task
  through the source that reported it, and the box fills in without waiting for
  a refresh ([§FS-004-quick-actions.5](../requirements.md#5-a-task-is-ticked-where-it-is-read)).
  New capability `tasks`, new subcommand `ephor-forge-<name> resolve-task`.
- **A ticked box answers its thread.** Task state outranks who spoke last, in
  both directions: an open task keeps its conversation awaiting you however it
  ended, and a resolved one settles it even where every message belongs to a
  robot ([§FS-003-feed-categories.4](../requirements.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).
  Bot checklists could not be cleared before this — nobody but the bot ever
  writes in those threads, so the last word was never the reader's and never
  would be, and a pull request whose boxes were all ticked weeks ago still read
  as work.

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
- The thread screen advertised `+ react` on every message, including those no
  forge would take a reaction for; pressing it answered with one line at the
  bottom of a full screen of conversation, which is the one place a reader is
  not looking. Message keys are now offered per selected message
  ([§FS-004-quick-actions.2](../requirements.md#2-offered-only-where-it-would-work)).
- Posting a reaction only ever reached GitHub. `Forge::react` and the
  `ephor-forge-<name> react` subcommand were implemented and documented but had
  no caller, so an out-of-process forge that answered them could not be reached:
  a descriptor ephor did not recognize was dropped rather than handed back to
  the implementation that wrote it. Reactions now route through the source that
  reported the message.

### Changed

- **`github-prs` no longer needs a repository list, and no longer hides
  finished work.** `repos` is now optional: empty searches the whole forge, as
  `github-issues` already did, bounded by `updated_within_days` (30) instead —
  a pull request in a repository nobody configured is yours just as much as one
  on your own. Closed and merged pull requests come back too and land under
  Recent; a question asked of you does not stop being asked when the branch
  lands. Nothing further is fetched about a finished one, so the extra coverage
  costs no extra API calls. `reviews` now defaults to **on**.
- **`github-prs` reads the whole conversation before deciding you answered.**
  Answered-detection looked at the conversation tab only, so a reply left on a
  line of the diff never counted and the citation stayed pending forever.
  Review threads are now fetched in the same call and count as answers. A
  mention is also matched as a whole handle rather than as a substring:
  `@vjovanovic` is no longer a mention of `@vjovanov`, and a team named in the
  conversation now cites everyone on it.
- **Deciding what a pull request means moved out of `github-prs` and into
  policy**, where every forge gets the same treatment
  ([§FS-001-forge-interface.3](../requirements.md#3-policy-lives-above-the-interface-never-in-an-implementation)).
  The provider reports roles, reasons, conversation, and gate; `role`, the
  displayed state, and `needs_response` are composed above it.
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
