# Changelog

Records every notable change to `ephor`. Versions follow semver
([§FS-002-release](functional-spec/FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change));
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
([§FS-002-release.1](functional-spec/FS-002-release.md#1-changelog)).

### 1.3 Progressive discovery

Only **Unreleased** and the most recent release are inline. When a new release
ships, the previous "latest" section moves verbatim to
`docs/changelog/<version>.md` and a one-line link is added under
[§3 Older releases](#3-older-releases).

## Unreleased

### Changed

- **One live run per checkout, enforced where runs start**
  ([§FS-005-dispatch.24](functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)). The autorun sweep skipped a work *root* a run
  already held, so two roots over one checkout — a second panta beside the
  first, or a hand-started run beside a swept one — could both put an agent in
  the same working tree. The guard is now over the checkout, at every place a
  run starts: the sweep passes over a root whose tree a live run holds from
  any root — naming that run, rather than reporting an empty sweep — a tree a
  launch takes mid-sweep is passed over for the rest of that sweep with the
  run that took it, one `ephor work run` over two roots in one tree starts one
  of them and refuses the rest by the id of the run it just made, and `R` on
  the work screen is refused the same way. The refusal is `a run is live in
  this checkout: <id>`; `--force` lifts it for a run asked for by name and
  does nothing with `--due`. Queueing is untouched — laying or dispatching a
  ticket into a busy checkout's plan still succeeds, which is what makes
  handing work down to a tree somebody is in safe. (PR #N)

- **File-size limits have one authority.** The duplicated numeric limits table
  is removed from [§FS-012-file-size](functional-spec/FS-012-file-size.md#fs-012-file-size-every-file-is-measured-against-a-budget-set-by-how-it-is-read), so
  `.agents/fissile.toml` alone holds the configured numbers and its hard-limit
  findings now say that each rule sets its own ceiling. (PR #57)

- **A scope selector is honoured or refused, never ignored**
  ([§FS-011-command-line.9](functional-spec/FS-011-command-line.md#9-a-scope-selector-is-honoured-or-refused)). `--workspace`, `--tag` and `--org` were parsed by
  every verb and read by three, so `ephor status --org foundation` and `--org
  graal` printed the same site-wide table and `ephor work dispatch --org X
  --dry-run` proposed the other organization's work. Every variant of the
  command tree is now classified: `list`, `status`, `feed`, `refresh`,
  `mark-read`, `branches`, `tui`, `work list`, `work dispatch`, `work sync`,
  `work run`, `validate`, `ensure-agents` and `update` scope their reading by
  the selectors, and every other verb refuses each one it was given, naming
  itself and the flag and exiting 2 — the code an empty selection and every
  other configuration refusal already took — and under `--json` saying so as an
  outcome on standard output like any other refusal. Because the feed-side
  verbs pick their projects from `status.json` while the selectors name
  registry rows, a selector is resolved against the registry and intersected
  with what the site watches; a selection that comes out empty is refused
  rather than printed as an empty table. The screen scopes with them too: a
  `tui` opened under a selector shows those projects and its `r` key now
  fetches only them. **Callers who passed a selector that was silently ignored
  now get an error, and that is the point.** (PR #58)

- **`--all` belongs to the verbs that read it.** It was a global flag with two
  meanings: "every branch entry rather than only the active ones" to
  `validate`, `ensure-agents` and `update`, and "every project" to
  `mark-read`. It is now declared on each of those four verbs and says there
  what that verb means by it, so `ephor validate --all` and `ephor mark-read
  --all` are unchanged while `ephor --all validate` — the flag before the verb
  — is no longer accepted, and no other verb advertises it. `mark-read --all`
  is narrowed by `--org`, `--tag` and `--workspace` like every other project
  selection, and prunes the read-marks of vanished items only when the sweep
  really covered every watched project. (PR #58)

- **Every declaration is listed in its folder's index.** `grund check` warned
  that thirty-two declarations — across `§FS`, `§AR`, `§REQ` and the decision
  folders — were absent from their index README, and that the warning becomes an
  error in grund 0.13.0. The entries are added now, and the two decision folders
  that had no index at all have one. (PR #55)

- [§FS-002-release.5](functional-spec/FS-002-release.md#5-between-releases-main-carries-a-dev-version):
  after a release, main opens the next patch as `X.Y.Z-dev` instead of keeping
  the version it just published. A build from main previously reported the tag it
  was already ahead of, so a merged-but-uninstalled fix looked exactly like an
  installed one. (PR #53)

### Added

- **Work placement can reach above a project, and discovery can find it there**
  ([§FS-005-dispatch.6.1](functional-spec/FS-005-dispatch.md#61-the-work-root-is-a-template-and-it-may-reach-above-the-project)).
  The registry has always given an organization a `root`, and nothing in work
  placement could name it: `work.root` was site-wide or per-project, its
  placeholders stopped at project scope, and a template naming `{org_root}`
  wrote a directory literally called `{org_root}`. `{org}` and `{org_root}`
  are now placeholders like any other, resolved from the organization the
  project's registry row places it in, and `organizations.<org-id>.work.root`
  is the tier between the site's `work.root` and a project's — precedence
  project, then organization, then site, with the innermost one written the
  whole answer. So `"root": "{org_root}/panta"` on an organization makes one
  work root for all of its projects, for work that belongs to no single
  repository. The board finds a plan laid there: the organization's root is
  one of the places the work-root walk probes, where the template reaches for
  it, so such a plan is enumerated and swept rather than written to and never
  looked at. Where the answer is missing the dispatch refuses by name —
  *organization acme declares no root*, and as explicitly for a project whose
  registry row names no organization — instead of writing a literal
  `{org_root}` directory or a path with the segment missing; discovery skips
  the same template rather than guessing, since nothing could have been
  written through it. `ephor checkout` resolves the work root through the
  same three tiers, so the placement is one answer for both. Nothing changes
  for a configuration that names no organization placeholder and no
  organization-tier root. (PR #59)
- **Burn — what this machine is spending on agents, as a page and a command**
  ([§FS-013-burn](functional-spec/FS-013-burn.md#fs-013-burn-what-this-machine-spends-on-agents-is-a-reading-like-any-other)).
  `$` from any screen opens the Burn page and `ephor burn [--window
  1h|6h|24h|7d] [--by project|model|session|plan|matter] [--rescan]` is the
  same reading on the command line, with the published `burn` shape under
  `--json`. Ephor had no cost surface at all: the agent command-line tools
  were already writing down every token and the runtime was already metering
  its own runs, and neither fact was reachable. **Two lenses, and nothing
  sums them** — the machine lens is built from the tools' own transcripts
  (`~/.claude/projects`, `~/.codex/sessions`) and is the ground truth for a
  total, while the work lens is built from the runtime's accounting records
  under each work root and reaches a matter through ephor's own ledger. A run
  measured by both appears in both, so adding them would double-count every
  run the runtime started, and the reading says which lens answered. The work
  lens carries how many invocations recorded no usage rather than letting a
  reader mistake a short number for a cheap plan. Ingest reads each log the
  one way that is right: only the outer counters of a call, never the
  per-iteration breakdown that restates them; the session counter diffed
  rather than summed, because it is cumulative; each delta attributed to the
  model in force at that event; and a dollar rollup filed under the model its
  own session called, so tokens and dollars are one row instead of two.
  A response written as one record per content block is charged once, under
  the request its records all name, so a reply that calls two tools is not
  billed three times. The runtime's records are read where it writes them —
  one directory per plan under each work root — and their cached counters are
  taken out of the input where the tool that reported them put them inside it
  and left alone where it put them beside it, since both shapes arrive and no
  field says which. A matter's burn reaches the plans a workflow laid down
  for it as well as the one its ledger entry wrote itself.
  Records are attributed to a project by matching the directory they ran in
  against the registry, longest match first, and anything under no registered
  root lands in `other` rather than being dropped. Scanning is incremental —
  a per-file cursor of offset, size and modification time, so the first pass
  is a backfill and every later one reads only what was appended — and what
  it reads is aggregated into five-minute buckets under
  `~/.local/state/ephor/burn/`, one file per day, thirty days kept, swept on
  the next scan. Neither surface scans while drawing: the page refreshes from
  the same tick that watches the work artifacts and the command refreshes
  inline only when the store is over thirty seconds stale, both local file
  reads, so `refresh` remains the only verb that asks the world. Tokens are
  the reading and dollars are opportunistic — ephor computes no prices and
  ships no price book — so **`unpriced` is never rendered as `$0.00`**, and
  `cost_usd: null` with `priced: false` stays distinct from a priced zero in
  the machine form. Deferred and written down rather than dropped: a site
  price book, per-invocation live detail, and drilling into a row. (PR #60)

- **Autorun capacity has an organization tier between the site's and each
  project's**
  ([§FS-005-dispatch.24](functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)).
  The feed configuration gains a top-level `organizations` map mirroring
  `projects`: `organizations.<org-id>.work.max_concurrent` bounds the live
  autorun roots of every project whose registry row carries that
  `organization` id, inside the site's aggregate ceiling and outside each
  project's own. Membership stays the registry's; the ceiling is read-only
  against it. All three ceilings are evaluated and a sweep refuses by
  whichever is full first, naming the outermost one in the `passed-over`
  reason in prose and `--json`. A project ceiling written above its
  organization's — or above the site's — is a note at the sweep giving the
  project, the ceiling above it and both numbers; it is never refused, never
  rewritten, and the ceiling above it still bounds its total. A ceiling of `0`
  above is a pause somebody wrote on purpose rather than a budget, so the
  numbers under it are not noted. Omission is unchanged in both directions: a
  configuration with no `organizations` map, or a project whose registry row
  names no organization, starts exactly what it started before. An
  `organizations` id no registry row places a project in bounds nobody and is
  named for it — by `ephor doctor`, in the words an unknown project id gets,
  and in a note at the sweep — rather than quietly ignored. That covers an id
  the registry never declared and one it declares that no project has joined
  alike: membership is the project row's `organization` field and nothing
  else, so the ceiling and the note read the same thing. Note that the feed
  configuration refuses unknown keys, so a `status.json` carrying
  `organizations` needs this version. (PR #54)

- **The functional specification is one document per declaration.** The twelve
  `§FS` declarations now live under `docs/functional-spec/`, so each subject
  has its own file and the obsolete whole-file size exceptions are gone. (PR #52)

- **`ephor work lay` loads repeatable workflow values files**
  ([§FS-005-dispatch.19](functional-spec/FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)).
  `--values <file>` accepts YAML or JSON mappings relative to the invocation
  directory, merges repeated files left to right, preserves structured values,
  and lets explicit `--set` answers win. File answers are shown with their
  provenance, execution targets remain under Ephor's hand policy, and invalid
  or runtime-rejected input leaves no partial workflow workspace. (PR #49)

- **A workflow entry can ask to run itself, and what it lays down is due like
  any other work**
  ([§FS-005-dispatch.28](functional-spec/FS-005-dispatch.md#28-a-workflow-entry-can-ask-for-the-same-thing-a-recipe-can),
  [§FS-005-dispatch.24](functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)).
  `"autorun": true` may now be written on an entry that lays a workflow down,
  in all three of its homes — beside the workflow, in the project's manifest,
  in your own `status.json` — and is refused on an entry that runs a command
  or asks for a ticket, where it would be a second spelling of a fact the
  recipe already carries. `ephor work dispatch` lays the first matching
  entry that said it about a matter no recipe covers and that has no work,
  in the ranking's order — finished work never matches, exactly as it never
  matches a recipe — counted against `--limit`, honest under
  `--dry-run`, reported as `laid` / `would-lay` rows and a `laid` count in
  prose and `--json` alike; a refusal is named with nothing written, and a
  second sweep does not lay it again. `ephor work run --due` then treats the
  laid plan as ordinary work: its tasks are read wherever the runtime wrote
  them — including the `tasks/*.md` of a plan rendered as a directory — and
  every surface that reads them judges them by the state machine in force for
  that plan — the one beside it where it declares one, and the work root's
  where it declares none, which is what the runtime itself resolves such a
  plan against: the due sweep, the operations board, and the menu row all
  call a task its own machine parks *waiting on you* rather than queued, and
  a board that had to withhold judgment for want of a machine names the plans
  it withheld it for. A plan nothing in the record laid is still nobody's to
  start, whatever its tasks are named. It runs with capacity ceilings, cross-process
  reservation, ranking, failed-start back-off and the branch guard applying
  exactly as they do to a root a recipe wrote. Recipes keep their priority, and an entry that says
  nothing behaves exactly as before: a menu row, laid by you and started by
  you. (PR #28)

- **A second autorun ceiling bounds the work an agent is actually doing**
  ([§FS-005-dispatch.24](functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)).
  `work.max_active` and `projects.<id>.work.max_active` mirror the
  `max_concurrent` pair over the live roots that are *working*, so the two
  questions a single number had to answer — how much may be spent at once,
  and how many worktrees and processes this machine may hold — can be set
  apart. A live root is **parked**, and outside `max_active`, when nothing in
  it is witnessed running and an open ticket in it waits on a person: a
  `gating` state, or a poll declaring `waiting_on`, which the runtime now
  lets a machine say. A root whose machine cannot be read counts as working,
  because a misreading must not hand out capacity. Omitting `max_active` is
  unlimited, so a configuration that names only `max_concurrent` starts
  exactly what it started before and is refused in exactly the words it was;
  `--max-concurrent N` still overrides the roots-in-flight ceiling and that
  one alone. A refusal now names the key it refused on, and the sweep's
  reading says how many roots are live and how many of those are parked, in
  prose and under `--json`, so a full ceiling never hides that a person is
  holding one of the slots. Ceilings gate starts, so parked runs resuming
  together can carry working roots above `max_active` until one finishes.
  (PR #56)

- **Autorun sweeps can cap live work globally and per project**
  ([§FS-005-dispatch.24](functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself)).
  `work.max_concurrent` sets an aggregate ceiling and
  `projects.<id>.work.max_concurrent` adds a project ceiling inside it;
  omission stays unlimited and zero starts no new runs. `ephor work run
  --due --max-concurrent N` overrides the aggregate ceiling for one sweep
  while retaining project ceilings. Existing live roots consume capacity,
  failed or already-finished launches leave room for the next ranked root,
  and eligible roots omitted only because capacity is full are reported as
  non-failing `passed-over` outcomes in prose and JSON. (PR #25)

- **Dispatch reads an ordering already made, and a limit bounds what runs**
  ([§FS-005-dispatch.26](functional-spec/FS-005-dispatch.md#26-an-ordering-already-made-can-be-read-and-a-limit-bounds-what-runs),
  PR #24). Ephor does not compute a rank — it reads one a project already
  wrote: an optional `"ranking": "<path>"` in the `work` block of site
  configuration, or `--ranking <path>` on `ephor work dispatch` displacing it
  for one run, names a file of item ids, one per line, most important first.
  Ranked items dispatch before every unranked one, in exactly the file's own
  order; everything the file does not name follows in the order it already
  had — the file orders, it never filters. `ephor work dispatch --limit N`
  bounds how many items are actually dispatched (opened, or would-open under
  `--dry-run`), taken from the top of that order; an item skipped for another
  reason — it already has work, it fails `--kind` or `--updated-within`, no
  recipe applies — costs nothing against the bound. A ranking file that is
  absent, empty, or unreadable is not an error: the sweep falls back to
  today's newest-first order, and the reading says which of the three
  happened; an id the file names that matches no item is skipped and named,
  not fatal. The reading says which file it used and how old it is, in prose
  and in `--json` alike. With no ranking configured and no `--limit`,
  behaviour and output are exactly today's.

- **Every file is measured against a budget set by how it is read**
  ([§FS-012-file-size](functional-spec/FS-012-file-size.md#fs-012-file-size-every-file-is-measured-against-a-budget-set-by-how-it-is-read), PR #23). `.agents/fissile.toml` gives this tree its first
  file-size budgets and `fissile check` enforces them, in the pre-commit hook
  against the files a commit touches, in `just check`, and in a `fissile` job in
  CI against the whole tree. The budget follows the reader rather than the file
  extension: a `§`-declared spec is reached by an ID and fetched a section at a
  time, so it gets 750 soft and 2000 hard lines; an entrypoint is addressed by
  nothing and loaded whole into every session, so it gets 250 and 500;
  `docs/manual.md` is neither, and gets a rule of its own; and
  `docs/changelog.md` is append-only and is not line-measured at all. Four files
  are over a hard limit today, and each is recorded in
  `docs/file-size-human-exceptions.toml` with the boundary it is missing rather
  than trimmed to fit.

- **Work about a matter with no branch can mint the branch it needs**
  ([§FS-005-dispatch.25](functional-spec/FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs),
  [§FS-004-quick-actions.7](functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout),
  PR #13). An entry that hands work over may now say which branch that work
  belongs on: `"branch": "fix/issue-{number}"`, on a configured action carrying
  `agent` or `workflow`, a project's own offer naming a workflow, the entry
  beside a workflow, or a recipe. It is a template rendered from the matter
  exactly as a brief is — and refused by name, before anything is made, where
  it names `{branch}`, `{workspace}` or `{reply}`, which are what it produces,
  something that is no field of a matter at all, or a field this matter has not
  got, and where what it renders is not a name git will take as a branch. The matter's own branch always wins: a pull request keeps the
  branch the forge recorded, and the template applies only where there is none.
  Rendering it *is* the resolution and nothing is written down, so a second
  dispatch about the same matter lands in the same workspace, and one already
  on disk is worked in as it stands. Saying it means the work needs the
  checkout, and the dispatch makes that workspace with the operation
  `ephor checkout` already is — one implementation with a third caller, not a
  second copy of it — after the hand is chosen, the machine vetted and the
  inputs answered, so a refusal still leaves nothing behind. The machine vetted
  is the one in force where the work root is already there and the one ephor
  would install where it is not, since a workspace minted in order to read its
  machine back is the thing left behind. One case escapes that, and says so: a
  runtime that installs a machine of its own when it is asked to make the store
  installs it inside the workspace just made, so a refusal on it outlives the
  mint — and it names the workspace it left, and nothing further is made behind
  it. A dry run makes
  nothing at all, including the work root and the files the runtime would be
  shown, and names the branch and the directory it would have made instead.
  Nothing is written to the registry and nothing is pushed; a project with no
  `branch_root_template` is refused by name. The offer follows on both
  surfaces: such an entry stands on a branch-less matter in the *will check out
  first* shape, and the reading carries the branch and the workspace — on the
  menu, in `ephor actions`, and on the workflows sub-screen, which is one
  reading of the same menu. A surface asking where that work would go asks
  about the same workspace: the hand shown on a row and the roster its picker
  offers are read at the work root the dispatch will use, which for such an
  entry is inside the workspace it names.

- **Work nobody has to start starts itself**
  ([§FS-005-dispatch.24](functional-spec/FS-005-dispatch.md#24-work-nobody-has-to-start-starts-itself),
  [§DA-008-a-run-follows-the-ticket](decisions/architectural/DA-008-a-run-follows-the-ticket.md#da-008-a-run-follows-the-ticket-autorun-is-a-sweep-that-starts-a-run-never-a-runner-kept-alive),
  PR #6). A recipe may now say `"autorun": true`, and a ticket written from it
  gets its run without anyone pressing a key: the reader's deliberate act
  moves one step earlier and is made once, when the recipe is adopted. What
  starts runs is a **sweep** — `ephor work run --due`, run by dispatch in the
  same breath as the ticket, by `work sync`, and by the work-sync timer —
  which reads the world rather than the ledger: every work root, the plans in
  it, the machine's own words about their states, and the runtime's lock. A
  root is due when it holds an open, unclaimed, unparked ticket from such a
  recipe and no run is live on it, so the sweep is idempotent by construction
  (a root a run already holds gets nothing, because a second run there would
  only wait for the first) and safe to invoke as often as anything cares to.
  A ticket a hand appended counts exactly as a dispatched one: the recipe is a
  fact about the ticket, read from the ledger where ephor wrote it and from
  the id's own `<recipe>-<n>` shape where it did not. Everything dispatch
  refuses before writing a ticket, starting refuses before running one —
  including a working tree standing on another branch, which the ledger now
  records the branch to check. A start that fails is remembered as ephor's own
  act (never as work state) and that root rests before it is tried again,
  doubling to a two-hour cap, so a runner that refuses cannot become a spawn
  loop. Where the binding has no detached shape the sweep starts nothing and
  says so: a run nobody asked for must not take a terminal. Silence still
  means the key — nothing autoruns unasked.

### Changed

- **A registry match to the project's main branch is not a matter's own
  branch, for placement**
  ([§FS-005-dispatch.25](functional-spec/FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs),
  [§FS-005-dispatch.1](functional-spec/FS-005-dispatch.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for), PR #40).
  An issue or pull request the registry matched only to the configured
  `main_branch` — the trunk every workspace is grown from — used to keep that
  branch like any other of the matter's own, so a `branch` template was
  skipped and dispatch resolved inside the main checkout itself. Such a
  matter is now placed exactly as a branch-less one is: a `branch` template
  mints its own workspace and the offer follows on both surfaces, and work
  that edits the change with no template is refused — naming the main branch
  it declined rather than calling the matter unmatched. Work that only reads
  the change — a reply, a review, anything needing no checkout — still runs
  where the matter's code is, the main checkout included, and lays its ticket
  beside the project all the same: laying a plan is a write, and every write
  is placed. A forge-recorded branch that happens to equal `main_branch` is
  the forge's own fact and keeps winning.

- **The shipped `implement` recipe isolates branch-less issue work**
  ([§FS-005-dispatch.1](functional-spec/FS-005-dispatch.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for),
  [§FS-005-dispatch.25](functional-spec/FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs), PR #38).
  With no configured replacement, an issue with no branch is dispatched on
  `fix/issue-{number}` inside a workspace minted when the project has
  `branch_root_template`; without branch workspaces it is refused by name with
  configuration guidance instead of writing at the project root. Existing
  matter-branch precedence and configured `implement` replacements are
  unchanged.

- **`src/work/mod.rs` takes the first seam its size budget names, and the
  boundary check learns that shape**
  ([§FS-012-file-size.1](functional-spec/FS-012-file-size.md#1-the-budget-follows-the-reader),
  [§REQ-001-boundary.5](requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).
  The dispatch module's inline `#[cfg(test)] mod tests` moves whole to
  `src/work/mod_tests.rs`, attached by `#[cfg(test)] #[path =
  "mod_tests.rs"] mod tests;` — a pure move, no test edited, the same 891
  tests — which brings the file back inside the ceiling
  `docs/file-size-human-exceptions.toml` records for it. Because the
  `#[cfg(test)]` now sits on the attachment rather than in the file,
  `scripts/check_boundary.py` reads a `<name>_tests.rs` sibling as a test
  body in its entirety, so fixtures that name a product stay the examples
  the law already permits. (PR #28)

- **Work that edits the change, about a matter on no branch, is refused on the
  command line instead of written at the project root**
  ([§FS-005-dispatch.25](functional-spec/FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs),
  [§FS-005-dispatch.6](functional-spec/FS-005-dispatch.md#6-dispatch-is-offered-where-it-would-work-and-refuses-where-it-would-not),
  PR #13). A recipe with `needs_checkout` or an entry with `requires_checkout`,
  dispatched about a matter no branch could be found for on a project whose
  checkouts are one per branch, used to resolve its work root from the project
  root — which on such a project is the directory those workspaces sit in and
  holds no change to edit, so an agent started there stood in the wrong tree.
  It now refuses, naming `branch` as the way out. The menu has always blocked
  exactly that entry, so this is the two surfaces coming to agree
  ([§REQ-002-parity.2](requirements/REQ-002-parity.md#2-parity-runs-both-ways)). Everything with a branch of its own, every project
  keeping one checkout at its root, and all work that reads the change rather
  than editing it are placed exactly as before.

- **A ticket a run has in hand says so on its own row**
  ([§FS-005-dispatch.23](functional-spec/FS-005-dispatch.md#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about),
  PR #6). *Open* and *being worked on right now* were one yellow `⚙`: the
  ticket an agent was inside of and the ticket nothing had picked up looked
  identical, and telling them apart meant leaving for the board. A ticket a
  live run holds is now `▶` in green, one it will reach says `· queued`, and a
  live run that has gone silent carries `· quiet Nm` — the board's own words,
  narrowed to one matter rather than invented a second time. It asks nothing
  new of the world: the liveness is the lock the watch already probes and the
  holding is the run's own record ([§FS-005-dispatch.15.2](functional-spec/FS-005-dispatch.md#152-what-a-run-is-doing-is-read-from-the-runs-own-stream)), taken once per work
  root across the whole feed rather than once per matter. `running`, `queued`,
  and `quiet` join the readings too, so a command sees what a row does.

- **What a live run is doing is read from the run's own stream, not from a
  journal that outlives every run**
  ([§FS-005-dispatch.15.2](functional-spec/FS-005-dispatch.md#152-what-a-run-is-doing-is-read-from-the-runs-own-stream),
  [§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place),
  PR #6). The runtime writes a record of each run — truncated when that run
  starts, one line per structural move, numbered and terminated — and ephor
  now reads it wherever it asks which tickets a run has in hand: the board,
  the rows beneath a matter, and the check a cancel makes. It removes a whole
  class of inference rather than adding a source: the transition journal is
  append-only *across* runs, so an assignment a crashed run never released had
  to be argued down from the ticket's own state and the age of a log against
  the birth of the lock, and a witness to one run needs none of that. A run
  that died mid-slot is now read as **dropped** from its own unreleased
  assignment and its missing end record; a run that finished leaves nothing
  open at all. The journal stays the floor and is read unchanged where a
  runner writes no stream, so nothing here becomes a requirement on the
  binding, and liveness is still the lock and only the lock. The stream joins
  the change gate's fixed handful of timestamps, so a run that has written
  only there still surfaces within moments
  ([§FS-005-dispatch.15.1](functional-spec/FS-005-dispatch.md#151-the-board-keeps-itself-current)).

- **The workspace a row says is missing is made from that row, and what ephor
  makes is a place work can go**
  ([§FS-004-quick-actions.7.1](functional-spec/FS-004-quick-actions.md#71-a-workspace-that-is-there-is-still-owed-its-store),
  [§FS-004-quick-actions.7.2](functional-spec/FS-004-quick-actions.md#72-the-offer-is-a-key-on-the-row-that-says-the-workspace-is-missing),
  [§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live),
  PR #6). A row that says `∅ not checked out` now answers **`C`** with the
  checkout, on the matter's row, on the work rows beneath it, and on the
  branch's own — including the rows under *(not linked to a branch)*, which is
  where a matter whose branch nobody has checked out usually sits. It runs the
  same entry the `x` menu holds, the project's own `checkout` command
  included, so the key and the entry cannot come apart; the footer teaches it
  only where the workspace is actually missing.

  **And the workspace it makes is one the runtime can be handed work in.**
  ephor used to write the work store's files itself out of its own idea of what
  a runtime project is. It now asks the **runner** for one — `<runner> init
  --here <work root>` — at the work root ephor resolves rather than wherever
  the runner would put it left to itself, and installs ephor's state machine
  beside what the runner wrote. The store still ignores itself, so `git status`
  in the checkout is unchanged by it, whatever the runner's own project says
  about version control. Where the runner is not on `PATH` the checkout is
  unharmed: ephor writes the store it can and says what it could not do.

  **A workspace that was already there is owed the same.** *Already checked
  out* answered the question about repositories and not the one about work, so
  a workspace made before ephor made stores at all — or made by a project's own
  `checkout` command — held every repository it should and had nowhere for a
  plan to land, with nothing to do about it. Asking for the checkout again now
  makes whatever is missing and says so, and `--json` carries the same answer
  under `store`.

- **A workflow's inputs are answered on one screen, and what has a known set
  is chosen from it**
  ([§FS-005-dispatch.19](functional-spec/FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here),
  [§DA-006-hands-fill-a-workflows-targets](decisions/architectural/DA-006-hands-fill-a-workflows-targets.md#da-006-hands-fill-a-workflows-targets-who-a-workflows-agents-are-is-ephors-answer-not-the-workflows),
  PR #5). Laying a workflow down used to ask only about the holes: one missing
  scalar was one line typed where you stood, and anything more was a JSON file
  in `$EDITOR` — which meant the answers ephor had resolved, the workflow's own
  defaults most of all, went in unseen. Choosing a workflow now opens **every
  input on one screen**, each row carrying the answer the five steps reached
  and the step it came from, and each one changeable there. Where the values an
  input can take are known it is **picked** rather than spelled: a flag has two,
  an input that names who does the work has the roster — narrowed, and saying
  who is unavailable and why — at the effort the hand declares, and an input
  whose own check is a plain set of words has those words. An input wanting
  several hands takes several, in the order you take them. `p` asks the runtime
  what it would write before it writes it; `e` still opens everything resolved
  in `$EDITOR`, which is where a record is answered; and the last row lays it
  down, refusing while a required input is unanswered and naming it.

  **An input naming who does the work is now recognized where it always
  should have been.** Which inputs those are was read out of the workflow's
  `template.yaml` — so it worked for a workflow kept as a directory and for
  nothing else, and every workflow the runtime ships inside itself had its
  targets left at its author's models, quietly, which is the one thing
  [§DA-006-hands-fill-a-workflows-targets](decisions/architectural/DA-006-hands-fill-a-workflows-targets.md#da-006-hands-fill-a-workflows-targets-who-a-workflows-agents-are-is-ephors-answer-not-the-workflows) refuses. ephor now reads it from the
  runtime's own listing where the listing says it (rhei publishes the whole
  input schema as of its next release), keeps the manifest reading as the
  fallback for an older one, and understands a **list** of execution targets as
  an input answered with hands too. `ephor work lay --set review_targets='["luna","sol"]'`
  answers such an input with several from the command line, the same call the
  screen makes.

- **A run of the runtime starts beneath the screen, and is watched by
  attaching**
  ([§FS-005-dispatch.20](functional-spec/FS-005-dispatch.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching),
  [§AR-007-runtime](architecture/AR-007-runtime.md#ar-007-runtime-the-runtime-adapter-and-rhei-as-its-shipped-binding), PR #4). Pressing `R` used
  to hand the whole interface to one run for as long as the work took — and the
  work was handed over precisely so that nobody had to stay. `R` and
  `ephor work run` now start the run **detached**, in a session of its own that
  outlives the terminal, and answer with one line saying the run began and what
  it is called. The root turns live on the board from the lock as it always
  did; nothing new is watched. `ephor work run --watch` keeps the old
  behaviour, and a runner with no detached shape gets it unasked, saying so.

  **A run has an identity, and it is the binding's.** A live run names itself —
  an id, and the address of its control while it serves one — read from the
  descriptor the runtime leaves beside its lock and never from anything ephor
  remembers having started, so a run somebody began in another terminal is
  named and reached exactly like one ephor dispatched. The board says the id on
  the row, the work screen says it on the operation, and
  `ephor operations --json` prints it beside the control address, the runner's
  own attach command, and the runner's own **stop** command — shown, never run:
  a key that stopped a run would be a channel to the run ephor promised never
  to hold.

  **Watching is attaching.** `a` on the operations board, and
  `ephor operations attach <run>`, open the binding's own surface on a live
  run; leaving it detaches and never stops the run.

- **What is already going is shown where it could be started again**
  ([§FS-005-dispatch.21](functional-spec/FS-005-dispatch.md#21-what-is-already-going-is-shown-where-it-could-be-started-again),
  [§FS-011-command-line.8](functional-spec/FS-011-command-line.md#8-what-is-going-is-said-and-the-way-in-is-printed),
  PR #4). The menu said what could be done and the board said what was being
  done, and neither said the other: opening the menu on an item whose rebase was
  already replaying showed the rebase as something to start. Every entry with
  work going about its subject is now **marked running and set apart** — first,
  under a line that says so, a step further in, in one colour used for nothing
  else on that screen — with how long it has been going and what it is at right
  now: the job's own last line, the ticket a run holds and the state it is in,
  *waiting on you* where the ticket it opened is parked — with a run still at
  the gate or without one — and *queued* where the root's run will reach it.

  Found by looking, never remembered from the keypress: a job is a held lock
  and a record naming the entry it came from (a job now records that, and the
  branch on a branch row), a run is a held lock and the descriptor beside it,
  and the row that would make a branch workspace is running while the job whose
  first step is making it holds its own. A second ephor sees the same rows, and
  a job that died is not running whatever started it — and the whole reading is
  taken once for a menu rather than once for each of its rows.

  **Pressing a running entry opens it.** `Enter` (or `l`) goes to the thing
  that is running — a job's log followed as it writes, a run attached, a window
  brought forward — and the footer says *open* rather than *run*.
  `ephor actions [--json]` carries the same mark with the same facts and prints
  **the way in**, and `ephor actions open <id>` is that key as a command,
  refusing by name where the entry has nothing going. A parked question opens
  where the answer belongs: the run still standing at its gate, and the plan the
  question is written in where none is.

- **A window of the reader's own, where one is bound**
  ([§FS-005-dispatch.22](functional-spec/FS-005-dispatch.md#22-a-window-of-the-readers-own-where-one-is-bound),
  [§AR-002-summons.6](architecture/AR-002-summons.md#6-windowed-the-readers-own-window),
  [§DA-007-window-is-a-bound-opener](decisions/architectural/DA-007-window-is-a-bound-opener.md#da-007-window-is-a-bound-opener-a-window-of-the-readers-own-is-a-bound-opener-with-the-terminal-as-the-floor),
  PR #4). ephor has one terminal and is sitting in it, and handing it over
  stays the floor — but a reader inside a multiplexer, or in a terminal that
  opens windows on request, had to make the better move by hand. The window is
  now a **seam**: two commands, one that opens a window running a given command
  and prints a handle, one that brings a handle forward. `window` under
  `defaults` in `status.json` names which — a shipped binding (a terminal
  multiplexer's new window, and two terminals' remote-control spawn) or a pair
  of commands of your own — and with nothing configured ephor recognizes the
  environment it was started in from the variable each product sets for exactly
  this, never by spawning one to find out. Nothing bound and nothing recognized
  means the terminal, with the line saying so.

  An action or an offer that *is* a program you type into says `"window": true`
  and runs there instead of taking the terminal: a job like any other, with the
  lock as its liveness and the handle in its record, so it is a row that says
  *running* and opens to the program rather than something ephor handed the
  terminal to and forgot. Such a job leaves **no log** — what its program wrote
  is on a screen you were watching and is not duplicated into a file — so the
  window is its inspection, and every surface says so rather than offering an
  empty file. Attaching to a run goes through the same opener, from the key and
  from `ephor actions open` / `ephor operations attach` alike. ephor opens a
  window and brings one forward; it never closes one and never ends what is in
  it.

- **Everything the screen holds is a command, and every answer has a machine
  form** ([§REQ-002-parity](requirements/REQ-002-parity.md#req-002-parity-every-ability-is-reachable-without-the-screen-and-every-answer-has-a-machine-form),
  [§FS-011-command-line](functional-spec/FS-011-command-line.md#fs-011-command-line-every-ability-is-a-command-and-every-answer-has-a-json-form),
  [§AR-009-surfaces](architecture/AR-009-surfaces.md#ar-009-surfaces-one-api-beneath-both-surfaces-and-one-schema-per-answer), PR #2). The interactive
  interface is a convenience over the watch, never the place the watch lives —
  but a good third of what it could do had no command behind it, so a runtime
  ephor hands work to could read a feed and not finish a move. That is now a
  **law**: every ability the interface offers is also a command, every command
  that prints a reading takes `--json`, and a key on neither the ability list
  nor the stated presentation list fails the build (`just check`, and CI).

  **New commands.** `ephor actions` prints the menu a matter or a branch
  carries — the source's offers, ephor's, the project's, your own, the work
  that can be handed over — in provenance order, each row saying whether it can
  run and, where it cannot, the ladder's own sentence. `ephor actions run <id>`
  runs one, in the same place and with the same `EPHOR_*` context a keystroke
  would, with `--hand` for who gets the work, `--set` for a workflow's inputs,
  `--yes` where an entry asks to be confirmed, and `--command` for the freehand
  row. `ephor branches` says where every branch stands — checked out or not,
  how far behind its base and its published copy, and as of when.
  `ephor operations` is the whole board, the runtime's half and ephor's own
  jobs together. `ephor thread` prints a matter's conversation, numbered, and
  `ephor react`, `ephor tick` and `ephor reply` take those numbers — including
  sending the reply a run drafted, which until now only a keystroke could do.
  `ephor work offers` is one matter's work screen.

  **`--json` everywhere.** `react`, `tick`, `reply`, `failures`, `restart`,
  `rebase`, `checkout`, `mark-read`, `refresh`, `list`, `validate` — including
  `validate --manifest` — `job log`, `check`, `update`, `ensure-agents` and
  every `work` subcommand, `work states` included, now answer as JSON, printing
  what they *changed* rather than a re-description of the request: which
  repositories rebased and which conflicted, what a restart asked for and what
  it skipped, which tickets opened, which managed workspaces were rewritten,
  and — where a replay's conflict was handed over — the ticket that opened for
  it. Under `--json` the reading is alone on standard output: notes and
  progress go to stderr, and so does the output of anything ephor runs for you,
  including the runtime under `work run`, an entry under `actions run` and a
  project's own check verbs under `check`. A command that is *refused* answers
  too, with `{"ok": false, "says": …}` on standard output and the exit code it
  always had — a script reading only the reading used to see an empty stream,
  which is also what a move that worked silently looks like. `ephor doctor
  --json` prints **one** document with both passes in it rather than two
  objects on one stream, which no parser could read. Every shape is published —
  `ephor schema views` prints the document, a walk of the command tree fails
  the build on a `--json` that declared none, the end-to-end suite runs every
  one of those commands and validates what it actually prints against its own
  entry, and they are a stability surface like the manifest and forge schemas.
  `failures` and `restart` also take `--item ID` now, instead of only the four
  coordinates a quick action passes them; `job log` takes `--follow`, and with
  `--json` it waits for the job to end and then answers with the whole log and
  the state it ended in, which is how a script waits on a job.

  **Refused rather than dropped.** `--hand` on an entry that hands no work
  over, `--set` on one that lays no workflow down, and a configured entry or
  recipe whose id claims one of the names ephor mints for its own rows
  (`@command`, `@workflows`) are all refused by name, where you can still see
  what you wrote. `--dry-run` on `actions run` reports the whole chain an entry
  would walk — the checkout first where the branch workspace is missing, then
  the entry itself, in the workspace that checkout is about to create.

  **One implementation underneath.** The menu's assembly, the capability
  gating, the branch standings, the board's phrasing, the conversation walk and
  every move now live in `src/api/`, below both surfaces: the interface renders
  what a command prints, so the number `ephor thread` gives a message is the
  number `ephor react --message` takes, and a greyed row and a JSON `refusal`
  are the same sentence. One of everything, too — one session, one work ledger,
  one read of the registry per run. The work screen and `ephor work offers` now
  answer from the same derivation, so a row one of them offers is a row the
  other has — workflows included, which the screen used to drop; `+`, `t` and
  `p` in a conversation go through the same calls `ephor react`, `ephor tick`
  and `ephor reply` do, so posting a reply retires the draft on both surfaces
  or on neither; the interface's dispatch writes the session's own ledger
  rather than a second copy of it; and an entry runs the same chain, in the
  same place, whether it takes your terminal, runs beneath it, or is only being
  described by `--dry-run`. Where a reading cannot be given at all — a project
  the registry cannot place — the reason travels with the empty list rather
  than being the empty list.

- **A workflow the runtime offers is an action**
  ([§FS-005-dispatch.19](functional-spec/FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here),
  [§FS-006-project-interface.9](functional-spec/FS-006-project-interface.md#9-offers-the-projects-actions),
  [§DA-006-hands-fill-a-workflows-targets](decisions/architectural/DA-006-hands-fill-a-workflows-targets.md#da-006-hands-fill-a-workflows-targets-who-a-workflows-agents-are-is-ephors-answer-not-the-workflows)).
  The runtime carries more than a place to put tickets: it carries whole
  **workflows** — parameterized plans that lay down tasks of their own, under a
  machine of their own, with fan-out and gates ephor never wrote. Using one
  meant leaving the watch, remembering another vocabulary, and coming back with
  a workspace nothing here knew about. They are now entries in the same menu,
  selected by the same `when` language and refused in the same sentence. An
  **entry** is what makes a workflow an action, and it may be written in any of
  three places: beside the workflow itself, in the project's manifest, or in
  your own configuration — ranked by where the workflow was found, so what the
  runtime ships ranks with what ephor ships and the project's with the
  project's offers.

  **Its inputs are answered here.** Five steps, each displacing the ones after
  it: what you answered for this laying alone, what the entry says, ephor's
  answer for an input that names who does the work, the workflow's own default,
  and — where an input is required and still unanswered — you, asked or refused
  by name. What the entry says is data rather than prose: a string is rendered
  with the matter's own fields (`{branch}`, `{repo}#{number}`, and `{dossier}`
  and `{item}` for the files ephor writes beside the plan), anything else is
  passed on as it stands, and strings nested inside a list or a record are
  rendered too. One missing scalar is one line typed where you are standing;
  anything more, or anything wanting a list or a record, opens a file in your
  editor with everything already resolved in it.

  **Who does the work is ephor's answer, not the workflow's.** Half of a
  workflow's input surface is usually its agents, each defaulted to whatever
  model its author was running — so `work.hands`, the hand picker and
  `work.permitted_hands` would all have stopped applying at exactly the
  keystroke that mattered. An input the workflow declares an execution target
  for, or that an entry lists under `hands`, now resolves through the same
  seven steps a ticket's hand does and is refused under a narrowing wherever it
  was named, the workflow's own default included.

  What lands is a **plan of its own beside the matter's**, never a ticket
  inside it, written into the matter's own work root — so the operations board
  finds it by looking like every other plan there, and a workflow and a ticket
  about one change share the root's one run rather than editing the same tree
  at once. Laying one down writes files and nothing else
  ([§FS-005-dispatch.7](functional-spec/FS-005-dispatch.md#7-handing-over-work-is-the-readers-move-and-stays-inside-the-machine)):
  running it is the move after, from the board. A second laying of the same
  entry is a second record, not a correction of the first. From the command
  line, `ephor work workflows` lists what the runtime offers and what each one
  takes, and `ephor work lay <entry> --item <id> [--set <input>=<value>]…`
  lays one down, with `--dry-run` showing what would answer every input before
  anything is written. With no runtime bound there are no workflows, in the
  *workable* rung's own words — what the binding offers is the binding's to
  say.

- **A gate can be restarted from the inbox, in two shapes**
  ([§FS-004-quick-actions.9](functional-spec/FS-004-quick-actions.md#9-a-gate-is-offered-the-restart-in-two-shapes),
  [§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)).
  Reading what failed answered the question a red gate asks; it never answered
  the one you usually have about it, which is *was that even me*. A runner
  died, a mirror was unreachable, the same flake landed again — nothing about
  the change is wrong and what it needs is another run, and finding which
  button to click for that took a browser tab and five minutes. A gate now
  carries `⟳ restart what failed` and `⟳ restart the whole gate`. They are two
  entries because they are two diagnoses: one job died on infrastructure, or
  the merge commit itself is suspect and the greens are as untrustworthy as
  the reds. A red gate gets both; a gate that is not red keeps only the whole-
  gate entry, which is the one that still has something to do there; an item
  with no gate gets neither. Restarting everything asks first, and both run
  beneath the screen as jobs
  ([§FS-005-dispatch.17](functional-spec/FS-005-dispatch.md#17-a-move-that-needs-nobody-runs-beneath-the-screen)):
  a restart asks nothing and the gate answers minutes later.
  On **GitHub** it is native and per-job — the pull request's head commit is
  resolved, its workflow runs are re-run, and `--failed` is GitHub's own
  *rerun failed jobs*; a check that is not a workflow run is named rather than
  silently skipped. For a forge reached through an **extension**, `restart` is
  a new protocol subcommand carrying the scope as the caller's word, answered
  with what was actually asked for — a count, or the forge's own sentence
  where a whole-gate start is executed asynchronously and no count exists.
  Omitting the count is an answer; a zero there would read as *nothing needed
  restarting*. `ephor restart --scope failed|all` is that same ask from the
  command line, for the sources that answer it — GitHub's entries reach `gh`
  directly, exactly as `see the CI failures` always has.

- **A move that needs nobody runs beneath the screen**
  ([§FS-005-dispatch.17](functional-spec/FS-005-dispatch.md#17-a-move-that-needs-nobody-runs-beneath-the-screen),
  [§AR-002-summons.5](architecture/AR-002-summons.md#5-detached-the-job)).
  Pressing `⤴ rebase onto master` handed the whole interface to a replay that
  asks nothing: minutes of output nobody has to read while it is produced, no
  watch underneath it, and a keypress at the end to get the screen back. The
  rebase entries now run as **jobs** — a process of their own, in a process
  group of their own, so quitting ephor or closing the terminal does not take
  one down — and the interface stays exactly where it was. A live job is a row
  on the operations board (`;`) saying what its log last said, `e` or `Enter`
  there reads the log with `less +F` following it, and when the job ends its
  one line lands on whatever screen is being read. Afterwards the record stays
  with the item: the work screen (`w`) lists what ephor ran there and `L`
  reads the newest one. From outside the interface, `ephor job list` and
  `ephor job log <id>` answer the same questions — a job outlives the ephor
  that started it, so it is answerable without one. Liveness is a lock, as it
  is for a run: a job that died holds none and wrote no outcome, and is
  reported as died rather than as running. The chain travels with the job — an
  entry needing the branch workspace runs its checkout as the job's first
  step, with the directory verified rather than trusted — and a conflict is
  still handed over as a ticket
  ([§FS-005-dispatch.12](functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
  Configured actions and project offers keep the terminal unless they say
  `"background": true`: an entry may be `lazygit`, an editor, or a pager, and
  one of those started beneath the screen is a program nobody can type into.

- **A source follows a label**
  ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)).
  `github-issues` could ask two questions — the issues you opened, the issues
  you are in — and neither reaches a repository where the work you follow is
  named by the project rather than by you: on one with 518 open issues the
  role searches flood, and the 17 labelled `priority` you actually watch
  include ones nobody has spoken in, which is exactly what those searches
  filter out. A block now takes `labels`, and each label is one `gh search
  issues --label <name> --state open` — open only, since following a label is
  following work and the closed would spend the `limit` on history. What it
  finds arrives whoever is in it, under the role its author gives it: **My
  Issues** where you opened it, **Participating** otherwise. `authored` joins
  `participating` as a switch, so a source can follow labels alone; a block
  with both off and no label asks nothing and is refused when it is read,
  rather than answering "nothing" forever. And a label search that comes back
  with exactly `limit` issues fails the source, naming the count, the label,
  and the two ways out — it delivered a prefix nobody can size, and a queue
  shown as a fraction of itself reads as the queue
  ([§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
  The two role searches are otherwise unchanged, and still truncate silently
  at their own limit.

- **A ticket can be taken back**
  ([§FS-005-dispatch.16](functional-spec/FS-005-dispatch.md#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so),
  [§DA-005-cancel-is-the-runtimes-move](decisions/architectural/DA-005-cancel-is-the-runtimes-move.md#da-005-cancel-is-the-runtimes-move-cancelling-a-ticket-asks-the-runtime-and-never-rewrites-the-state-line)).
  The same recipe pressed twice was two runs of one fix in one checkout, and
  the only way back was an editor on the plan. `c` on the work screen now
  picks an open ticket — numbered, `j`/`k` or a digit — and asks why in one
  line; `ephor work cancel --item ID TICKET… [--why …] [--dry-run]` is the
  same from the shell, several at once. Cancelling is the runtime's own move:
  ephor asks the bound runner for the transition into `cancelled` — the plan
  language's abandonment state, the one final state that satisfies no
  `**Prior:**` — carrying the reason as the ticket's result, and relays what
  the runner answered. It never rewrites a `**State:**` line by hand, since
  the plan language reserves a written ticket's state to the runtime's verbs
  and their checks and trail. The plan keeps the ticket, marked `⊘` with the
  reason beneath it, and the row's badge says `⊘ recipe · cancelled` where
  that is the last word; the operations board counts cancelled apart from
  finished. Refused before the runner is asked, one sentence each: a ticket a
  live run holds (the run's to finish; the lock and the journal say so, as
  the board reads them), one already over, a machine declaring no final
  `cancelled` state — named with what to add — and, with no runtime bound,
  the workable rung's own sentence, exactly as `R`. Cancelling names the open
  tickets ordered after the one taken back, which will not start while it
  stands cancelled; and a ticket ephor appends afterwards — a reopen, an ask
  — is ordered after the last one *not* cancelled, so ephor's own chain never
  hangs off abandoned work. The shipped machine and both example machines
  declare `cancelled` and a `from: "*"` transition into it; a `states.yaml`
  already installed is never rewritten and has to gain both by hand.

- **The replay onto the published copy has its environment spelling**
  ([§FS-004-quick-actions.8](functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)).
  Every argument of `ephor rebase` could arrive as an environment variable —
  which is how a state machine passes `{meta.*}` — except the one that picks
  the other rebase, so a program state could ask for `--onto` via `ONTO` and
  could not ask for the published-copy replay at all. `UPSTREAM` set to any
  non-empty value now asks for it. And because the flag parser's
  `--upstream`/`--onto` conflict cannot see the environment, the same refusal
  is repeated across both spellings: asked for together in any combination,
  the rebase refuses rather than silently preferring one and running a
  different rebase than the state asked for.

- **An agent-only hand actually binds**
  ([§FS-005-dispatch.14](functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).
  A hand naming an agent and no model — which is every hand on a machine whose
  runtime settings declare no model profiles — was chosen and then did
  nothing: the plan language pins a model, so the ticket carried no line and
  the runtime picked as if nobody had spoken. Now the choice binds in one of
  two spellings, never both. A hand carrying a model is written on its ticket,
  as before; a hand that cannot be rides the run instead, as the runtime's own
  `--agent` / `--agent-mode` flags on `ephor work run`, resolved when the run
  is invoked. An effort-less choice is settled by what the hand declares —
  asked plainly where it declares no efforts, completed with a note where it
  declares exactly one, refused with the list where several — so neither
  spelling ever travels effort-less against an agent that has efforts: a bare
  selector would run without any mode, and a bare `--agent` flag would fail
  the run outright where the state machine's mode is not the agent's. The
  flags ride only where they can re-aim nothing: a ticket with a full target
  line is resolved from that line alone and rides beside them, one pinning a
  bare model would take its carrier from them and runs the plan unflagged
  with the reason said, and a claimed ticket is not the run's to advance at
  all. **On a machine with no model profiles, `work.hands` now bites with no
  further configuration — name a hand and run through `ephor work run`; or
  declare a model profile in the runtime's own settings, which pins the hand
  per ticket everywhere and needs none of this.**
- **The reader picks the hand, once**
  ([§FS-005-dispatch.14](functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).
  The first of the seven steps — what the reader picked for this dispatch
  alone — existed only as an unfed slot in the resolution. `t` on a menu
  entry that hands work over now opens the picker: the roster's hands in one
  column and the selected hand's declared efforts in a second, absent where
  it declares none — the common shape on a machine with no model profiles.
  An unavailable hand is listed with its reason and cannot be chosen; a hand
  the project's `permitted_hands` excludes is not listed at all; with an
  empty roster there is no picker and the entry dispatches unchanged. The
  same pick rides the command line as `--hand <hand>[:<effort>]` on `ephor
  work dispatch` and on `ephor rebase --dispatch` (env spelling `HAND`), so
  the key and the command are one operation. A pick lives exactly one
  dispatch: nothing records it, and the next dispatch of the same action
  resolves from the tables again. The work screen's offers now name the hand
  each would go to, in the same sentence the menu's entries carry.
- **An issue nobody has taken can count as work waiting**
  ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities),
  [§FS-003-feed-categories.4](functional-spec/FS-003-feed-categories.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).
  Whether an issue awaits you was read off the conversation — last word, or a
  reaction, or a forge-tracked task — and the conversation is exactly what
  cannot say this: an issue somebody filed and nobody picked up has its
  author's word last, so the rule that serves every other case reports it as
  answered. A backlog of ten open issues on your own project read as ten
  finished things. It is the same shape as a review asked for and never given:
  being waited on leaves no message behind. So assignment crosses the interface
  as a fact — `Issue` reports whether anyone has taken it, and an
  implementation with no notion of assignment omits it and is never counted as
  unclaimed — and an unclaimed issue is a fourth form of pending beside the
  three above. Whether it applies is the source's to say (`unclaimed: true`,
  off by default), because *unclaimed* only means *yours* where you are
  answerable for the backlog: on a project you run it is the whole point, and
  among issues you once commented on somewhere it would turn every stranger's
  open bug into your work.
- **A forge can report the review you gave**
  ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)). Only the
  forge knows it: a reviewer list says who was asked and a conversation says
  who spoke, and neither says who answered — an approval leaves no message
  behind, and a reviewer who commented at length has still not approved. So a
  reviewing row showing nothing but `open` could not tell a change you had
  dealt with from one you had not, which is the same gap the reasons beside it
  exist to close, seen from the other side. `review` is a declared capability
  carrying one of `approved`, `changes-requested`, `commented`; `github-prs`
  reads it off the call it was already making, and an implementation that does
  not report one loses nothing. It leads a reviewing row — `[open:approved]` —
  except where a review is being asked for again, since a re-request is the
  forge saying the old verdict is no longer the answer. What a verdict
  *retires* is not decided here: this reports what you did, not what is left.
- **`ephor doctor`: the watch can be asked whether it still works**
  ([§FS-010-doctor](functional-spec/FS-010-doctor.md#fs-010-doctor-ephor-can-be-asked-whether-it-still-works-and-answers-in-one-screen)).
  Everything that makes the watch untrue is quiet — a credential that expired,
  an extension that left `PATH`, a checkout somebody deleted — and each one
  only makes a section of the feed empty, which is the one thing an empty
  section must never mean. Two passes. **The site** refreshes every configured
  source and reads each project's ladder, adding no opinion of its own: the
  sentence it prints for a missing rung is the one a greyed menu entry shows.
  **The self pass** builds a throwaway project in a temporary place and walks
  the seams against it — a forge out of process, a refresh that categorizes, a
  summons answering by code and by envelope, check verbs probed and declared,
  the checkout and the rebase, a dispatch whose ledger is read back out of its
  plan, and a local ticket store — reaching no forge and reading nothing of
  the reader's, then taking the place away. The exit code is the answer: `0`
  well, `4` degraded, `3` nothing reachable, `1` ephor itself wrong. Nothing
  is repaired on the way; a diagnostic that could not be run while unsure is
  one nobody runs.
- **`ephor capabilities`: the ladder is answerable on its own**
  ([§FS-010-doctor.2](functional-spec/FS-010-doctor.md#2-the-ladder-is-answerable-on-its-own)).
  Every rung of [§FS-006-project-interface.10](functional-spec/FS-006-project-interface.md#10-capability-rung-by-rung) was computed with a sentence
  saying why it was missing, and the only thing that ever printed it was the
  interactive inbox — so "why is this action not offered here" cost a TUI
  session. `capabilities [PROJECT] [--json]` prints it, reading the last
  refresh rather than running one.
- A cache no refresh produced now says exactly that, rather than reporting
  every source as silent
  ([§FS-006-project-interface.10](functional-spec/FS-006-project-interface.md#10-capability-rung-by-rung)).
  The *observable* rung is "at least one source **answering**", and a refresh
  that has not run is not every source having failed: one is a command to run,
  the other is a credential or a network to go and look at. The case that
  actually bites is a cache stored under an older model, which is dropped on
  load and leaves a feed with no providers in it — every project on a site
  that predates the model bump read as totally silent. `fetched_at` is what
  tells them apart, and the rung names the remedy.
- **A project's own issues are read where they live, and the rung is named
  for what it holds**
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).
  The default work root is `{workspace}/panta`, so dispatch on a
  branch-addressable project writes its plans into the branch's own tree — and
  the reader looked only at the forest root, so it never read them back. A
  project with nine stores on disk reported holding none and showed none of
  that work in its feed, one of them a ticket parked in `needs-human`; the
  single-checkout case worked only because workspace and root are the same
  directory there. Stores are verified **on disk** now, at the root and in
  every branch workspace that has one — the row names the branches somebody
  wrote down, and the work is wherever branches were actually checked out,
  which are not the same list. And `ephor checkout` initializes a store in a
  workspace it makes, so the first dispatch has somewhere to land. That is not
  an artifact required of the project: the store ignores itself, so what it
  holds is ephor's own planning state that happens to live in a checkout
  ([§REQ-001-boundary.3](requirements/REQ-001-boundary.md#3-requirements-on-a-project-are-capabilities-never-artifacts)).
- The *ticketed* rung is now **local-issues**
  ([§FS-006-project-interface.10](functional-spec/FS-006-project-interface.md#10-capability-rung-by-rung)).
  A *ticket* is what a remote tracker keys — a Jira key, a forge issue number —
  and these are the project's own, kept in its checkout, so one name for one
  thing ([§FS-001-forge-interface.3](functional-spec/FS-001-forge-interface.md#3-policy-lives-above-the-interface-never-in-an-implementation)). `requires: ["ticketed"]` still resolves,
  so nothing anybody already wrote stops meaning what it meant.
- **A released binary is self-checked by doing its job, and the profile is
  trained on the same walk**
  ([§FS-002-release.3](functional-spec/FS-002-release.md#3-artifacts)). Both "self-checks"
  ran `--version` and `--help`, which test the argument parser: a binary that
  linked, printed its version and could not refresh a source would have
  shipped. Every artifact is now held to `doctor --self-only`, which builds a
  project of its own and walks the seams against it.

  The PGO training run had the worse version of the same problem. It ran
  `list`, `validate`, `status --cached` and `feed` under `EPHOR_HOME="$repo"` —
  but `EPHOR_HOME` does not redirect the configuration, which resolves
  `~/.config/ephor` first. On a developer's machine that trained on their own
  private registry; on a release runner there is no such file, so every
  command died at "Cannot read workspaces.json" and the profile was gathered
  entirely from error paths. The training workload is the self pass now:
  hermetic, and the only workload that is also what the release verifies.
- `doctor` says what it is doing while it does it
  ([§FS-010-doctor.3](functional-spec/FS-010-doctor.md#3-two-passes-the-site-and-ephor-itself)).
  Asking every source of every project takes as long as the slowest forge, and
  the first version printed nothing at all until it was finished — a minute of
  silence that reads as hung, which is the same failure the tool exists to
  name with ephor as the source that did not answer. The site pass announces
  each project before it asks and answers it when it comes back, on the error
  stream so that what a program reads stays the report; the self pass narrates
  by being incremental instead, each check printing its own line as it
  finishes.
- The ladder counts the sources that were **asked**, not the ones the
  configuration names
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).
  A local ticket store is probed rather than configured and a shared source is
  declared once for the site, and both write a slot of their own — so a
  project with a store reported more sources answering than it had, `5/4`.

- **The tree carries nobody's employer any more, and the roadmap says what the
  law still owes**
  ([§FS-001-forge-interface.5](functional-spec/FS-001-forge-interface.md#5-no-site-specific-data-in-the-repository),
  [§RM-003-boundary](roadmap.md#rm-003-boundary-the-seams-the-law-still-owes)).
  `scripts/check-no-site-specific.sh` passes both halves for the first time: a
  private CLI in a shipped example, a source name in two test fixtures, and the
  worked example in `docs/design.md` — an employer's project, its ticket
  prefix, its build command — are all neutral now, and the packaged crate was
  already clean. `RM-001-forge-interface` is rewritten to what it still owes
  (a run against a public repository from the examples alone, and a first tag)
  rather than to a violation that has been fixed, and the new
  `RM-003-boundary` names the four places where the boundary law is still
  observed by intention rather than by construction: the `defaults.github_user`
  ledger entries, `forest` reaching the git prober, the gate seam having no
  surface, and the `beads` reader nobody has written.
- **The manual is the interface reference, surface by surface**
  ([§FS-006-project-interface](functional-spec/FS-006-project-interface.md#fs-006-project-interface-a-project-and-ephor-meet-over-one-interface-in-three-homes)).
  Every part of the project interface now has a section that cites the point it
  documents: the three homes and their resolution order (§1.1), a field table
  for `ephor.json` (§4.2.1), check verbs and how features are enumerated
  (§4.2.3), gate verbs and the forge-hosted default (§4.2.4), local ticket
  stores (§4.2.5), and the answer envelope a verb may write (§4.2.6). The
  registry page gains identity and territory and what a row believes about a
  checkout; the work chapter names the runner binding where it says what runs a
  plan; the README's tour gains what a project can say about itself. Two
  corrections fell out of the sweep: the ladder's *checkable* rung now counts a
  manifest that binds the verbs elsewhere, as [§FS-006-project-interface.5](functional-spec/FS-006-project-interface.md#5-checks-are-verbs-and-every-script-is-self-contained) always
  said it should — probing was the only thing it asked — and the vocabulary said
  four recipes ship where five do.
- **Every seam has an executable scenario, and the scenario cites its spec
  point** ([§FS-006-project-interface](functional-spec/FS-006-project-interface.md#fs-006-project-interface-a-project-and-ephor-meet-over-one-interface-in-three-homes),
  `e2e/cases`). Six cases under `e2e/cases/`, each one a story run against the
  real binary in a temporary world: a forge nobody built in, reached by a bash
  script on `PATH`; a repository checking itself with `ephor check`; a project
  whose CI answers `status`, `failures`, and `restart` from its own commands; a
  plan directory in a checkout arriving as matters; a pull request dispatched
  to tickets that sit on disk until a runtime is bound, then run by one that
  hands back a verdict and a drafted reply; and a project's menu offer refused
  in the ladder's own sentence when a rung it needs is missing. The file *is*
  the `E2E-NNN` declaration, so `grund check` holds each scenario to the
  E2E→FS rule and `grund refs` finds, for a spec point, the scenario that runs
  it. They are cargo test targets declared by path, which puts them in
  `just check` and CI and keeps them out of the published crate; `just e2e`
  runs them alone.
- **The boundary is checked by the build, not observed by reviewers**
  ([§REQ-001-boundary.5](requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter),
  [§AR-001-layers.2](architecture/AR-001-layers.md#2-where-literals-live)).
  `scripts/check_boundary.py` runs in `just check` and in CI, and it fails the
  build on two things: a product name — `gh`, `rhei`, `beads`, a chat vendor —
  spelled anywhere but the adapter that owns it, and a core module reaching
  the filesystem, a process, or a module above it. Documentation and test
  fixtures are exempt because the law exempts them, so the check reads Rust
  properly rather than grepping: comments out, strings kept, `#[cfg(test)]`
  bodies skipped. Making it pass moved real code — GitHub's reaction and reply
  writes into the GitHub adapter, the plan file's shape and the runner's
  words into the runtime module, ticket-key parsing out of the registry and
  into core — so nothing above a seam names a product any more. The four sites
  that remain are one pinned ledger entry each, and an entry that stops
  matching fails the check too: the list only shrinks.
- **Three CI steps ship, and they run from your repository alone**
  ([§FS-009-shipped-actions](functional-spec/FS-009-shipped-actions.md#fs-009-shipped-actions-what-ephor-ships-for-ci-runs-from-the-repository-alone),
  manual §9.3). `setup` installs a pinned ephor release, checksum-verified,
  and puts it on `PATH`; `validate` holds a repository's `ephor.json` — and a
  committed registry where it keeps one — to the published schemas; `check`
  runs the check verbs the repository declares, one job per feature where its
  smoke enumerates them. Each ships twice: as a composite action to compose
  with, and as a `workflow_call` workflow that is a whole job. What selects
  them is the rule that a shipped step reads repository-committed material and
  workflow inputs and nothing else — no registry, no bindings, no credentials
  for anybody's sources — so the watch-and-work loop stays on machines that
  have a site. ephor's own CI is the first consumer, running them against
  ephor's own `ephor.json`.
- **`ephor check` runs a project's own verbs from its checkout**
  ([§FS-006-project-interface.5](functional-spec/FS-006-project-interface.md#5-checks-are-verbs-and-every-script-is-self-contained),
  manual §9.3). The seam had no command line; now it has one, and it is what
  the shipped step stands on. With no verb named it runs the aggregate where a
  project declares one and whatever else it declares where it does not; the
  project's output streams to your log rather than being swallowed and
  summarized; a verb that exits `75` is parked, not failed; and
  `--list-features --json` prints what a matrix fans out over.
  `ephor validate --schema-only` is the registry half — the schema is what a
  repository can check, since the checkouts its rows name are on somebody's
  machine.

- **A project can offer menu entries, and yours still win**
  ([§FS-006-project-interface.9](functional-spec/FS-006-project-interface.md#9-offers-the-projects-actions),
  manual §7.6). The `actions` of a project's `ephor.json` now arrive in the
  action menu, between what ephor recognized and what you configured. All three
  are one shape: an `id`, a `when` selector in the language work recipes use —
  roles, gate, `needs_response`, sources, `behind`, not just `kinds` — a
  `requires` list of capability rungs, a `cwd` saying which repository of the
  forest it runs in, and `confirm` for an entry that should be asked about
  twice. Where two entries share an `id`, the later provenance replaces the
  earlier **where it already sits**, so yours beats the project's beats the
  shipped one without renumbering the menu under your fingers. A `requires`
  rung you do not hold leaves the row visible with the ladder's own sentence,
  and a requirement that is not a rung at all is refused by name rather than
  quietly met. The manifest's trust switch needs no second thought: a checkout
  trusted for descriptions only never carries offers in the first place.

- **An answer comes back as a proposal, and posting it is one move of yours**
  ([§FS-005-dispatch.13](functional-spec/FS-005-dispatch.md#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal),
  manual §8.12).
  The shipped `answer` recipe now asks the run for the reply as a file of its
  own, and ephor reads it back and shows it in the thread screen under the
  conversation it answers, marked as unsent. Where the channel says it can
  carry a reply, `p` posts it and `e` opens it in your editor first — edited or
  as it stands, sent through the same provider a reaction goes through; where
  it says nothing, the card is what you copy, which is a stated degrade rather
  than a missing key. Nothing reaches a channel on its own: a proposal that was
  posted is moved aside so the same words cannot go out twice. Work about a
  conversation still needs no checkout — its plan is written at the branch
  workspace where one resolves and at the forest root where none does — so an
  answer is dispatched on a project that could never check the branch out. A
  forge declares `replies` and puts a `reply` descriptor on the threads that
  can carry one; the out-of-process protocol gains a `reply` subcommand taking
  that descriptor and the text.
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
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).
  A `panta/` plan directory — or one the manifest points at — is now a source
  like any other: its open tasks arrive in the feed beside what the forges
  reported, under the store's own ids, attributed to the checkout's project
  because a store in a checkout is about that checkout and nothing has to
  guess. `.beads/` is recognized and reserved; a store ephor can see but
  cannot read yet reports nothing rather than pretending. Declaring a store
  does not hide a probed one — a project may keep two, and both are read — and
  the *ticketed* rung counts a declared store as well as a well-known name.
- **The gate is the project's, in three verbs**
  ([§FS-006-project-interface.6](functional-spec/FS-006-project-interface.md#6-the-gate-is-the-projects-in-three-verbs)).
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
  ([§FS-006-project-interface.5](functional-spec/FS-006-project-interface.md#5-checks-are-verbs-and-every-script-is-self-contained)).
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
  ([§FS-006-project-interface.2](functional-spec/FS-006-project-interface.md#2-the-manifest-is-offered-never-required),
  [§FS-006-project-interface.11](functional-spec/FS-006-project-interface.md#11-the-interface-is-versioned),
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
  ([§FS-008-attribution](functional-spec/FS-008-attribution.md#fs-008-attribution-every-conversation-finds-its-project-or-says-that-it-could-not),
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
  ([§FS-007-matters.2](functional-spec/FS-007-matters.md#2-same-subject-one-matter-related-subjects-linked-matters),
  [§FS-007-matters.5](functional-spec/FS-007-matters.md#5-an-event-moves-state-and-resurfacing-names-its-reason)).
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
  ([§FS-007-matters.3](functional-spec/FS-007-matters.md#3-a-discussion-is-messages-grouped-in-a-channel),
  [§FS-007-matters.5](functional-spec/FS-007-matters.md#5-an-event-moves-state-and-resurfacing-names-its-reason)).
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
  ([§FS-007-matters](functional-spec/FS-007-matters.md#fs-007-matters-the-feed-is-made-of-matters-and-a-matter-knows-why-it-is-there),
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
  [§FS-006-project-interface.10](functional-spec/FS-006-project-interface.md#10-capability-rung-by-rung),
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
  ([§FS-005-dispatch.8](functional-spec/FS-005-dispatch.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)).
  Under it, one resolver answers where a branch is checked out — the inbox's
  grouping, the action menu, dispatch, and the CLI had three implementations
  of that question and now share one
  ([§AR-004-forest.3](architecture/AR-004-forest.md#3-workspace-resolution)).
- **Running work is a summons too**
  ([§AR-002-summons](architecture/AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world),
  [§FS-005-dispatch.12](functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
  `ephor work run` and the inbox's `R` key built the runner invocation twice,
  in two places, with two ideas about what a failure was. There is one
  construction of it now and one process path to it, still run from the
  checkout the work is about
  ([§FS-005-dispatch.3](functional-spec/FS-005-dispatch.md#3-one-rhei-per-item-one-ticket-per-dispatch)),
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
  action receives ([§FS-005-dispatch.8](functional-spec/FS-005-dispatch.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)).
- **One executor for everything ephor asks of the world**
  ([§AR-002-summons](architecture/AR-002-summons.md#ar-002-summons-one-executor-runs-everything-ephor-asks-of-the-world),
  [§FS-006-project-interface.3](functional-spec/FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out)).
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
  ([§FS-006-project-interface.4](functional-spec/FS-006-project-interface.md#4-the-answer-envelope))
  — now embedded in the binary — with its `failures` and `gate` conveniences
  normalized into events, its `features` into facts, and its answer paths
  resolved against where the command ran. No answer file is a complete answer:
  the exit code stands alone, and standard output is never parsed for
  structure.
- **The decisions behind the boundary are on record, and the climbing rule is
  checked.**
  [§DA-001-runtime-bound-default](decisions/architectural/DA-001-runtime-bound-default.md#da-001-runtime-bound-default-the-runtime-is-a-bound-default-not-a-named-coupling)
  records the runtime's reversal into a bound default, superseding the old
  [§FS-005-dispatch](functional-spec/FS-005-dispatch.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)
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
  ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)). `github-prs`
  searched `--author`, `--commenter`, and `--mentions` — all three of which
  find pull requests you have *already spoken in*. A review requested of you and
  a pull request assigned to you leave nothing behind in the conversation, so
  they looked exactly like work that was none of your business. Both are now
  searched (`--review-requested`, `--assignee`), every reason a pull request is
  yours rides on the item as `raw.reasons`, and a review asked for and not yet
  given needs a response on its own — no thread rule can find that one.
- **`github-notifications`: the source whose job is to be exhaustive**
  ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities), manual §5.2).
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
  ([§FS-003-feed-categories.5](functional-spec/FS-003-feed-categories.md#5-one-subject-is-one-row-however-many-sources-reported-it)).
  Sources are meant to overlap now, so the overlap is merged rather than shown:
  the report carrying the conversation, the gate, and the role wins the row, and
  what only the thinner one knew — the reason GitHub gave for telling you —
  comes with it. Identity is the subject the forge stated, never the title.
- **The rebase ephor already knew you needed**
  ([§FS-004-quick-actions.6](functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
  The inbox has always said `3 behind` on a branch row and then left you to go
  elsewhere about it. Now a pull request whose branch workspace is on disk and
  trails its `main_branch` is offered **`⤴ rebase onto <main> (N behind)`** in
  its action menu, and `ephor rebase` is the command behind it: fetch and
  replay every repository in the checkout, an answer per repository, no forge
  and no vendor CLI anywhere in it. Uncommitted work is reported and left
  alone rather than stashed.
- **A conflict becomes work, and nothing else does**
  ([§FS-005-dispatch.12](functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
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
  ([§FS-005-dispatch](functional-spec/FS-005-dispatch.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)).
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
- **A ticket carries the item as data** ([§FS-005-dispatch.8](functional-spec/FS-005-dispatch.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)), not only as
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
  ([§FS-004-quick-actions.7](functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)). ephor knew the branch was not on disk — it
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
  or a state machine calls it ([§FS-005-dispatch.12](functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)). A project that wants its
  own checkout command still configures one and it still wins; the difference
  is only whether anybody expects to want their own.
- **A failure that was never the change's fault is restarted, not fixed**
  ([§FS-005-dispatch.11](functional-spec/FS-005-dispatch.md#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed)). The loop could recognize a dead runner or a flake in
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
  ([§FS-005-dispatch.9](functional-spec/FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)). Where a ticket sits in a state the runtime will not
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
- **Asking by hand** ([§FS-005-dispatch.10](functional-spec/FS-005-dispatch.md#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)): `a` on the work screen types one
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
  ([§FS-004-quick-actions](functional-spec/FS-004-quick-actions.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)).
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
  a refresh ([§FS-004-quick-actions.5](functional-spec/FS-004-quick-actions.md#5-a-task-is-ticked-where-it-is-read)).
  New capability `tasks`, new subcommand `ephor-forge-<name> resolve-task`.
- **A ticked box answers its thread.** Task state outranks who spoke last, in
  both directions: an open task keeps its conversation awaiting you however it
  ended, and a resolved one settles it even where every message belongs to a
  robot ([§FS-003-feed-categories.4](functional-spec/FS-003-feed-categories.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).
  Bot checklists could not be cleared before this — nobody but the bot ever
  writes in those threads, so the last word was never the reader's and never
  would be, and a pull request whose boxes were all ticked weeks ago still read
  as work.
- **ephor binds its own gate, so it holds every rung of its own ladder**
  ([§FS-006-project-interface.6](functional-spec/FS-006-project-interface.md#6-the-gate-is-the-projects-in-three-verbs)).
  `scripts/gate-status.sh`, `scripts/gate-failures.sh` and
  `scripts/gate-restart.sh` ask GitHub Actions what the gate is doing, what
  failed, and to run the failures again; `ephor.json` binds them as the three
  gate verbs. The shipped forge default would have answered too, but only for a
  matter it has cached — which is a pull request, and this project is worked by
  pushing to a branch as often as by opening one. The verbs ask about a commit
  instead, so the question is answerable from the checkout alone, on a branch
  with no pull request on it. A forge that cannot be reached is refused rather
  than reported green: silence and a clean gate have the same shape, and only
  the forge's exit code tells them apart.
- **ephor learns who can be asked**
  ([§FS-005-dispatch.14](functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).
  The runtime binding grows a fourth verb beside writing, running and reading
  back: the **roster** — every agent/model pairing the binding's own merged
  settings declare, each a **hand** with an id configuration can name, the
  agent and model it resolves to, the efforts it declares, and, where it
  cannot be used, the computed reason why. The enumeration is read from the
  binding rather than kept as a list of ephor's, which would drift the first
  time an agent or model was added on the other side
  ([§DA-004-roster-is-asked-not-configured](decisions/architectural/DA-004-roster-is-asked-not-configured.md#da-004-roster-is-asked-not-configured-the-roster-is-asked-of-the-binding-never-kept-by-ephor));
  the binding's `agent[mode]:provider:model` grammar is rendered inside
  `work/runtime/` and nowhere else, and the read mirrors the binding's own
  semantics exactly: settings overlays merge by field presence, so an
  explicit `null` clears what it inherits; a model's carrier resolves
  `defaults.agent`, then the older top-level `agent`, then the profile's own
  `default_agent`; efforts keep the order the settings declare them in. Ids
  are unique — the binding's model and agent registries are separate
  namespaces, and where a model profile claims an agent's name the profile
  holds it and the agent standing alone is listed as `@<agent>`. `ephor
  doctor` and `ephor capabilities`
  print the roster — an unavailable hand stays on the list with its reason —
  and their `--json` output becomes an object with `projects` and `roster`
  keys, where it was the projects array alone. With no runtime bound, or a
  bound one not on `PATH`, the roster is empty and says so in the workable
  rung's own sentence — a settings file that does not parse empties it too,
  naming the file — and every other rung resolves as before. No action
  chooses a hand yet: the resolution order is specced, and lands with the
  configuration and the picker.
- **A branch knows where it is published, and the row says both distances**
  ([§DA-003-upstream-is-the-published-copy](decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config),
  [§AR-004-forest.1](architecture/AR-004-forest.md#1-folds)). The upstream of
  a branch, as ephor means it, is its **published copy** — resolved per
  repository from that repository's own `HEAD`, never from the workspace
  directory's name: the recorded `@{upstream}` where it does not name the
  repository's base (tracking that names the base is `branch.autoSetupMerge`
  recording where the branch was *cut*, and read at face value it would hand
  the menu the rebase onto main a second time under another name), else the
  remote's branch of the same name — the shape `worktree add -b` leaves, the
  one bare `git rebase` fails on — else unpushed, which is an answer and not
  an error. One `for-each-ref` per repository reads ref, upstream and both
  distances at once, and the behind-main count is derived from the same fold
  so the two numbers on a row cannot disagree about when they were measured.
  A checked-out branch row now carries both, apart: `· 13 behind · ↓2` is
  thirteen commits behind the project's main branch and two behind what was
  pushed of this branch. A copy that is level, or a branch published nowhere,
  adds no arrow. No offer reads the fact yet — the rebase onto the published
  copy lands separately.
- **A branch that trails its own published copy is offered the rebase onto it**
  ([§FS-004-quick-actions.8](functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)).
  Somebody else pushing to your branch is a different fact from main moving
  under it, and now it has its own move: a second quick action beside the
  first, naming the ref and the count — `⤴ rebase onto origin/you/ABC-42-retry
  (2 behind)` — and `ephor rebase --upstream`, which excludes `--onto` because
  a per-repository ref has no branch name to give. The replay is a fold with a
  different base in every repository, each branch's own copy read off its
  `HEAD`; a repository that has published nothing is reported as *nothing
  published* and the run still exits `0`, in the same register as one already
  current, never as a refusal. Every other guard is unchanged: uncommitted work
  reported and left alone, a rebase already stopped reported as the conflict it
  is, and a conflict handed over rather than decided. This is the case bare
  `git rebase` cannot do — a branch grown by `git worktree add -b` and pushed
  carries no tracking configuration, and git refuses before it starts. The
  offer is withheld in exactly one place: where the published copy *is* the
  base for every repository under the checkout — a workspace repository parked
  on the main branch and tracking it — because two entries running one
  operation is the duplication the resolution exists to prevent; where the
  repositories disagree, both are offered and the report says what happened to
  each. Recipes and project offers gain a matching `behind_upstream` selector,
  measured from the same fold as `behind` — on a project naming no main
  branch, dispatch measures the fold all the same and nulls only `behind`,
  the same split the rows make, so a `behind_upstream` recipe offered in the
  menu is dispatchable everywhere it is offered.

### Changed

- **Recent holds the finished work that still leaves something to do, and
  nothing else**
  ([§FS-003-feed-categories.2](functional-spec/FS-003-feed-categories.md#2-recent), PR #6). Recent
  used to be every item that had finished inside the recency window, so a week
  of merged pull requests and closed issues stood at the bottom of the tree
  asking to be read and offering nothing to do about any of them — the sweep
  this project exists to retire, rebuilt daily out of a project's own good
  news. A finished matter now earns its place only by a **loose end**: an
  answer that was missing when it finished — somebody else's last word, an
  unticked task box, a notice that named the reader — a **red gate**, or
  **work the runtime still holds open** on it. A merge nobody said anything
  about leaves the feed the moment it merges, whatever the window would still
  allow.

  Settling a finished item still clears the response it owed — finished work is
  news and not a task, and nothing that counts work left to do counts it — but
  it now **keeps** that answer as the loose end instead of dropping it, which
  is what lets Recent tell the merge somebody commented on from the merge
  nobody did. The comment usually arrives as a notice from a second source, and
  the fold onto the finished row is where it is recorded. Open work is the one
  loose end the window does not age out: it stands on rows beneath the matter
  ([§FS-005-dispatch.23](functional-spec/FS-005-dispatch.md#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)),
  and a run nobody can see is a run nobody can take back.

- **Work stands on rows of its own beneath the matter, where a key can reach
  it**
  ([§FS-005-dispatch.23](functional-spec/FS-005-dispatch.md#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about),
  PR #6). A matter's work used to ride on the end of the matter's own line —
  `⚙ fix-gate · fix`, after the title, the state and the gate. It said the true
  thing and left nowhere to go with it: a line is not a row, so the cursor
  could not reach it, and the keys on the row it rode belong to the matter, so
  none of them were the work's. Taking that ticket back meant opening a second
  screen to find what you were already looking at. The work now comes off the
  matter's line and stands beneath it, one row per ticket the plan holds open —
  the one the runtime parked (`⚠ … waiting on you`) first, each saying when it
  was asked for where the ledger knows. Where nothing is open there is one row
  for what the last ticket decided, and an item that moved under its work adds
  a `⟳` row saying what changed. On such a row the keys are the work's: `c`
  takes *that* ticket back with no picking screen, `a` attaches to the run
  holding it, `e` reads the plan, and the keys that go to the matter — `Enter`,
  `o`, `w`, `x` — go there from here too. The footer says which apply, measured
  against the row the cursor is on rather than the screen
  ([§FS-004-quick-actions.2](functional-spec/FS-004-quick-actions.md#2-offered-only-where-it-would-work)):
  a row whose work is over has no ticket to take back, so it does not teach
  `c`, and pressing it says so instead of appearing to act.

### Removed

- The `bitbucket-prs` and `jira` providers, which called a vendor CLI from
  core. They are replaced by a forge extension living outside this repository;
  ephor now names no forge, tracker, or vendor tool anywhere in its source, and
  `scripts/check-no-site-specific.sh` passes.

### Fixed

- **Branch templates that need a field absent from a matter no longer turn a
  match into a dispatch refusal**
  ([§FS-005-dispatch.25](functional-spec/FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs),
  [§FS-005-dispatch.27](functional-spec/FS-005-dispatch.md#27-an-offer-that-a-selector-refused-says-why)).
  The entry does not serve that matter: menus and readings withhold both agent
  and workflow entries, dispatch sweeps step over recipes and autorunning
  workflows without recording a refusal, and `ephor work offers` explains
  which field the branch template needed. Templates with author errors remain
  visible and refuse by name. (PR #51)

- **`ephor checkout` no longer claims a local-only branch tracks the branch
  the forge has**
  ([§FS-004-quick-actions.7](functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout),
  PR #8). A repository that already had the branch was checked out and
  reported as tracking the branch the forge has whether or not the forge
  actually had a copy of it — a local branch cut from the main branch and
  never pushed came out identical to one the forge actually published. Such a
  repository is now reported **published nowhere**, naming what its tracking
  configuration records instead of claiming a copy that is not there
  ([§DA-003-upstream-is-the-published-copy](decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config));
  a repository whose branch the forge does have is unaffected. The checkout
  changes no tracking configuration and pushes nothing, either way.

- **A refresh no longer spends more of the forge's search allowance than the
  forge will give**
  ([§FS-001-forge-interface.8](functional-spec/FS-001-forge-interface.md#8-a-refresh-is-asked-in-the-cheapest-form-the-forge-offers)).
  `github-prs` asked one search per role, per repository, per project, and
  `github-issues` asked two or more of its own — so a registry of seven tracked
  projects spent forty-five requests of GitHub's thirty-a-minute search
  allowance on every refresh. It never failed outright: the sources asked first
  answered and the ones asked last were refused, so the feed came back short by
  a different amount each run, for a reason nowhere in it. Every role is now one
  aliased search in a **single** GraphQL request per source — the graph is
  metered five thousand points an hour and a whole request costs one — and the
  repositories a source watches ride in that one question rather than one
  question each. Nothing about the answer changed: each role is still its own
  search under its own alias, so a pull request still arrives with every reason
  it is the reader's ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)).

  **A pull request's branch and review decision arrive with it.** They were a
  second request each, one per pull request the reader authored, on top of the
  searches; the graph hands them over with the row that needed them
  ([§FS-001-forge-interface.8.3](functional-spec/FS-001-forge-interface.md#83-what-is-already-in-hand-is-not-asked-for-again)).

- **A finished task in a local store is no longer a matter**
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).
  The plan reader pushed every task heading whatever its state, on the belief
  that a finished one would be hidden by the recency window anyway — but a
  local task has no activity time of its own beyond its plan file's, so every
  finished task in a plan resurfaced each time the plan was touched. A project
  keeping its own work in a store showed its whole record as open issues.
  Finality is now the store's own word: the machine its `states.yaml`
  declares, or — where it declares none — the runtime's built-in default, which
  is what such a store's tasks actually run under. A task in a final state is
  history the store keeps and is not read; a store whose machine cannot be read
  reports as a source that did not answer, exactly like a plan ephor cannot
  read ([§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
- **A parked subtask is visible on an idle root**
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place),
  [§FS-005-dispatch.9](functional-spec/FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).
  The plan floor read only `###` headings and refused the dots in the
  runtime's subtask ids, so a subtask existed to the board only while a live
  run's own listing named it — a run that split a ticket, parked the
  question, and exited left a root that showed nothing at all. The floor now
  reads the runtime's whole heading grammar, taken from its parser rather
  than assumed: `###` through `######`, a dotted id with exactly one segment
  per heading level — names or canonical numbers — and a kind word matched by
  shape, since Title Case is the runtime's convention, not its grammar. A
  parked subtask keeps its row with no run and no runner, like any other
  ticket, and a new dispatch still follows the last dispatch: a subtask is
  never the prior of a top-level ticket.
- **A dead run's leavings are not a question**
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)).
  A ticket parked for a person and a ticket a crashed run was holding
  mid-slot both rendered *waiting on you*, and they ask different things:
  one is a question about the work, the other a run that wants starting
  again. The artifacts already told them apart — parked is the machine's
  gating word on the ticket's own state, a dead run's hold is the journal's
  unreleased slot under a lock nobody holds — so the board now says
  **dropped by a run that died** for the second, listed right after what
  waits on you.
- **A root whose machine cannot be read says so on its row**
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)).
  With no readable `states.yaml` the board already withheld *queued* and the
  finished count — they are the machine's words — but withheld them
  silently, leaving a zero that read as nothing done. The fact rides the
  operation now and the row wears it in so many words: `no states.yaml —
  nothing judged queued or finished`.
- The board's ticket rows say what the work is about — the matter's own
  title beside the ids — instead of carrying the title in the data and never
  rendering it
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)).
- An operation whose tickets were all filtered out still answers for the
  plan behind it: the operation carries its root's plans, so the board's
  `Enter` and `e` no longer depend on a surviving ticket to find one
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)).
- **A base nobody could resolve no longer turns tracking config into a
  publication**
  ([§FS-004-quick-actions.8](functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it),
  [§DA-003-upstream-is-the-published-copy](decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config)).
  The rule that a recorded upstream naming the base publishes nothing was
  written as *the base, where one resolved* — so in a repository where nothing
  names a base (no row main, no project main, no `refs/remotes/<remote>/HEAD`;
  the `~/c/g/master/master` shape) a branch cut from `origin/main` and never
  pushed reported `origin/main` as its published copy: the row showed a `↓N`
  that was really the distance to the base, and `--upstream` would have
  replayed onto the very ref the resolution exists to keep it off. It fails
  closed now — an unresolved base cannot clear the record of naming it, and
  only a pushed copy of the branch's own name counts. Two smaller lies in the
  same neighborhood: a replay distance that could not be measured was reported
  as *Already on top of* (unreachable today, but None is not zero anywhere
  else either — it is a refusal now that names the ref), and a declared
  repository with no working tree on disk was silently absent from the
  rebase's answer, which now names it per repository, gating nothing
  ([§AR-004-forest.1](architecture/AR-004-forest.md#1-folds)).

- **Keys and menu entries that could not do what they advertised**
  ([§FS-004-quick-actions.2](functional-spec/FS-004-quick-actions.md#2-offered-only-where-it-would-work)).
  Six of them, all the same shape — the offer was on the screen and the
  keystroke was refused. The checkout offered on a branch row ran
  `ephor checkout --item "$EPHOR_ITEM_ID"`, and a branch row has no matter
  behind it
  ([§FS-004-quick-actions.6](functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)),
  so the one row that offer was added for answered *Nothing says which branch
  to check out*; it names the branch as well now, and either half may be empty.
  That row also pointed `EPHOR_WORKSPACE` at the directory the checkout had not
  made yet — it falls back to the project root, as an item's menu already did —
  and left `EPHOR_ITEM_ID` unset, which is not empty but whatever the shell
  that launched ephor happened to hold, so a stale one could have bound a
  branch's rebase to somebody else's change; it is now said, and said empty. An
  entry marked *(unavailable)* had no verb in the footer and still took the
  menu down when chosen, to repeat in the header the reason the row was already
  carrying: it now leaves the menu standing. The work screen advertised `R run
  the runtime` on machines with no runtime bound; the screen is told the
  runtime rung's answer
  ([§AR-005-capabilities.2](architecture/AR-005-capabilities.md#2-features-declare-needs))
  and drops the key, saying why if you knew it anyway — everything else on that
  screen goes on working, because writing the ticket and running it are
  different capabilities. And `; ops` was taught only by the navigator's
  footer though the key is read on every screen: the thread, gate, work and
  board footers say it too, and the thread's reaction picker is excluded, since
  a board opened over an armed picker leaves it armed underneath.
- **An issue whose branch was on disk showed nothing saying so**
  ([§FS-004-quick-actions.6](functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
  The `✓`/`∅` marker asked whether the item was a pull request before asking
  whether its workspace was there, so an issue or a message about a change you
  have checked out was offered the rebase from its own row and shown nothing
  that said the branch was here. It follows the branch now, like the offer it
  sits beside: what the marker reports is a change on this machine, and a forge
  having filed a pull request about it is not that fact.
- **Items arriving mid-refresh were filed under no branch**
  ([§FS-008-attribution.2](functional-spec/FS-008-attribution.md#2-two-stages-one-engine),
  [§FS-001-forge-interface.7](functional-spec/FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)).
  Each project takes its place in the feed as its own sources answer, and that
  landing did everything the full reload does except place the new items on
  their branches — a pass the tree stopped doing for itself when the answer was
  moved off the draw path. So everything a running refresh brought in sat under
  *(not linked to a branch)*, and the count on the branch above undercounted,
  until the whole run finished. The landing folds the placements too; it asks
  the world nothing, which is why it belongs in the cheap half.
- **The operations board lost your place, and the cursor could leave the
  screen**
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place),
  [§FS-005-dispatch.15.1](functional-spec/FS-005-dispatch.md#151-the-board-keeps-itself-current)).
  A rebuild fires from the tick and from every refresh landing, and it kept
  only the cursor's index — so a row appearing above it silently changed what
  `Enter` and `o` acted on. The cursor now belongs to the execution root, which
  is what a row of this board is. An operation is several lines, and `j`/`k`
  moved the selection without moving the view, so the selected row could sit
  off screen; the view follows it. A run *starting* on a root the board had no
  row for writes no file the glance watches — the OS takes its lock and that is
  the whole event — so the locks of roots without rows are probed too, and one
  that came alive gets its row. The tick asked for a frame every two seconds
  whether or not anything had moved; it now asks only when something did. `e`
  on a live root whose tickets had all been filtered answered *No plan behind
  this row*, and falls back to the plan the ledger knows for that root. And
  finding each row's matter walked every project's feed once per row, rebuilding
  every matter into a row each time — one walk answers them all.
- **A project whose type declares a base per repository showed no branch a
  distance, and was never offered the rebase**
  ([§AR-004-forest.2](architecture/AR-004-forest.md#2-probes-not-declarations)).
  Such a type writes that base as a template — `{branch}`, expanded per branch
  workspace — and the forest carried it into the fold verbatim, so every
  repository was measured against a ref literally called `{branch}`, which no
  repository has. Every count came back unmeasurable, the checkout's total was
  *nothing to ask* rather than a number, no branch row carried a distance, and
  the quick action offered only on a branch that has fallen behind
  ([§FS-004-quick-actions.6](functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase))
  was therefore never offered on that project at all — silently, because a row
  showing no count and a row that is up to date look the same. A declared base
  that is still a template is passed over now, falling through to the project's
  main branch and then to what the repository's own remote calls its default:
  on the tree this was found in, one branch went from no count to
  `33 behind (ce 17, ee 16)`, and twenty-four workspaces that had never been
  measured are measured. The remote went the same way — it was the literal
  `origin` in five places, and is read off each repository once where the
  layout is already probed (the branch's own upstream, else the sole remote,
  else `origin`), so a clone whose remote is called something else is fetched,
  measured and replayed like any other, and the reports name the ref they used.
- **A branch you had checked out was still "not linked to a branch"**
  ([§FS-008-attribution.2](functional-spec/FS-008-attribution.md#2-two-stages-one-engine)).
  The inbox measured whether an item's workspace was on disk by expanding the
  project's template and looking, and then grouped that same item under the
  branches the registry row happened to name — two answers to one question,
  from two sources. So a row could read `✓` for "checked out" while sitting
  under the heading that says it belongs to no branch, and a workspace
  `ephor checkout` had just made stayed invisible to the grouping the moment
  after it was made, because nothing writes a branch back into the row. On a
  project with fourteen trees on disk and three branches written down, eleven
  branches' worth of pull requests fell into one undifferentiated pile. A
  project's branches are now the row's plus every workspace found under the
  workspace base — a bounded filesystem walk, no git process per directory —
  each named for the directory it was found in so that
  [§AR-004-forest.3](architecture/AR-004-forest.md#3-workspace-resolution)
  keeps one answer. The row still has the last word on a branch it also names,
  and a branch only the disk knows cannot widen what the project claims:
  identity is the row's alone ([§FS-008-attribution.1](functional-spec/FS-008-attribution.md#1-identity-is-declared-and-the-row-has-the-last-word)).
- **Moving the cursor in the inbox redrew at the price of a full attribution
  pass.** Two counts a row shows — how many items a branch holds, and a
  project's visible/unread/awaiting totals — were worked out while drawing,
  and a cursor move redraws without rebuilding, so each keystroke paid for
  them again. The branch count matched every item against every branch, and
  the project totals walked the feed three times over; each walk rebuilds
  every matter into a row, and each match joins an item's whole recorded
  conversation into one string. Measured on a 46-item feed with 26 branches:
  27 ms per branch row, about 273 ms for a screen holding ten of them. Both
  are settled once when the view is rebuilt now, and a draw does no matching
  and no counting
  ([§AR-008-pipeline.2](architecture/AR-008-pipeline.md#2-attribute-and-merge)).
  Placing an item also asks the matching engine once for the whole branch
  table rather than once per branch, which is the engine's own ranking rather
  than list order, and 46 items across 26 branches in 5.7 ms rather than 27.
- **An item could be filed under a branch that merely resembled its own.** The
  inbox asked each branch in turn which rows matched it, so a pull request on
  `you/ABC-42-retry` was taken by `you/ABC-42` if that branch came first — both
  carry the ticket key, and near-miss names are common once branches are found
  on disk rather than written down. Each item is now placed once against the
  whole branch list, with the branch the forge recorded winning over any that
  only resembles it, and the count on a branch row is the group beneath it.
- **A summoned command was handed paths its own shell could not read**
  ([§FS-006-project-interface.3](functional-spec/FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out)).
  A summons runs through a shell, so `$EPHOR_ANSWER` and the rest are strings
  that shell parses before anything opens them — and where the native separator
  is the shell's own escape character, a path handed over verbatim stops being
  a path. `cat > "$EPHOR_ANSWER"` wrote somewhere else, or nowhere, and the
  answer came back empty with nothing saying why, which is exactly the silence
  [§REQ-001-boundary.1](requirements/REQ-001-boundary.md#1-the-anatomy) refuses: every structured answer from every check verb,
  gate verb and offer was lost on Windows, and lost quietly. Path-valued
  variables use `/` between their segments on every platform now. Where the two
  spellings already agree this is the identity, so nothing changes anywhere
  else.
- **Nothing on Windows was ever found on `PATH`, and no bound script there was
  ever recognized as a path.** `command_exists` looked for the bare name, but
  an executable on `PATH` there is `sh.exe` rather than `sh` — so every rung
  that asks "is this runner installed" answered no on a machine where the
  runner was sitting right there, and the loop refused to run. It honours
  `PATHEXT` now. Separately, `missing_binding` decided what looked like a path
  by testing for a leading slash, so `C:\tools\check.bat` read as a bare
  command: a script that had been deleted was handed to the shell to fail as
  "command not found" instead of being refused at invocation by name
  ([§AR-005-capabilities.3](architecture/AR-005-capabilities.md#3-the-table-is-honest-about-time)). What counts as absolute is the platform's answer
  now, which is the same answer as before everywhere else. Both were found by
  the Windows leg of CI, which had never compiled far enough to run a test.
- **`--registry` was parsed and dropped by most of the commands that offer
  it.** The manual has always spelled the resolution `--registry` →
  `$EPHOR_REGISTRY` → the configured file, and one branch of `main` did that;
  every subcommand that returns early and resolves the registry for itself —
  `status`, `feed`, `refresh`, `checkout`, `rebase`, `work`, `capabilities`,
  `doctor`, `tui` — went to the configured one regardless. So
  `ephor capabilities --registry <other>` answered about a file the reader had
  not named while wearing the label of the one they had, which is worse than
  not offering the flag: it makes "try the change against a copy first"
  quietly impossible. The flag is recorded once, before anything dispatches,
  where all of them already look. Every test in the tree drives ephor through
  `EPHOR_REGISTRY`, which is why nothing caught it; the new one uses the flag
  and points the environment elsewhere.
- **The test tree could not compile on Windows, and failed on macOS.** Two
  helpers imported `std::os::unix::fs::PermissionsExt` unconditionally, so
  every integration and scenario binary failed to build on `windows-latest` —
  including the ones that summon no shell at all. The exec bit is set under
  `#[cfg(unix)]` now, the shape the seam's own tests already used. On macOS the
  temporary directories are `/var/folders/…` and really `/private/var/…`, and a
  summoned shell prints the second spelling as its `$PWD`: three tests compared
  two spellings of one directory and failed there and nowhere else. The world
  is built inside a base directory the operating system has already resolved,
  so both sides are one spelling.
- **CI installed a `grund` that could not read this tree.** The pin was 0.7.0,
  which speaks init block v4, while the entrypoints carry v7 — so the gate
  failed on the documentation rather than on anything a change did. Pinned to
  0.9.0, and the pin now says what it is for.
- **A finished matter could still be counted as awaiting an answer**
  ([§FS-003-feed-categories.2](functional-spec/FS-003-feed-categories.md#2-recent)). `forge::policy`
  settles each report it builds, but a merge folds two of them: a notice's
  state is the reason the forge sent it, never a terminal state, so nothing
  settled the thin report before it reached `absorb`, and a merged pull
  request the notice list also mentioned came back `finished` *and*
  `needs_response`. The model settles now — on the way in and after a merge —
  so the summary's Respond column, the unread counts and `status --check`
  stop counting work that is over.
- **A finished status line appeared under both Status and Recent**
  ([§FS-003-feed-categories.1](functional-spec/FS-003-feed-categories.md#1-the-categories)). Every
  interactive section but Recent's excluded finished work; Status did not, so
  a project answering `"status": "done"` was double counted in exactly the
  pile the categories exist to make readable. The plain renderer had it right.
- **A lost shared source let the whole run report success**
  ([§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
  `ephor refresh` counted per-project failures and printed the shared ones
  without counting them, so losing the leg that reads the forge's own notice
  list — the completeness capability — exited `0`. It is a refreshed unit like
  a project now, and its failure is exit `4`.
- **The dossier's message budget was a total that could be exceeded**
  ([§FS-005-dispatch.2](functional-spec/FS-005-dispatch.md#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)).
  Two messages were reserved per thread *before* the total was applied, so
  twenty threads quoted forty messages against a budget of twenty-four. The
  reservation is spent out of the total now, and a thread the budget cannot
  reach is counted as dropped rather than silently omitted from the tally.
- **The `rebase` recipe asked a model to run the rebase**
  ([§FS-005-dispatch.12](functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
  Only `ephor rebase --dispatch` made the deterministic move first; the inbox
  key, `work dispatch --recipe rebase` and `work sync` wrote a ticket whose
  brief said "run `ephor rebase` first" — a pass paid to have two commands
  typed, and a ticket even where the replay would have been clean. A recipe
  may now declare its deterministic opening move, `dispatch` makes it before
  anything is written, a clean replay opens no plan at all, and a conflict is
  handed over as the situation it stopped in.
- **Declared territory tied with references instead of settling it**
  ([§FS-008-attribution.3](functional-spec/FS-008-attribution.md#3-venue-beats-reference-beats-resemblance)).
  A subject on a repository the project claims is that project's "before any
  reference or alias is consulted", but territory scored as a reference — so a
  mention on the project's own ecosystem that happened to name another
  project's ticket key tied and went to the unattributed bucket. Territory is
  a venue now, as the forest is.
- **Resemblance could amend a subject some source had stated**
  ([§FS-008-attribution.3](functional-spec/FS-008-attribution.md#3-venue-beats-reference-beats-resemblance)).
  The strength a placement was reached by was computed and dropped, so nothing
  downstream could hold "resemblance may start a new row, it may not amend
  one". It rides on the placement now, and a matter placed by nothing firmer
  than the project's name is its own subject in the merge.
- **A ticket store that could not be read answered as an empty store**
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live),
  [§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
  An unreadable plan directory, and a plan the reader could not parse, both
  became "no tickets" — the one thing an empty section must never mean. A
  store is now reported like any other source that did not answer.
- **A gate verb could never run where it said it did**
  ([§FS-006-project-interface.3](functional-spec/FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out)).
  `seams::gate::run` built a rootless site, so a verb declaring
  `"cwd": "workspace"` silently ran at the forest root instead of in the
  branch workspace the change resolves to. It takes the caller's site now.
- **The `observable` rung counted sources that were merely configured**
  ([§FS-006-project-interface.10](functional-spec/FS-006-project-interface.md#10-capability-rung-by-rung)).
  The ladder calls it "at least one source *answering*"; a project whose every
  source was broken still held the rung, which is the empty feed claiming to
  mean "nothing is waiting". Configured-and-silent is its own reason now.
- **The shipped CI workflows could not run for anyone but ephor**
  ([§FS-009-shipped-actions.1](functional-spec/FS-009-shipped-actions.md#1-the-set)). Inside a
  reusable workflow a relative `uses: ./.github/actions/…` resolves against
  the *caller's* checkout, so `ephor-check.yml` and `ephor-validate.yml`
  failed for every repository that wired them in. They fetch their own steps
  at the version the caller pinned. The composite-action form was unaffected.
- Five sites cited `§FS-005-dispatch.10` for work that stops for a person,
  which is `§FS-005-dispatch.9`. They resolved, so nothing failed — they
  pointed a reader at the wrong law.
- The manual's CI examples pinned `v0.4.1`, a version that has never existed;
  they name this tree's version, and say why `version` and the `@ref` have to
  agree ([§FS-002-release](functional-spec/FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change)).
- A provider block's `timeout_seconds` was read from the configuration and
  then ignored: every provider ran under the shared
  `defaults.provider_timeout_seconds`. A forge behind a VPN, configured with
  the longer ceiling it needs, timed out on every refresh and its whole
  section of the feed stayed empty
  ([§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
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
  ([§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
- The thread screen advertised `+ react` on every message, including those no
  forge would take a reaction for; pressing it answered with one line at the
  bottom of a full screen of conversation, which is the one place a reader is
  not looking. Message keys are now offered per selected message
  ([§FS-004-quick-actions.2](functional-spec/FS-004-quick-actions.md#2-offered-only-where-it-would-work)).
- Posting a reaction only ever reached GitHub. `Forge::react` and the
  `ephor-forge-<name> react` subcommand were implemented and documented but had
  no caller, so an out-of-process forge that answered them could not be reached:
  a descriptor ephor did not recognize was dropped rather than handed back to
  the implementation that wrote it. Reactions now route through the source that
  reported the message.

### Changed

- **A finished job says so under the branch it ran on, not at the top of the
  screen**
  ([§FS-005-dispatch.17](functional-spec/FS-005-dispatch.md#17-a-move-that-needs-nobody-runs-beneath-the-screen)).
  A replay started beneath the screen used to announce itself in the
  header — `⤴ rebase onto main (level as of Aug 23): ok` above `ephor stream`
  — which names no project and no branch, so a reader with three of them going
  had to guess which row had just moved, and the line was gone at the next
  keypress. The line now lands **under the subject the job ran on**: the branch
  row it replayed, or the matter it was started about, beside the distance it
  just changed. Where that row is not on the screen at all — a branch with no
  item filed under it, the projects summary — the project's own row carries it,
  because news with nowhere to land is news that is lost. It stays there until
  the row is opened (`enter`, `o`, `x`, `w`, `c`) or a later job about the same
  subject replaces it. Only what has **ended** lands there: a job still going
  is already marked running where it could be started again
  ([§FS-005-dispatch.21](functional-spec/FS-005-dispatch.md#21-what-is-already-going-is-shown-where-it-could-be-started-again))
  and holds a row among the operations
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)).
- **The rebase is offered on every branch that is here, and every distance says
  how fresh it is**
  ([§FS-004-quick-actions.6](functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase),
  [§FS-004-quick-actions.8](functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)).
  A workspace whose repositories were last fetched nineteen days ago read
  `✓ checked out · up to date` and offered no rebase, while master had moved
  the whole time: the count is measured against the *last-fetched*
  `origin/<main>` and nothing in the watch fetches, so the offer was gated on a
  stale reading of the one move that would have refreshed it. Both entries —
  onto the main branch and onto the branch's own published copy — are now
  offered wherever there is something to replay onto, level or behind; a level
  branch replays onto nothing and is told so, in the register a current
  repository is always told it in. Rows and entries read `13 behind as of
  Jul 28` and `level as of Jul 28`, the day taken from the ref's own reflog —
  the last time that copy actually moved here, which a fetch that found nothing
  and a fetch that failed to connect both leave alone — per repository, oldest
  wins across a forest, and omitted entirely where no day was ever recorded.
  *Up to date* is retired: it claimed the branch was current when all that was
  measured was a match against a copy of unstated age. The `behind` recipe
  selector is unchanged and still means *behind*: dispatching a level rebase to
  an agent is a ticket to do nothing, while pressing the key runs git first
  ([§FS-005-dispatch.12](functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
- **The project's own work is called a task, everywhere**
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live),
  [§FS-003-feed-categories.1](functional-spec/FS-003-feed-categories.md#1-the-categories)).
  A **ticket** is what a remote tracker keys, an **issue** is what a forge
  files, and what a project keeps in its own checkout is neither — so it is a
  **task**, and one name for one thing ([§FS-001-forge-interface.3](functional-spec/FS-001-forge-interface.md#3-policy-lives-above-the-interface-never-in-an-implementation)). Tasks get
  a row of their own: a `task` kind and a **Tasks** category between
  Participating and Messages, in the navigator, the plain renderer and the
  manual's table, instead of sitting in **My Issues** among things other
  people filed. The rung is **tasks**; `requires: ["ticketed"]` and
  `requires: ["local-issues"]` still resolve, so nothing anybody already
  wrote stops meaning what it meant. The manifest key is `tasks`, with
  `tickets` still read as the older spelling — evolution by addition, so the
  schema gains the new key rather than breaking the old
  ([§FS-006-project-interface.11](functional-spec/FS-006-project-interface.md#11-the-interface-is-versioned)).
  The feed cache model is bumped, so a cache holding these as issues is
  rebuilt rather than shown stale. What is not one of these keeps its name:
  the ticket ephor writes to dispatch work and the ticket keys a forge is
  asked for are other things and are still called tickets.
- **The operations board enumerates the work roots, not the ledger**
  ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place)).
  Rows used to derive from ledger entries — operations about items ephor
  itself dispatched, about a third of what the section claims — so a plan
  written by hand, a project's own planning tickets, and a run started in
  another terminal on a root ephor never wrote into were invisible. The
  board now enumerates `*.rhei.md` under every work root: the configured
  `work.root`, resolved at each project's checkout and again in every
  branch workspace on disk, with aliased spellings of one directory
  collapsing to one row. Such an operation has no matter behind it by
  construction, so `Enter` opens its plan, titled in the plan's own
  heading. The walk runs when rows are built — the board opened, a refresh
  landing, the glance seeing something move — never on the bare 2-second
  tick, which keeps statting only what the last walk found
  ([§FS-005-dispatch.15.1](functional-spec/FS-005-dispatch.md#151-the-board-keeps-itself-current));
  measured against a real tree of fourteen projects and two dozen branch
  workspaces, the walk costs under a millisecond warm and the tick's gate
  stays in the tens of microseconds.

- **The inbox's run key binds the hand too**
  ([§FS-005-dispatch.14](functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).
  `R` on the work screen built its run without ever asking who should get it,
  so a hand naming an agent and no model bound from `ephor work run` and not
  from the key a reader actually presses — the wrong way round, since the
  inbox is the surface the choice exists for. The key now resolves the hand
  the way the command line does, over the one plan it runs, and carries it as
  the runtime's own agent flags; the header above the run names who is getting
  it. There is no longer a second way to build a run: the unflagged builder
  the key used is gone, so a surface that starts one has to answer the
  question. What the resolution had to say — a hand nothing resolves, an
  effort completed to the agent's only one, a plan whose tickets disagree —
  now waits on the message line for the reader coming back, rather than being
  printed into a terminal the run is about to take.

- **A workspace missing one of the project's repositories is completed, not
  called done**
  ([§AR-004-forest.1](architecture/AR-004-forest.md#1-folds),
  [§FS-004-quick-actions.7](functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)).
  A declared repository with no working tree on disk became visible last round
  — every fold names it rather than quietly answering for fewer repositories
  than you have — and what was left undecided was whether it should also
  *fail*. It does not, in any fold over what is there: an exit code routes an
  outcome to whoever acts next, and this one routes nowhere, since retrying
  replays no tree that is not there, the missing one holds none of your change,
  and the condition was as true before the command ran as after. `ephor
  checkout` is where it does fail, because there it is exactly the outcome the
  command was asked to change — so that is the exit code that answers *is this
  workspace whole*. For it to answer that at all, `ephor checkout` on a
  workspace that already exists now folds over the project's layout instead of
  stopping at the directory: repositories already there are reported as already
  there and left untouched, missing ones are made, and one that could not be
  made exits `1`. A whole workspace still says *already checked out* and
  changes nothing.

- **A refresh landing places the project that landed, not every project**
  ([§FS-001-forge-interface.7](functional-spec/FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it),
  [§FS-008-attribution.2](functional-spec/FS-008-attribution.md#2-two-stages-one-engine)).
  Items arriving mid-refresh are filed under their branches as they land, and
  that was done by re-running the whole site's placement pass on every arrival
  — so a refresh over N projects paid the whole matching pass N times, and the
  cheapest project's landing paid for the most expensive project's items. The
  pass is scoped now, one implementation parameterised by what it answers for
  rather than two that could file a row differently mid-scan and at the end: a
  landing re-places its own project and leaves the rest of the site standing,
  while the reload at the end of the run still places everything. Measured on a
  seven-project cache — 228 items, 27 branch workspaces on the largest project
  — a refresh drops from 2,009 placements to 228, about 15 ms of matching to
  about 2 ms.

- **A repository parked on the base counts toward the rebase onto main alone**
  ([§FS-004-quick-actions.8](functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)).
  The copy-is-the-base duplication guard was all-or-nothing across the forest
  while the copy-side count summed across it, so a workspace with one
  repository on the change's branch and two parked on `master` tracking it —
  the ordinary graal shape — showed both menu entries carrying the identical
  number and doing the identical thing to the identical repositories. A
  repository whose published copy is its base is now left out of the
  copy-side sum entirely: its one distance belongs to the base count, the
  copy entry counts and names only the repositories that trail a copy of
  their own, and a checkout of nothing but parked repositories measures
  nothing there and is offered only the rebase onto main. The standing fold
  behind all of this also stopped re-deriving its own facts — the base is
  resolved once and carried on the per-repository answer, one `for-each-ref`
  reads branch, upstream and both distances at once, and presence on disk is
  a path test rather than a subprocess
  ([§AR-004-forest.1](architecture/AR-004-forest.md#1-folds)) — which cuts a
  refresh landing on the inbox from ~8 git subprocesses per repository to ~3.

- **Recipes and actions are one menu**
  ([§FS-005-dispatch.1](functional-spec/FS-005-dispatch.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for),
  [§FS-006-project-interface.9](functional-spec/FS-006-project-interface.md#9-offers-the-projects-actions)).
  What you could do about a row depended on which key you knew: `x` listed the
  commands and `w` listed the work, and neither mentioned the other. `x` now
  carries both — the recipes that apply to the item stand after the quick
  actions, the project's offers and your own, each with the recipe's own icon
  and a `→` naming the hand that would get it, resolved through the same six
  steps the dispatch resolves, so an unavailable hand says why and a refused
  one refuses the entry rather than being found out at the keystroke. Pressing
  it hands the work over through the one path the work screen uses: same plan,
  same ticket, same ledger entry. A configured action may now carry an
  `agent` block — `brief`, and optionally `state` and `hand` — instead of a
  `command`, which is a recipe under another name and lets a project write one
  agent entry rather than an entry and a recipe that have to agree; an entry
  with both, with neither, or asking for work under no `id` is refused when
  the file is read. Work is offered only where it would work: never about a
  finished item, and — where it edits the change — only where the change is on
  this machine. The shipped `rebase` recipe drops its `kinds`/`roles`
  selector, because the entry that dispatches it asks about a branch on disk
  that trails its base and the two cannot be gated differently; a recipe whose
  id the menu already carries is dropped from it, so a stale branch still
  shows one rebase row and the recipe is what that row hands its conflict to.
  The menu footer is built from the selected entry, and with no runner bound
  the work is still offered — the ticket is written either way — with the
  *workable* rung's own sentence where the hand would be.
- **Every operation is visible in one place, and parked work resurfaces on
  its own** ([§FS-005-dispatch.15](functional-spec/FS-005-dispatch.md#15-every-operation-is-visible-in-one-place),
  [§FS-005-dispatch.9](functional-spec/FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).
  Answering "what is ephor doing right now" meant visiting the work screen of
  every item that might have an answer. `;` now opens a watch-only operations
  board from any screen: rows are execution roots — the runtime locks one per
  root, so two items in one branch workspace are one operation — with
  liveness read from that lock by a non-blocking probe, never from a process
  table, the held ticket from the run's own journal, the rest queued, a
  `quiet` badge on a run that has written nothing for a while (silence is not
  death: the OS releases the lock when a run dies), and a ticket claimed with
  no run behind it shown as *claimed, not scheduled* beside the runner's own
  release command. Work parked for a person keeps its row after the run
  exits — the usual end of a parked ticket's run, since nothing else was
  schedulable and parking writes no claim — and a run that died mid-slot
  leaves its held ticket *waiting on you* rather than vanishing with the
  lock; within one operation what waits on the reader lists ahead of what
  runs, then claims, then the queue
  ([§FS-005-dispatch.9](functional-spec/FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).
  The journal outlives every run and is believed accordingly: an assignment
  no run released is held per invocation — fanout cannot mark a task free
  while a sibling still runs it — and stops counting the moment the ticket's
  own state moved on, so a crashed run's ticket never reads running under a
  later run. A root whose `states.yaml` cannot be read says less instead of
  guessing: running and claimed still show, nothing is called queued or
  counted finished on a machine that is not there. `Enter` goes to the
  matter, `o` opens a live run's
  dashboard, `Esc` returns exactly where the reader was; the background
  refresh reports on the board additionally to the header line it keeps
  ([§FS-001-forge-interface.7](functional-spec/FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)).
  Plan state and assignee read straight off the plan files as always — the
  floor that stays with no runner bound, when the board is the refresh row
  alone — and the runner's own `list --json` sharpens them where the binary
  is there: it also surfaces a subtask a live run holds, which the plan read
  alone cannot see, and it is asked only about roots that hold an operation
  — an idle root costs stats, never a fork. The tick's change gate is the
  journal and the lock, one stat each — never a sweep of the agent logs,
  which grow for the life of a project and are read only to clock the
  `quiet` badge on a live row. The side effect reaches past the board: work state used to be
  re-read only on refresh landings and dispatch, and now an mtime-gated tick
  between key reads re-reads what actually moved — so a ticket the runtime
  parks for you resurfaces the moment it parks
  ([§FS-005-dispatch.15.1](functional-spec/FS-005-dispatch.md#151-the-board-keeps-itself-current)),
  which is [§FS-005-dispatch.9](functional-spec/FS-005-dispatch.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking) finally holding without a refresh.
- **A project says who does which action, and which hands may be used on it at
  all** ([§FS-006-project-interface.9](functional-spec/FS-006-project-interface.md#9-offers-the-projects-actions),
  [§FS-005-dispatch.14](functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).
  Every ticket ephor wrote went to whoever the runtime would have picked
  unasked, so a trivial replay and the conflict that needed judgment were the
  same request — and the only way to change that was to point the runtime
  itself somewhere else, for everything. `work.hands` now maps an action's id
  to a hand from the roster (`{ "default": "sonnet", "rebase": "luna:high" }`),
  at site level and under `projects.<id>.work`, and the seven steps that answer
  *who does this* run narrow before broad: what you picked for this dispatch,
  the `hand` the recipe carries, the project's entry for this action, the
  project's default, the site's entry for this action, the site's default,
  then the runtime's own unasked pick. It is the runtime's resolution order mirrored, so the two
  cannot disagree about one configuration. The long form
  `{ "agent", "model", "effort" }` stays legal for a pair the runtime's
  registry never listed — a proxy serving a model it does not know — and is
  accepted with a note, since ephor cannot prove it invalid; a name the roster
  does have is checked, and a typo or an undeclared effort is refused before
  anything is written. `permitted_hands` narrows a project to the hands that
  may work on it, for a repository under a policy about which models may see
  its code: anything outside it is refused with that reason wherever it was
  named, never quietly replaced. With no table anywhere nothing changes, and
  with no runtime on `PATH` a configured hand resolves to nothing, says so in
  the workable rung's own words, and the ticket is written all the same.
- **A refresh no longer takes the screen**
  ([§FS-001-forge-interface.7](functional-spec/FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)).
  `r` ran the whole fetch on the thread that draws and reads keys, so the
  interface froze until the last provider answered — nothing repainted, no key
  was read, and `^C` is a key event in raw mode, so there was no way out
  either. Projects are asked one after another and a provider may set its own
  ceiling, so a site with one forge allowed ten minutes could hold the screen
  for the length of a coffee break. The run now lives on a thread of its own:
  the screen stays yours for all of it, each project takes its place in the
  feed as its sources answer rather than the whole run landing at the pace of
  the slowest one, and the header carries `Refreshing <project> (3/7)…` so a
  live screen is not read as a finished one. Pressing `r` during a run says so
  instead of starting a second. What a refresh costs the forge is unchanged —
  the projects are still asked one at a time, and it is the waiting that moved,
  not the load.
- **The cursor follows the row it was on across a rebuild**, rather than the
  line number that row happened to occupy
  ([§FS-001-forge-interface.7](functional-spec/FS-001-forge-interface.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)).
  Selection was kept by index, which was harmless while the tree only changed
  when you asked it to; with answers arriving underneath a reader who is still
  moving, an index is the wrong thing to keep — rows sort in above the cursor
  and `x` opens the menu for a matter you were not looking at. A row that is
  gone still leaves the index standing, so marking a pile done walks down it as
  before.
- **CI no longer runs a Windows leg, and [§RM-004-windows](roadmap.md#rm-004-windows-ephor-runs-where-there-is-no-posix-shell)
  says what a port would take.** `windows-latest` was in the matrix from the
  first commit and was never once green: every test binary failed to compile,
  so nothing behind that had ever run. Compiling it took one line, and what
  came out from behind it is a port — an out-of-process forge extension is a
  shell script, and Windows cannot exec a file whose executability is a `#!`
  line, with `work`, `feed`, `forge_extension`, `checkout`, `update` and six
  e2e cases still unexamined behind that. Four real defects the compile break
  had been hiding are fixed and listed above. The leg is off until the rest is
  decided, because a leg that is always red is one nobody reads, and it costs
  the two that mean something.
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
  ([§FS-001-forge-interface.3](functional-spec/FS-001-forge-interface.md#3-policy-lives-above-the-interface-never-in-an-implementation)).
  The provider reports roles, reasons, conversation, and gate; `role`, the
  displayed state, and `needs_response` are composed above it.
- **A gate now carries the forge's verdict, not only its counts.** A pull
  request whose every job passed can still be refused — on an approval, on a
  downstream repository, on jobs the gate never started — and a row showing
  `✓118` read as finished work. The row now says `⊘ blocked` beside the counts
  and the reasons are one keystroke away
  ([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)).
- **A refresh that lost any provider now exits non-zero** (`4`; `3` still means
  every provider failed) and reports each failure as `error:` naming the
  project and provider. A partial refresh used to exit 0, so a source could
  stay dark indefinitely behind a timer that saw nothing wrong
  ([§FS-001-forge-interface.6](functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
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
- **The rebase is offered wherever there is a branch**
  ([§FS-004-quick-actions.6](functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
  Both replays were offered only on pull requests, and only on projects whose
  registry row named a `main_branch` — so an issue about the same change, a
  status a source filed about it, and every branch row in the detail view got
  nothing, and a project that names no main branch got nothing at all. What the
  offer is about is a branch on disk that trails something, never the kind of
  row that mentions it: any row resolving to a branch workspace now carries
  both entries, and so does the branch row itself, which is where the `13
  behind · ↓2` a reader is reacting to is actually written — `x` on it opens
  the same menu, built by the same code, carrying ephor's own offers only,
  since a source's, a project's and a person's entries are selected against an
  item a branch row does not have. The two replays are also gated apart: the
  one onto main has to name the branch it replays onto and is offered only
  where the project declares one, while the one onto the branch's published
  copy resolves its ref inside each repository and needs no such name, so a
  project with no `main_branch` is offered it — and its rows show `↓2` alone
  rather than nothing at all. Where a branch cannot be resolved to a checkout
  the offer is withheld rather than made and left to fail on the keystroke
  ([§FS-004-quick-actions.2](functional-spec/FS-004-quick-actions.md#2-offered-only-where-it-would-work)).
- **Tests split into an integration home, and e2e moves under `tests/`.**
  (PR #18) `.agents/grund.toml`'s deprecated `[[kinds]] prefix` key is renamed
  `kind` throughout — required before grund 0.13.0, which stops loading it —
  and the citable `E2E` kind gives way to two non-citable homes: `e2e` at
  `tests/e2e` (the corpus, moved from `e2e/`) and `integration` at
  `tests/integration` (the ten cross-part Rust tests and three Python
  repo-hygiene tests, moved out of `tests/`). `[citations.E2E]` becomes
  `[citations.e2e]` (must cite FS, should not cite AR) and
  `[citations.integration]` (should cite AR). CI's grund pin moves 0.9.0 →
  0.12.3 so the new config loads, and the entrypoints are re-rendered by that
  version. Cargo's `[[test]]` blocks and `exclude`, CI's and the pre-commit
  hook's Python discovery path, and the justfile's `e2e` recipe all follow the
  two moves.

### Fixed

- **Optional workflow execution targets can resolve to nobody**
  ([§FS-005-dispatch.19](functional-spec/FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)).
  An empty target answer now passes to the runtime as written without being
  parsed, narrowed, or rendered as a hand, so optional tiers can retain the
  execution-target format and its policy for every non-empty choice. Empty
  positions in a list of targets have the same reading. (PR #50)

- **Capabilities and missing-hand refusals explain how to configure
  model-carrying hands**
  ([§FS-005-dispatch.14](functional-spec/FS-005-dispatch.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)).
  A roster containing only agent-default hands now points to the Rhei settings
  `models` registry, and choosing an unknown named hand keeps refusing it while
  explaining how a matching model profile with an agent carrier creates that
  name. Selection, narrowing, JSON shape, and no-write refusal behavior stay
  unchanged. (PR #48)

- **`ephor work lay --dry-run` leaves the ordinary work root untouched**
  ([§FS-005-dispatch.19](functional-spec/FS-005-dispatch.md#19-a-workflow-the-runtime-offers-is-an-action-and-its-inputs-are-answered-here)).
  A dry run now asks the runtime to validate and report the resolved workflow
  without creating the work root or the dossier, item, and values files a real
  laying would carry. (PR #29)

- **`ephor work offers` explains why a selector refused a recipe**
  ([§FS-005-dispatch.27](functional-spec/FS-005-dispatch.md#27-an-offer-that-a-selector-refused-says-why),
  [§FS-003-feed-categories.1](functional-spec/FS-003-feed-categories.md#1-the-categories),
  PR #22). A project's own tasks carry no role, so a `roles` selector — non-empty
  by definition once written — excluded every one of them the same as any other
  selector refusal: an offer that never appeared, and a "nothing matches this
  matter" that could not say why. `ephor work offers` now names every recipe
  considered for a matter whose selector refused it, and which field refused —
  the role-less case worded plainly: the matter carries no role, the selector
  asks for one. Both readings carry it in the same words: the JSON gains an
  additive `excluded` array beside `offers`, and the prose lists it under the
  same "nothing matches this matter" a reader already looks under. Selector
  semantics do not change — a role-less item still matches only an empty
  `roles`, and the dispatch dry run is unaffected. `task` is also added to the
  `--kind` help across the three commands that take it, the manual's selector
  table, and the `Selector` kinds rustdoc, since the feed already accepted and
  returned it undocumented.

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
- `grund` tree: [§FS-001-forge-interface](functional-spec/FS-001-forge-interface.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
  and [§FS-002-release](functional-spec/FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change),
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
