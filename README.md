# ephor

A Rust CLI that watches over every project you work on. It manages the
project/workspace registry (extracted from the `automation` repo's Python
`dev/projects` tool) and aggregates a per-project information stream — PRs in
review, gate state, messages awaiting a response, Jira tickets, and custom
project status.

**[docs/manual.md](docs/manual.md) is the manual**; this page is the tour.

*ἔφορος* — overseer, from *epi* (over) + the root of *horaō* (to see). In
Sparta a board of five ephors watched over the kings: they did none of the
governing themselves, but they observed, and they could summon and suspend.
That is the shape of this tool. Each **provider** is one ephor on watch —
`github-prs`, `github-ci`, `github-threads`, `custom-status`, and any forge
extension you install — and the TUI is the board they report to, with an
action menu for when watching is not enough.

## Layout

- `src/` — the `ephor` binary (registry engine + status feed)
- `config/*.example.json` — worked examples of the two configuration files;
  your own live in `~/.config/ephor/` and are never committed
  ([§FS-001-forge-interface.5](docs/functional-spec/FS-001-forge-interface.md#5-no-site-specific-data-in-the-repository))
- `config/templates/` — AGENTS.md templates referenced by the registry
- `assets/*.schema.json` — the published schemas, embedded in the binary and
  printable with `ephor schema`: the registry, a project's `ephor.json`, the
  answer envelope, and the forge interface
  ([§FS-006-project-interface.11](docs/functional-spec/FS-006-project-interface.md#11-the-interface-is-versioned))
- `tests/e2e/cases/` — one executable scenario per seam, each citing the `§FS`
  point it holds ephor to
- `tests/integration/` — cross-part Rust tests and Python repo-hygiene tests
- `ai/skills/` — canonical agent skills (`track-project`); `ai/link-global-skills.sh` links them into `~/.claude/skills` etc.
- `systemd/` — user units for periodic feed refresh and work sync
- `docs/manual.md` — **the manual**: every command, key, and configuration
  field, end to end; `docs/registry.md` explains what the registry's concepts
  mean, and `docs/functional-spec/`, `docs/roadmap.md` and
  `docs/changelog.md` are the grund tree

## Install

```bash
just install            # cargo install --path . → ~/.cargo/bin/ephor
just link-skills        # link ai/skills/* into the global agent skill dirs

mkdir -p ~/.config/ephor
cp config/workspaces.example.json ~/.config/ephor/workspaces.json
cp config/status.example.json     ~/.config/ephor/status.json
```

Scripts that wrap ephor — your own checkout and update commands — reach it
through `PROJECTS_BIN`, and `EPHOR_HOME` says where its configuration lives;
both are ordinary environment variables, set wherever you set the rest.

`cargo install ephor` is not available yet: nothing has been published, and the
first release has to be tagged by hand before the bump workflows have anything
to count from. The site-specific blocker is cleared —
`scripts/check-no-site-specific.sh` passes on this tree, so no employer or
vendor identifier and no real registry reaches an artifact — and what is left
before a first release is in
[§RM-001-forge-interface](docs/roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface).

## Requirements and releases

The repository is a [grund](https://github.com/vjovanov/grund) tree:
[docs/functional-spec/](docs/functional-spec/) declares what ephor must do,
[docs/roadmap.md](docs/roadmap.md) sequences what is not built yet, and
`grund check` verifies that every `§ID` citation resolves.

```bash
just check         # the CI gate: fmt, build with -D warnings, tests, grund
just pre-release   # everything a release verifies, publishing nothing
```

Releases follow [§FS-002-release](docs/functional-spec/FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change):
a version exists exactly when a `vX.Y.Z` tag does, every pull request adds a
bullet under `## Unreleased` in [docs/changelog.md](docs/changelog.md), and
publication is a workflow rather than a hand-run command — `Auto bump` cuts a
patch release on Mondays when main has observable changes and green CI,
`Release minor` does the same on demand for a minor, and both dry-run the whole
release on a candidate branch before fast-forwarding main. `Release` then
builds a profile-guided binary per target, publishes the crate, and attaches
the archives to a GitHub release whose notes are that version's changelog
section. The first release has to be tagged by hand — the bump workflows need a
tag to count from.

## Registry commands (drop-in for the old `dev/projects`)

```bash
ephor list                                  # table of registered projects
ephor validate                              # schema + semantic + path validation
ephor ensure-agents [--workspace ID | --type T --root R --var k=v ...]
ephor [--workspace ID ...] update [--debug] [--skip-agents]
```

Resolution: registry `--registry` > `$EPHOR_REGISTRY` >
`~/.config/ephor/workspaces.json` > `$EPHOR_HOME/config/workspaces.json`
(legacy, still honored); schema `--schema` > `$EPHOR_SCHEMA` > embedded. The
feed config resolves the same way: `$EPHOR_STATUS_CONFIG` >
`~/.config/ephor/status.json` > `$EPHOR_HOME/config/status.json`. Template
paths in the registry resolve relative to the registry file.

[docs/registry.md](docs/registry.md) explains what the registry's concepts
mean — organizations, project types and their repos, release versus working
branches, ticket inference, branch workspaces, and hook sets. The embedded
`assets/workspaces.schema.json` is the authority; `ephor validate` enforces it.

## Status feed

```bash
ephor tui                                   # interactive inbox (alias: ephor inbox)
ephor refresh [PROJECT ...] [--quiet]       # fetch providers into the cache
ephor status                                # summary table per project
ephor status widget [--refresh|--cached] [--max-age SECS] [--json] [--check]
ephor feed [--project P] [--unread] [--kind pr|ci|message|status] [--json]
ephor mark-read widget | mark-read --all | mark-read --id ITEM_ID
```

- Cache lives in `~/.local/state/ephor/` (`feed/<project>.json`, `seen.json`).
  An item is unread until `mark-read`, and resurfaces when it changes again.
- A failing provider keeps its last-good items marked `(stale)`; one flaky
  source never blanks the feed. Provider errors are warnings, not failures.
- `ephor status --check` exits 4 when unread needs-response items exist
  (usable from shell prompts).
- Exit codes: 0 ok, 2 config/registry error, 3 all providers failed, 4 check.

### Inbox TUI

Everything in `ephor tui` is organized per organization (with its checkout
folder from the registry's `organizations[].root`, e.g. Widget — `~/c/g`,
Foundation — `~/f`), then per project, then per type (Status / My Pull
Requests / Reviewing / CI / Messages), then per branch — items nest under the registry
branch they belong to (matched via ticket key or branch name), with a
"(not linked to a branch)" group for the rest. Two views, toggled with
`Tab`:

- **Stream** — the full tree across all organizations, unread-only by
  default; `Enter` on a project row drills into its detail view.
- **Projects** — one summary row per project (active branches, items,
  unread, needs-response); `Enter` opens the project detail, which also
  lists every registry branch (active marker, ticket, release flag,
  linked-item count). On a branch row, `Enter` opens the most urgent feed
  item linked to that branch.

Every PR row carries its **gate status** — `✓72 ✗1 ⋯3` (passed / failed /
still running), counted over every repository the gate covers. An internal
change gates across its whole PR tree (app + plugins + docs-site), so
those rows also show the per-repo breakdown: `✓72 ✗1 ⋯3  (app ✓42 ✗1 ·
plugins ✓30 ⋯3)`. Counts come from `gh pr checks` on GitHub and
from a forge extension's `gate` field, are recorded during refresh
(`raw.gate`), and are omitted for PRs with no jobs. Each gate costs one
extra call per PR at refresh time; `"gates": false` on a `github-prs`
provider block turns it off.

Every branch row states its checkout: `✓ checked out` (green) when the
branch workspace exists on disk, `∅ not checked out` otherwise. Checked-out
branches also show staleness against the project's `main_branch` — `· N
behind as of Jul 28` (or `· level as of Jul 28`), where N sums `git rev-list
--count HEAD..origin/<main>` over **all** the workspace's repos (Widget CE + EE +
docs-site for a poly-repo workspace). Counts are measured locally at startup and on
`r`, against the last-fetched origin — and the date is when that fetch last
moved `origin/<main>` here, oldest across the repos, omitted where nothing
ever recorded one.

| key | action |
|---|---|
| `j`/`k`, arrows | move (skips headers) |
| `Tab` | switch stream ↔ projects |
| `Enter`/`l` | open the item's thread screen (falls back to the browser when nothing is recorded); on a project header: drill in |
| `o` | open in browser |
| `v` | thread screen, strictly (status message when empty) |
| `x` | summon the configured actions for the item (see below) |
| `C` | check out the branch the row is about, where it says `∅ not checked out` |
| `m`/`d`/Space | mark done (resurfaces if the item changes again) |
| `a` | mark everything visible done |
| `[` / `]` | previous / next project (detail view) |
| `Esc`/`h` | back to the project list (detail view) |
| `u` | toggle unread-only / everything |
| `r` | refresh (detail view refreshes just that project) |
| `q` | quit |

**Thread screen** (`Enter` on any item): the recorded threads in full —
each message is a card with a per-author colored gutter, its age, wrapped
text, and its reactions (`👍 2 (alice, bob)`); multiple threads are
labelled and the last message is flagged while the item needs a response.
Providers store the threads in the item's `raw.threads` during refresh, so
reading is instant and works offline.

Keys: `j`/`k` select the previous/next message (the view follows), `f`/`b`
page, `g`/`G` first/last, `+` react to the selected message, `t` tick the
selected task, `Enter`/`o` opens the item in the browser, `m` marks it
done, `Esc`/`q` goes back. `+` and `t` are offered on what is selected, so
the footer only shows a key where it would do something.

**Reactions**: `+` opens a picker with GitHub's palette (👍 👎 😄 🎉 😕 ❤️
🚀 👀) — `←`/`→` or `1`-`8` choose, `Enter` posts via the provider
(GitHub comments post through `gh api graphql`; a forge that does not
declare the `reactions` capability is display-only). Reacting is often
enough to answer a mention — the item stops needing a response on the next
refresh.

**Tasks**: where a forge tracks tasks — a checklist item, a blocker
comment, a review task — the message carrying one renders with its box, ☐
or ☑, and `t` ticks it in place. A box also answers its thread: an open one
keeps the conversation awaiting you however it ended, and a ticked one
settles it even where every message belongs to a robot, which is what stops
a bot checklist from sitting in the inbox forever.

### Item actions

`x` on any item (in the tree or the thread screen) summons its action menu
— `j`/`k` + `Enter` or `1`-`9` runs one, `Esc` cancels.

**Quick actions** need no configuration and lead the menu
([§FS-004-quick-actions](docs/functional-spec/FS-004-quick-actions.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)):
the source that produced an item offers what it knows to do about it.
Today `github-ci` offers `✗ see the CI failures` on a pull request whose
gate is red — the check list as GitHub reports it, then `gh run view
--log-failed` for every failed run, paged. One is offered only where it
would work: the gate is failing, the item still names its pull request,
and `gh` is installed.

ephor offers three of its own, from what is on disk rather than from a source.
`⤴ rebase onto <main> (N behind as of Jul 28)` on a pull request whose branch
workspace is on disk and whose project names a main branch — behind it or
level, since the replay fetches first and is the move that refreshes the
reading
([§FS-004-quick-actions.6](docs/functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
It runs `ephor rebase` there — fetch and replay every repository in the
checkout, nothing stashed, nothing pushed — and where the replay stops in a
conflict it opens the ticket about it, because that part is a question about
the code and the rest never was
([§FS-005-dispatch.12](docs/functional-spec/FS-005-dispatch.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).
`⤴ rebase onto <remote>/<branch> (N behind as of Aug 11)` where the same
checkout has a **published copy** of its own to replay onto instead — somebody
else pushed to your branch
([§FS-004-quick-actions.8](docs/functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)):
`ephor rebase --upstream` replays each repository onto its own copy, which is
the branch bare `git rebase` refuses to touch when no tracking configuration
names one. And `⇣ check out <dir>` on an item whose branch workspace is not there yet
([§FS-004-quick-actions.7](docs/functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)):
`ephor checkout` needs nothing configured — the registry says where the
workspace goes, which repositories it holds, and what a new branch grows from —
and it is also the step that runs before any other action on a missing
workspace.

**What the project offers** comes next, where it wrote an `ephor.json` of its
own (below), and **configured actions** follow, from `status.json`: top-level
`actions` apply to every project, `projects.<id>.actions` are appended for that
project, and an optional `kinds` list restricts an action to `pr` / `ci` /
`message` / `status` items. Where two entries share an `id`, yours beats the
project's beats the shipped one, in the place the earlier one held
([§FS-006-project-interface.9](docs/functional-spec/FS-006-project-interface.md#9-offers-the-projects-actions)).

```json
{
  "actions": [
    { "icon": "⎇", "description": "check out PR branch",
      "command": "gh pr checkout -R \"$EPHOR_REPO\" \"$EPHOR_NUMBER\"", "kinds": ["pr"] },
    { "icon": "🧪", "description": "run the gate", "command": "just gate" }
  ],
  "projects": {
    "widget": {
      "checkout": { "description": "create the branch workspace",
                    "command": "git worktree add \"$EPHOR_WORKSPACE\" \"$EPHOR_BRANCH\"" },
      "actions": [
        { "icon": "💡", "description": "open the app in the editor",
          "command": "nohup $EDITOR \"$EPHOR_WORKSPACE/app\" >/dev/null 2>&1 &",
          "requires_checkout": true }
      ]
    }
  }
}
```

**Checkout dependency**: a project may define one `checkout` command — its
contract is to make `$EPHOR_WORKSPACE` exist (ephor verifies the directory
afterwards; it runs in the project root). Actions marked
`requires_checkout: true` are gated on the branch workspace: when it is
missing, the menu annotates them "(will check out first)" and running them
chains checkout → action; the checkout also appears as its own menu entry.
Without a configured checkout command, or when the item is not linked to
any registry branch, such actions are shown "(unavailable)" and refuse
with the reason.

**A branch for work about an item that has none**: an entry that *hands work
over* — one carrying `agent` or `workflow`, a project's own offer naming a
workflow, the entry beside a workflow, or a recipe — may add
`"branch": "fix/issue-{number}"`. It is a template rendered from the item
exactly as a brief is, and refused by name before anything is made — for a
field it decides (`{branch}`, `{workspace}`, `{reply}`), a name that is no
field of an item at all, a field this item has not got, or a rendering git
will not take as a branch. It applies only where the item has no branch of its own
— a pull request always keeps the branch the forge recorded. Saying it means
the work needs the checkout, so ephor makes that branch's workspace with the
same operation `ephor checkout` is — the repositories grown from the project's
main branch, plus the task store — before writing anything, and the work root
resolves inside it. A project with
no `branch_root_template` is refused by name, and so is checkout-needing work
about an item with no branch and no template
([§FS-005-dispatch.25](docs/functional-spec/FS-005-dispatch.md#25-work-about-a-matter-with-no-branch-can-mint-the-branch-it-needs)).

The command runs via `sh -c` **in the item's checkout**, resolved through
the org → project → branch hierarchy: the item is matched to its registry
branch (the same matching the tree uses for grouping), and when the
project defines `branch_root_template` and that branch workspace exists on
disk (e.g. `~/c/g/vj/ABC-42-…`), the command runs there; otherwise it
runs in the project `root`. If the root itself is missing, the menu
refuses with a status message instead. The TUI leaves the screen while the
command runs — so interactive tools (lazygit, an editor) work — and
returns after Enter. The item's context is exported to the script:

| variable | value |
|---|---|
| `EPHOR_PROJECT`, `EPHOR_ROOT` | project id and its registry root |
| `EPHOR_WORKSPACE` | the resolved checkout the command runs in (also the cwd) |
| `EPHOR_BRANCH`, `EPHOR_TICKET` | provider-recorded branch, or the matched registry branch and its ticket |
| `EPHOR_ITEM_ID`, `EPHOR_SOURCE`, `EPHOR_KIND` | item identity (`pr`/`ci`/`msg`/`status`) |
| `EPHOR_TITLE`, `EPHOR_URL`, `EPHOR_STATE` | display fields (empty when absent) |
| `EPHOR_REPO`, `EPHOR_NUMBER` | best-effort `owner/name` and PR number from the item |
| `EPHOR_RAW` | the item's full raw JSON, for `jq` |

## What a project can say about itself

Tracking a project costs minutes and touches nothing in it: ephor requires
*capabilities* of a project, never artifacts in it
([§FS-006-project-interface](docs/functional-spec/FS-006-project-interface.md#fs-006-project-interface-a-project-and-ephor-meet-over-one-interface-in-three-homes)).
A project that wants to say more places one optional file at its forest root:

```jsonc
// ephor.json — every field optional, an empty {} is valid
{ "identity": { "aliases": ["widget"], "territory": ["acme-labs"] },
  "checks":   { "check": "./check.sh", "smoke": { "command": "./ci/smoke.sh", "features": "list" } },
  "ci":       { "status": "./ci/gate.sh", "failures": "./ci/gate-failures.sh" },
  "tasks":    [{ "kind": "rhei", "path": "docs/plans" }],
  "actions":  [{ "id": "rebuild", "description": "rebuild the docs site",
                 "command": "./tools/rebuild.sh", "requires": ["checkout-able"] }] }
```

- **Checks are verbs** — `check`, `style`, `smoke`, probed as `./check.sh`,
  `./check-style.sh`, `./smoke-test.sh` or declared here, each self-contained,
  smoke optionally enumerating features. `ephor check` runs them from the
  checkout alone.
- **The gate is the project's, in three verbs** — `status`, `failures`,
  `restart`. A forge-hosted gate needs none of this: the provider's own gate
  capability is the shipped default.
- **The project's own tasks are read where they live** — `panta/`, `.beads/`,
  or wherever the manifest says.
- **Offers** are menu entries the project ships with itself, gated by the same
  capability rungs your own actions use.
- **What crosses the interface is validated**: `ephor validate --manifest .`,
  and `ephor schema manifest|answer|registry|forge` prints the published
  schemas, which are what a release may not break silently.

Your registry row is authoritative over all of it — identity fields are hints
it adopts or overrides, and `manifest_trust` narrows a checkout you trust less
to `descriptions` or `ignore`. What a project can do is resolved into a ladder
of rungs, and a feature that needs a rung you do not hold says so where it
would have appeared rather than vanishing
([§FS-006-project-interface.10](docs/functional-spec/FS-006-project-interface.md#10-capability-rung-by-rung)).

Three CI steps ship for the repository half of this — `setup`, `validate`,
`check` — and each runs from repository-committed material and workflow inputs
alone, never from anybody's site
([§FS-009-shipped-actions](docs/functional-spec/FS-009-shipped-actions.md#fs-009-shipped-actions-what-ephor-ships-for-ci-runs-from-the-repository-alone),
[§9.3](docs/manual.md#93-ci-steps-ephor-ships)).

## Work: handing an item to an agent runtime

A watch that only watches hands you a list, and nearly every row on it has the
same next move — read the failures and fix them, answer the question, read the
change. ephor can hand that over
([§FS-005-dispatch](docs/functional-spec/FS-005-dispatch.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)):
an item plus a **recipe** becomes a ticket in a
[rhei](https://github.com/vjovanov/rhei) plan, written into the checkout the
item's branch resolves to. ephor writes files and nothing else — no comment, no
push, no pull request — and then keeps the ledger.

```bash
ephor work                                  # what has been dispatched, and what it reached
ephor work dispatch --dry-run               # what would be opened, and where
ephor work dispatch [--project P] [--recipe R] [--item ID] [--kind pr]
                    [--updated-within DAYS] [--again] [--ranking PATH] [--limit N]
ephor work ask --item ID "…"                # a ticket in your own words
ephor work sync [--dry-run]                 # reopen work whose item has moved
ephor work cancel --item ID TICKET… [--why "…"]   # take a ticket back; the plan keeps it
ephor work run [--project P] [--item ID] [-- --parallel 2]   # rhei run, one per checkout
ephor work forget [--item ID | --done | --missing]
ephor work states                           # the state machine the tickets run under
```

The loop, whole:

```bash
ephor refresh                                     # what happened
ephor work dispatch --dry-run --updated-within 14 # what would be handed over
ephor work dispatch --updated-within 14           # hand it over
ephor work run                                    # let the runtime work it
ephor work                                        # what it made of it
```

and then, whenever the world moves — a new comment, a gate that turned red —
`ephor refresh && ephor work sync` writes the next round of tickets and
`ephor work run` works them.

A recipe that says `"autorun": true` does not wait for that last command: the
ticket gets its run in the same breath it is written, and a timer running
`ephor work run --due` starts anything born elsewhere. The sweep behind it
reads the world rather than a memory — a root is due when it holds an open,
unclaimed, unparked ticket from such a recipe and no run is live on it — so it
is safe to invoke as often as anything cares to, and starts nothing on a root
that already has a run. Everything that says nothing about this is still
nobody's to start but yours.

In the TUI, `w` on any item opens its **work screen**: the tickets already
opened and what they reached, whether the item has moved under them, and the
recipes that apply now with the exact words each would send. `1`-`9` opens one,
`a` asks for something no recipe covers, `s` reopens stale work, `c` takes a
ticket back (through the runtime's own move into `cancelled`, with your reason
as its result — the plan keeps it), `R` hands this item's plan to the runtime
(which takes over the terminal, so you watch it work), `e` reads the plan.
Rows carry a badge — `⚙ fix-gate · review`, `✓ answer · done — …`,
`⊘ fix-gate · cancelled`, `⟳ 2 new messages`. A ticket a run has in hand right
now is `▶` in green instead, one the live run has not reached yet says
`· queued`, and a run that has gone quiet says `· quiet 12m`: open and being
worked on are different facts, and the row says which without your leaving it.

### By hand

Recipes are for the work that repeats; most work does not
([§FS-005-dispatch.10](docs/functional-spec/FS-005-dispatch.md#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)).
So nothing has to be written down in advance:

- **`a` on the work screen** — type one line and it becomes an ordinary ticket,
  same dossier, same plan, same order, with your words as the brief. It is
  never refused for not matching a selector: a merged pull request, an item no
  recipe covers, a second ask on work already running — all fair.
  `ephor work ask --item ID "bump the timeout to 30s and re-run the flaky test"`
  is the same thing from a script; with no words it reads them from stdin, which
  is how a longer ask composed in an editor gets in.
- **`⌨ run a command here…`**, the last entry of every `x` menu — type a shell
  command and it runs exactly as a configured action does: the item's checkout,
  the item's `EPHOR_*` environment, the terminal handed over. The menu now opens
  even when nothing is configured, because that is when you need it most.

**The ticket carries the dossier**, not a link: state, branch and checkout, the
gate's counts per repository and the forge's own reasons, and the conversation
quoted as messages with their authors. All of it was fetched during a refresh
already, so the work starts where you would have started.

**A moved item reopens its own work.** ephor fingerprints the item when it
dispatches — last activity, state, gate, how much conversation. When any of
that changes, `ephor work sync` appends a ticket to the *same* plan saying what
changed, ordered after the last one, and chosen against what the item is now: a
pull request whose gate has gone green and whose reviewer asked a question is
no longer a red gate. That is the loop — a plan per item, a ticket per round,
until nothing applies to it any more.

### Recipes

Five ship, and apply with no configuration at all: `fix-gate` (a pull request
of yours with failing jobs), `answer` (anything owing a reply), `review` (a
pull request you are reviewing), `implement` (an issue of yours), `rebase` (a
pull request of yours whose branch trails main). Add your own, or replace a
shipped one by reusing its id:

The shipped `implement` recipe uses `fix/issue-{number}` for an issue with no
branch. On a project with `branch_root_template`, dispatch mints that branch
and places the plan inside its workspace; without branch workspaces it refuses
by name and explains what to configure instead of writing at the project root.
A matter's existing forge or registry branch still wins. To deliberately keep
issue work branch-less, replace `implement` with a configured recipe carrying
your chosen semantics.

```json
{
  "work": {
    "root": "{workspace}/panta",
    "recipes": [
      { "id": "fix-gate", "icon": "🛠", "description": "fix the red gate",
        "state": "fix", "needs_checkout": true,
        "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
        "brief": "The gate on {title} is red. Run `just check` in {workspace} …" }
    ]
  },
  "projects": {
    "widget": {
      "work": { "recipes": [ { "id": "bench", "description": "run the benchmarks", "brief": "…" } ] }
    }
  }
}
```

A selector asks about `kinds` (`pr`, `ci`, `issue`, `task`, `message`,
`status`), `roles` (`author`/`reviewer`), `gate` (`failing` — jobs failed,
`blocked` — the forge refuses, `red` — either, `green`, `any`),
`needs_response`, and `sources`. Every field set must hold; finished work
never matches. A project's own tasks carry no role, so a non-empty `roles`
excludes them all — write a task recipe with `kinds: ["task"]` and no
`roles`; where a recipe is refused this way, `ephor work offers` names the
field that refused it
([§FS-005-dispatch.27](docs/functional-spec/FS-005-dispatch.md#27-an-offer-that-a-selector-refused-says-why)).
The brief takes `{title}`, `{url}`, `{repo}`,
`{number}`, `{branch}`, `{ticket}`, `{state}`, `{gate}`, `{workspace}`,
`{root}`, `{project}`, `{source}`, `{kind}`, `{id}`, and `{reply}` — the file a
drafted answer belongs in. A recipe may also pin the runtime's execution
identity with `"target"` or `"model"`, and say with `"branch"` which branch its
work belongs on for an item that has none of its own.

**An answer comes back as a proposal.** The shipped `answer` recipe asks for
the reply as a file of its own, and nothing posts it: the run writes it, ephor
reads it back, and the thread screen shows it under the conversation it answers
— `p` posts it through the same provider a reaction goes through, `e` edits it
first, and where the channel cannot carry a reply the card still names the file
you copy from
([§FS-005-dispatch.13](docs/functional-spec/FS-005-dispatch.md#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal)).
It needs no checkout, so a conversation is answerable on a project whose branch
is not on this machine.

### Where the work goes, and what runs it

`work.root` (default `{workspace}/panta`) is a rhei project directory in the
item's checkout. ephor creates it when it is missing — the manifest, a
`.gitignore` that ignores the directory itself so your repository stays clean,
and `assets/ephor-work.states.yaml` as `states.yaml`. **An existing
`states.yaml` is never replaced**: edit it for a different agent, model, or
timeout, or point `work.states` at one of your own. A recipe whose `state` the
machine in force does not declare is refused by name rather than written.

A rhei project that already holds plans of its own and declares no state
machine is refused: `states.yaml` is how every plan in a project resolves its
states, so writing one there would change what those plans run under. An empty
project — what `rhei init` leaves behind — has nothing to disturb, and ephor
moves in beside it. `ephor work states` prints the machine, for installing
deliberately (`ephor work states > panta/states.yaml`) or for editing.

The shipped machine is two agent passes — `fix` does the work and writes a
report, `review` reads that against the ticket and writes a verdict whose first
line is `VERDICT: done | partial | blocked`. ephor reads that line back onto
the row.

**What runs a plan is bound, not fused.** `work.runner` names the command;
unset, it is the runtime ephor ships wired and ready, and naming another is how
somebody who works differently points work at theirs
([§DA-001-runtime-bound-default](docs/decisions/architectural/DA-001-runtime-bound-default.md#da-001-runtime-bound-default-the-runtime-is-a-bound-default-not-a-named-coupling)).
With nothing installed under that name, everything except the running still
holds — tickets are written, read and reopened on disk — and only `ephor work
run` refuses, naming the runner it looked for.

**Scripts, before the agent.** Every ticket also carries the item as structured
metadata, so a state machine can hand a program `{meta.repo}`, `{meta.number}`,
`{meta.workspace}` and the rest, name its output with `{output.<name>.path}`,
and let the next state read that file as an `input` — a script asks the forge
what actually failed, and the agent starts from the answer. Its exit code picks
the next state, including `75` for "the gate is still running", which parks the
ticket on a poll instead of spending an agent on a half-finished gate.
`config/ci-failures.example.sh` and `config/ephor-work-ci.example.states.yaml`
are a working pair; the manual walks through them
([§8.5](docs/manual.md#85-a-script-in-front-of-the-agent)). Both passes are told to commit locally and touch nothing outward:
closing the loop out to the forge is a sentence you add to a brief
deliberately.

### Providers (`status.json`)

| provider | source | needs response when |
|---|---|---|
| `github-prs` | `gh search prs`: authored, plus with `reviews: true` PRs you are engaged with (`--commenter` / `--mentions`); gate from `gh pr checks` | authored: changes requested; reviewing: an unanswered citation |
| `github-ci` | `gh pr list` + `gh pr checks` per open PR | a check is failing |
| `github-issues` | `gh search issues` by role, plus comments | a comment awaits your reply |
| `github-notifications` | `GET /notifications` — GitHub's own list, the completeness net behind the searches you composed | the reason is a mention, a review request, an assignment, a broken gate, or an advisory |
| `github-threads` | GraphQL unresolved review threads | last comment is not yours |
| `custom-status` | any shell command in the workspace (`format: answer`, or the legacy `text` / `json`) | the answer says so |
| `<anything else>` | a forge extension: `ephor-forge-<name>` on `PATH` ([§FS-001-forge-interface.2](docs/functional-spec/FS-001-forge-interface.md#2-two-transports-one-interface)) | ephor's policy, over what it answered |
| `slack`/`discord`/`email` | stubs; activate by adding secrets under `~/config/secrets/ephor/` | mentions/DMs (planned) |

A task store in the checkout — `panta/`, `.beads/` — is read on every refresh
without any provider block, and reports under its own name
([§FS-006-project-interface.7](docs/functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).

**Answered detection**: a citation or thread stops needing a response once
you answered it — with a message afterwards, with a reaction on the
message, or by ticking the task it was waiting on. Task state outranks the
last word in both directions. Applies to github-prs mentions,
github-threads (unresolved threads), and to any forge extension's threads
and citations.

GitHub Enterprise: add `"host": "github.example.com"` to a github provider
block (sets `GH_HOST`).

Adding a provider: one module in `src/feed/providers/` implementing
`Provider`, plus a match arm in `providers::build_provider`. Per-project
customization without recompiling = `custom-status` shell commands.

### Periodic refresh

```bash
mkdir -p ~/.config/systemd/user
ln -sf ~/f/ephor/systemd/ephor-refresh.{service,timer} ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now ephor-refresh.timer
```

`ephor-work-sync.{service,timer}` is the same for work: it refreshes and then
reopens every dispatched item that has moved, half-hourly. It writes tickets
and runs nothing — spawning agents stays a thing you ask for.

## Development

```bash
just check     # the CI gate: fmt --check, build -D warnings, cargo + python
               # tests, the boundary check, grund
just test      # the Rust suite alone
just e2e       # the end-to-end scenarios alone (tests/e2e/cases)
just lint      # clippy -D warnings, deliberately not part of the gate
```
