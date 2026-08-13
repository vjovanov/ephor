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

- **The runtime is a module, and `work.runner` is where you point it**
  ([§AR-007-runtime](architecture/AR-007-runtime.md#ar-007-runtime-the-runtime-adapter-and-rhei-as-its-shipped-binding),
  manual §4.2.0). Everything runtime-specific now lives in `work/runtime/` —
  the plan language, the runner invocation, the verdict read back — and the
  name of the shipped runtime exists nowhere else in the source. Above that
  module, surfaces name *the binding*: the inbox's banner, `ephor work run`'s
  output and its errors all say whatever `work.runner` says, which is the
  shipped default until you name another. Nothing about writing or reading
  changes when no runtime is installed; only running refuses, naming the
  runner it looked for. The ledger's field was named for the runtime and is
  named for the plan now, with an alias so a ledger written before this still
  reads.
- **Tickets a project keeps in its checkout are read where they live**
  ([§FS-006-project-interface.7](../requirements.md#7-local-ticket-stores-are-read-where-they-live)).
  A `panta/` plan directory — or one the manifest points at — is now a source
  like any other: its open tasks arrive in the feed beside what the forges
  reported, under the store's own ids, attributed to the checkout's project
  because a store in a checkout is about that checkout and nothing has to
  guess. `.beads/` is recognized and reserved; a store ephor can see but
  cannot read yet reports nothing rather than pretending. Declaring a store
  does not hide a probed one — a project may keep two, and both are read — and
  the *ticketed* rung counts a declared store as well as a well-known name.
- **The gate is the project's, in three verbs**
  ([§FS-006-project-interface.6](../requirements.md#6-the-gate-is-the-projects-in-three-verbs)).
  `status` answers what the gate is doing per repository of the forest,
  `failures` answers what actually failed, and `restart` re-runs the failing
  gate and everything downstream of it, committing nothing. Each is bound site
  over manifest over **the forge itself** — a forge-hosted gate needs no
  manifest at all, because the provider's own gate capability is the shipped
  default, and above the seam nothing can tell that apart from a project that
  binds three commands. A `status` answer carries the per-repository breakdown
  through the envelope, so a change gating across a tree can still say which
  repository went red. Restart follows the dispatch semantics it was given:
  exit `75` is "still running, ask again later" rather than a failure, and
  restarting is bounded — past a few, the infrastructure is what is wrong and
  no amount of retrying is the fix. `ci-failures` and `restart-gate` stop being
  examples and become the shipped bindings: `ci-failures` now writes the
  envelope alongside its report, so the same run answers a reader and a
  program. The *gated* rung counts a bound verb, not only a source that
  happens to report one.
- **Checks are verbs, and which ones run stays your decision**
  ([§FS-006-project-interface.5](../requirements.md#5-checks-are-verbs-and-every-script-is-self-contained)).
  ephor binds three: `check`, `style`, `smoke` — probed as `./check.sh`,
  `./check-style.sh`, `./smoke-test.sh` at the forest root, or declared in the
  manifest under whatever paths a project prefers, or bound by you, in that
  precedence. Each runs as a summons and answers through the envelope, so a
  verb's `summary` and `failures[]` land in the verify dossier instead of a
  wall of log. Smoke may enumerate **features** — a static list in the
  manifest, or `--list` printing one id per line, which is a complete answer
  and not something ephor asks to be JSON — and a feature id runs that
  feature's smoke alone. What ephor does **not** decide is which verbs run or
  in what order: `$EPHOR_CHECKS` hands the bound ones to the verify step,
  newline-separated, and `config/verify.example.sh` sequences what it is
  given, falling back to its old guessing only when nothing was handed over.
  The *checkable* rung now counts a bound verb, not just a well-known filename.
- **A project can speak for itself, and the interface is published**
  ([§FS-006-project-interface.2](../requirements.md#2-the-manifest-is-offered-never-required),
  [§FS-006-project-interface.11](../requirements.md#11-the-interface-is-versioned),
  manual §4.2.1). ephor reads `ephor.json` at a forest root: identity hints,
  the forest's own layout, check and gate verbs, ticket stores, and offers.
  Offered, never required — every field optional, `{}` valid, and a project
  that places nothing is watched exactly as it stands. Two rules keep it safe:
  the registry row is authoritative over identity hints, adopting them only
  where it says nothing itself, and the row's new **`manifest_trust`** decides
  how much is believed at all — `full`, `descriptions` (read what it says
  about itself, run none of it), or `ignore`. Resolution everywhere is site
  configuration over manifest over probe, in one shared lookup rather than
  per caller. The three schemas are printable — `ephor schema
  manifest|answer|registry|forge` — and a manifest is checkable where it sits
  with `ephor validate --manifest .`, offline: no schema refers to another by
  URL, so a project can validate what it says with no ephor present.
- **Sources stop placing what they find; one engine does it**
  ([§FS-008-attribution](../requirements.md#fs-008-attribution-every-conversation-finds-its-project-or-says-that-it-could-not),
  [§AR-003-attribution](architecture/AR-003-attribution.md#ar-003-attribution-one-matching-engine-evidence-against-identity-at-two-scopes),
  manual §4.2). A source that asks nothing about any one project — GitHub's
  notification list, and mailboxes when they arrive — is declared at the top
  level in `sources` now and fetched **once** per refresh instead of once per
  project. Where each finding belongs is decided afterwards, by weighing what
  the conversation carries (its venue, the repository it is on, ticket keys and
  repositories named in what was said, who spoke) against what each registry
  row declares. Rows gained **`territory`**: repositories and organizations
  that are a project's business without being in its forest, either
  `"acme/plugin"` or a whole `"acme"`. That is what places the general case —
  a mention of you on some repository of the project's ecosystem, an issue
  filed there, none of it in any checkout.
  An explicit venue wins outright, a reference places next, resemblance only
  argues, and resemblance is whole words, so a project called `api` cannot
  claim "rapid". Two projects claiming the same thing equally is **not**
  settled by order: it goes to a bucket you can read with
  **`ephor feed --unattributed`**, because a guess that lands wrong amends
  someone else's row silently. Declaring a shared source under a project still
  works and says once per refresh where it belongs now.
- **A row that comes back says what brought it back**
  ([§FS-007-matters.2](../requirements.md#2-same-subject-one-matter-related-subjects-linked-matters),
  [§FS-007-matters.5](../requirements.md#5-an-event-moves-state-and-resurfacing-names-its-reason)).
  Merging is the model's now rather than a pass over rendered rows: reports of
  one subject fold into one matter, the fuller one surviving and the thinner
  one handing over the conversation, gate and reasons it alone saw. Matters
  that *reference* each other — the change implementing a ticket, the ticket
  it names — stay two matters and are linked on both sides, because merging
  what is one thing and linking what is related is the difference between a
  readable pile and a lossy one. And `seen.json` now remembers what a matter
  looked like when you read it, so when it returns the row says **⟳ the
  conversation moved** or **⟳ the gate moved** instead of only reappearing.
  A row marked read before this shows no reason rather than a guessed one.
- **One subject, one row: the CI and review-thread rows dissolve into the
  change they are about**
  ([§FS-007-matters.3](../requirements.md#3-a-discussion-is-messages-grouped-in-a-channel),
  [§FS-007-matters.5](../requirements.md#5-an-event-moves-state-and-resurfacing-names-its-reason)).
  A pull request with a red gate and two unresolved review threads was four
  rows: the pull request, a CI row for the same change, and one row per thread.
  It is one row now. `github-ci` reports the pull request carrying its gate —
  a gate is an observation of a change, not a subject — and `github-threads`
  reports the pull request carrying its unresolved threads as discussions.
  Merging keeps the fuller report as before, and now carries over what only
  the thinner one saw: a conversation it alone read (deduplicated, so the same
  thread reported by two sources is shown once) and a gate it alone fetched.
  Two consequences. The **CI category** now holds what it says it holds and
  nothing else — periodic build results, not gates that belong to a change —
  so it is empty until something reports one. And the **Failing** column
  counts matters whose gate is red wherever they sit, rather than rows of one
  kind, so it means the same thing it always did.
- **The feed is made of matters**
  ([§FS-007-matters](../requirements.md#fs-007-matters-the-feed-is-made-of-matters-and-a-matter-knows-why-it-is-there),
  [§AR-006-matters](architecture/AR-006-matters.md#ar-006-matters-the-core-types-of-the-watch)).
  The store now holds the model rather than a flat row per report: a `Matter`
  with a stated subject key, a placement (project and branch, or unattributed
  carrying the projects that claimed it), the conversation as `Discussion`s of
  `Message`s in channels that declare what they can carry, everything else as
  `Event`s, the keys it references as links, and a fingerprint digesting state,
  each discussion's last activity and message and task counts, and the event
  tail — which is what will let a resurfacing row say *what* moved instead of
  only that something did. A gate is an observation of a matter now, not a row
  of its own. The surfaces still read the flat rendering while they are ported
  onto the model, and a round-trip test keeps that rendering honest.
  **The cache is a cache**: it carries the model it was written in, and an
  older one is dropped rather than migrated, so the first run after upgrading
  shows an empty feed until `ephor refresh` fills it again. What you marked
  read is not in that store — it is keyed by matter key in `seen.json` and
  survives the rebuild untouched.
- **What a project can do is a ladder, computed once and consulted everywhere**
  ([§AR-005-capabilities](architecture/AR-005-capabilities.md#ar-005-capabilities-availability-is-computed-once-and-consulted-everywhere),
  [§FS-006-project-interface.10](../requirements.md#10-capability-rung-by-rung),
  manual §7.5). Whether something could run was decided in a dozen places, each
  with its own sentence or with none: a menu entry that could not run said only
  `(unavailable)` and made you press it to find out why, the inbox's `R` key
  handed the terminal to a runner that was not installed, and `ephor work run`
  had the one good sentence about it. There is a `CapabilitySet` per project
  now — eight rungs, each held or missing with the sentence saying why — and
  offering is filtering on it while refusing is rendering it. You see the same
  words wherever you meet the limit, and you see them where the feature would
  have been rather than after choosing it. It is resolved at load, after every
  refresh, and after a checkout, from `stat` calls and configuration only. And
  because a table speaks for the moment it was resolved, the executor
  re-checks the two things a command leans on — its directory, and its script
  where the binding names one — at invocation, so a directory deleted since
  then fails as the world rather than as a stale answer.
- **A project is a forest, and every git-facing feature folds over it**
  ([§AR-004-forest](architecture/AR-004-forest.md#ar-004-forest-git-is-the-substrate-and-a-project-is-a-forest-folded-over)).
  Repositories were a list of relative path strings that four places rebuilt
  and three folds collapsed into a bare number. They are a `Forest` now — an
  ordered set under a root, declared by the registry row and probed where it
  declares nothing — and staleness, rebase, and checkout fold over it, keeping
  the per-repository answer: a branch row that is `5 behind` can say it was
  `ce 2, ee 3`. Three consequences you can see. **Probing counts each
  repository once**: a checkout with `docs/` and `src/` under a single
  repository reported itself four times behind, because every subdirectory of
  a working tree answers "yes" to being one; a repository is its own toplevel
  now. **A declared repository that is not on disk is named** rather than
  silently dropped from the fold. And **`$EPHOR_REPOS`** hands a summoned
  command the same repository list, in the same order, so the shipped landing
  example folds over ephor's forest instead of probing its own
  ([§FS-005-dispatch.8](../requirements.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)).
  Under it, one resolver answers where a branch is checked out — the inbox's
  grouping, the action menu, dispatch, and the CLI had three implementations
  of that question and now share one
  ([§AR-004-forest.3](architecture/AR-004-forest.md#3-workspace-resolution)).
- **Running work is a summons too**
  ([§AR-002-summons](architecture/AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world),
  [§FS-005-dispatch.12](../requirements.md#12-a-key-and-a-state-are-the-same-operation)).
  `ephor work run` and the inbox's `R` key built the runner invocation twice,
  in two places, with two ideas about what a failure was. There is one
  construction of it now and one process path to it, still run from the
  checkout the work is about
  ([§FS-005-dispatch.3](../requirements.md#3-one-rhei-per-item-one-ticket-per-dispatch)),
  and a runner that exits `75` is parked rather than failed. Reading a plan
  in your editor goes through the same path, so the TUI has no hand-rolled
  spawn left.
- **The local runners now go through the one executor**
  ([§AR-002-summons](architecture/AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world)).
  Configured actions, quick actions, the one-off command, the configured
  checkout, and `custom-status` were four spawn sites with four ideas about
  what an exit code means; they are one now. Two things follow for anything
  already configured: `75` means *parked* everywhere — for a status source
  that is "nothing to report just now" rather than a failed refresh — and
  every command may write a structured answer to `$EPHOR_ANSWER`, so
  `custom-status` gains `format: "answer"` (manual §5.1) and speaks the same
  envelope as every other verb, with its stdout-reading `text` and `json`
  forms kept and marked as the legacy they are. `custom-status` is also told
  which project it is running for now, in the same `EPHOR_*` vocabulary a menu
  action receives ([§FS-005-dispatch.8](../requirements.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)).
- **One executor for everything ephor asks of the world**
  ([§AR-002-summons](architecture/AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world),
  [§FS-006-project-interface.3](../requirements.md#3-a-summons-environment-in-exit-code-and-answer-out)).
  A new `seams` layer holds the summons executor: it resolves where a command
  runs (the branch workspace where one resolves, the forest root otherwise, or
  a named repository of the forest) and refuses with the reason rather than
  running somewhere surprising, exports the dossier as the one `EPHOR_*`
  vocabulary, spawns through `sh -c`, and reads the exit code uniformly —
  `0` done, non-zero failed, `75` *parked*, meaning not applicable now, ask
  again later. Whether the person watches is the call site's property, not the
  binding's. Alongside it, `$EPHOR_ANSWER`: the executor names a fresh file
  before spawning, and a command that writes one gets it validated against the
  published envelope schema
  ([§FS-006-project-interface.4](../requirements.md#4-the-answer-envelope))
  — now embedded in the binary — with its `failures` and `gate` conveniences
  normalized into events, its `features` into facts, and its answer paths
  resolved against where the command ran. No answer file is a complete answer:
  the exit code stands alone, and standard output is never parsed for
  structure.
- **The decisions behind the boundary are on record, and the climbing rule is
  checked.**
  [§DA-001-runtime-bound-default](decisions/architectural/DA-001-runtime-bound-default.md#da-001-runtime-bound-default-the-runtime-is-a-bound-default-not-a-named-coupling)
  records the runtime's reversal into a bound default, superseding the old
  [§FS-005-dispatch](../requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)
  lead stance;
  [§DA-002-fetch-attribution-split](decisions/architectural/DA-002-fetch-attribution-split.md#da-002-fetch-attribution-split-fetch-normalizes-attribution-places)
  the fetch/attribution split with its `status.json` restructuring named as
  the accepted cost;
  [§DF-001-manifest-offered](decisions/functional/DF-001-manifest-offered.md#df-001-manifest-offered-the-manifest-is-offered-never-required)
  the manifest as offer, never requirement, with recipes excluded. The
  `[citations]` ruleset in `.agents/grund.toml` turns the climbing rule into
  checked configuration — E2E→FS gates, the rest surface as suggestions — and
  [§REQ-001-boundary](requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy)
  is now cited from every FS and AR page it binds.
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
