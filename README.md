# ephor

A Rust CLI that watches over every project you work on. It manages the
project/workspace registry (extracted from the `automation` repo's Python
`dev/projects` tool) and aggregates a per-project information stream — PRs in
review, gate state, messages awaiting a response, Jira tickets, and custom
project status.

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
  ([§FS-001-forge-interface.5](requirements.md#5-no-site-specific-data-in-the-repository))
- `config/templates/` — AGENTS.md templates referenced by the registry
- `assets/workspaces.schema.json` — registry JSON Schema (embedded in the binary)
- `ai/skills/` — canonical agent skills (`track-project`); `ai/link-global-skills.sh` links them into `~/.claude/skills` etc.
- `systemd/` — user units for periodic feed refresh
- `docs/registry.md` — what the registry's concepts mean; `requirements.md`,
  `docs/roadmap.md`, and `docs/changelog.md` are the grund tree

## Install

```bash
just install            # cargo install --path . → ~/.cargo/bin/ephor
just link-skills        # link ai/skills/* into the global agent skill dirs

mkdir -p ~/.config/ephor
cp config/workspaces.example.json ~/.config/ephor/workspaces.json
cp config/status.example.json     ~/.config/ephor/status.json
```

The automation repo's `environment.d` exports `EPHOR_HOME=~/f/ephor` and
`PROJECTS_BIN=~/.cargo/bin/ephor`; `gco`/`gbr`/`gupd` reach ephor through
`PROJECTS_BIN`.

`cargo install ephor` is not available yet: nothing is published while the tree
still carries site-specific configuration — see
[§RM-001-forge-interface](docs/roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface).

## Requirements and releases

The repository is a [grund](https://github.com/vjovanov/grund) tree:
[requirements.md](requirements.md) declares what ephor must do,
[docs/roadmap.md](docs/roadmap.md) sequences what is not built yet, and
`grund check` verifies that every `§ID` citation resolves.

```bash
just check         # the CI gate: fmt, build with -D warnings, tests, grund
just pre-release   # everything a release verifies, publishing nothing
```

Releases follow [§FS-002-release](requirements.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change):
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
ephor mark-read widget | --all mark-read | mark-read --id ITEM_ID
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
behind` (or `· up to date`), where N sums `git rev-list --count
HEAD..origin/<main>` over **all** the workspace's repos (Widget CE + EE +
docs-site for a poly-repo workspace). Counts are measured locally at startup and on
`r`, against the last-fetched origin.

| key | action |
|---|---|
| `j`/`k`, arrows | move (skips headers) |
| `Tab` | switch stream ↔ projects |
| `Enter`/`l` | open the item's thread screen (falls back to the browser when nothing is recorded); on a project header: drill in |
| `o` | open in browser |
| `v` | thread screen, strictly (status message when empty) |
| `x` | summon the configured actions for the item (see below) |
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
page, `g`/`G` first/last, `+` react to the selected message, `Enter`/`o`
opens the item in the browser, `m` marks it done, `Esc`/`q` goes back.

**Reactions**: `+` opens a picker with GitHub's palette (👍 👎 😄 🎉 😕 ❤️
🚀 👀) — `←`/`→` or `1`-`8` choose, `Enter` posts via the provider
(GitHub comments post through `gh api graphql`; a forge that does not
declare the `reactions` capability is display-only). Reacting is often
enough to answer a mention — the item stops needing a response on the next
refresh.

### Item actions

`x` on any item (in the tree or the thread screen) summons its action menu
— `j`/`k` + `Enter` or `1`-`9` runs one, `Esc` cancels.

**Quick actions** need no configuration and lead the menu
([§FS-004-quick-actions](requirements.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)):
the source that produced an item offers what it knows to do about it.
Today `github-ci` offers `✗ see the CI failures` on a pull request whose
gate is red — the check list as GitHub reports it, then `gh run view
--log-failed` for every failed run, paged. One is offered only where it
would work: the gate is failing, the item still names its pull request,
and `gh` is installed.

**Configured actions** follow, from `status.json`: top-level `actions`
apply to every project, `projects.<id>.actions` are appended for that
project, and an optional `kinds` list restricts an action to `pr` / `ci` /
`message` / `status` items.

```json
{
  "actions": [
    { "icon": "⎇", "description": "check out PR branch",
      "command": "gh pr checkout -R \"$EPHOR_REPO\" \"$EPHOR_NUMBER\"", "kinds": ["pr"] },
    { "icon": "🧪", "description": "run the gate", "command": "just gate" }
  ],
  "projects": {
    "widget": {
      "checkout": { "description": "gco branch workspace",
                    "command": "GCO_NO_IDE=1 gco \"$EPHOR_BRANCH\"" },
      "actions": [
        { "icon": "💡", "description": "open ee/vm-enterprise in IntelliJ IDEA",
          "command": "nohup idea \"$EPHOR_WORKSPACE/ee/vm-enterprise\" >/dev/null 2>&1 &",
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

## Work: handing an item to an agent runtime

A watch that only watches hands you a list, and nearly every row on it has the
same next move — read the failures and fix them, answer the question, read the
change. ephor can hand that over
([§FS-005-dispatch](requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)):
an item plus a **recipe** becomes a ticket in a
[rhei](https://github.com/vjovanov/rhei) plan, written into the checkout the
item's branch resolves to. ephor writes files and nothing else — no comment, no
push, no pull request — and then keeps the ledger.

```bash
ephor work                                  # what has been dispatched, and what it reached
ephor work dispatch --dry-run               # what would be opened, and where
ephor work dispatch [--project P] [--recipe R] [--item ID] [--kind pr]
                    [--updated-within DAYS] [--again]
ephor work sync [--dry-run]                 # reopen work whose item has moved
ephor work run [--project P] [-- --parallel 2]   # rhei run, per work root
ephor work forget [--item ID | --done | --missing]
```

In the TUI, `w` on any item opens its **work screen**: the tickets already
opened and what they reached, whether the item has moved under them, and the
recipes that apply now with the exact words each would send. `1`-`9` opens one,
`s` reopens stale work, `R` hands the root to the runtime (which takes over the
terminal, so you watch it work), `e` reads the plan. Rows carry a badge —
`⚙ fix-gate · review`, `✓ answer · done — …`, `⟳ 2 new messages`.

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

Four ship, and apply with no configuration at all: `fix-gate` (a pull request
of yours with failing jobs), `answer` (anything owing a reply), `review` (a
pull request you are reviewing), `implement` (an issue of yours). Add your own,
or replace a shipped one by reusing its id:

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

A selector asks about `kinds`, `roles` (`author`/`reviewer`), `gate`
(`failing` — jobs failed, `blocked` — the forge refuses, `red` — either,
`green`, `any`), `needs_response`, and `sources`. Every field set must hold;
finished work never matches. The brief takes `{title}`, `{url}`, `{repo}`,
`{number}`, `{branch}`, `{ticket}`, `{state}`, `{gate}`, `{workspace}`,
`{root}`, `{project}`, `{source}`, `{kind}`, `{id}`. A recipe may also pin the
runtime's execution identity with `"target"` or `"model"`.

### Where the work goes

`work.root` (default `{workspace}/panta`) is a rhei project directory in the
item's checkout. ephor creates it when it is missing — the manifest, a
`.gitignore` that ignores the directory itself so your repository stays clean,
and `assets/ephor-work.states.yaml` as `states.yaml`. **An existing
`states.yaml` is never replaced**: edit it for a different agent, model, or
timeout, or point `work.states` at one of your own. A recipe whose `state` the
machine in force does not declare is refused by name rather than written.

The shipped machine is two agent passes — `fix` does the work and writes a
report, `review` reads that against the ticket and writes a verdict whose first
line is `VERDICT: done | partial | blocked`. ephor reads that line back onto
the row. Both passes are told to commit locally and touch nothing outward:
closing the loop out to the forge is a sentence you add to a brief
deliberately.

### Providers (`status.json`)

| provider | source | needs response when |
|---|---|---|
| `github-prs` | `gh search prs`: authored, plus with `reviews: true` PRs you are engaged with (`--commenter` / `--mentions`); gate from `gh pr checks` | authored: changes requested; reviewing: an unanswered citation |

**Answered detection**: a citation or thread stops needing a response once
you answered it — either with a message afterwards or with a reaction on
the message. Applies to github-prs mentions, github-threads (unresolved
threads), and to any forge extension's threads and citations.
| `github-ci` | `gh pr list` + `gh pr checks` per open PR | a check is failing |
| `github-threads` | GraphQL unresolved review threads | last comment is not yours |
| `custom-status` | any shell command in the workspace (`format: text|json`) | the JSON sets `needs_response` |
| `slack`/`discord`/`email` | stubs; activate by adding secrets under `~/config/secrets/ephor/` | mentions/DMs (planned) |

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

## Development

```bash
just check     # fmt --check + clippy -D warnings + cargo test
just test
```
