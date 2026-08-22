# The ephor manual

ephor watches over every project you work on, and hands the work it finds to
an agent runtime.

It has two halves, and they are one loop. The **watch** aggregates what is
happening across your projects — pull requests, gates, issues, conversations,
whatever a shell command wants to say — into one inbox that is worth believing
when it says there is nothing to do. The **work** hands any of it to
[rhei](https://github.com/vjovanov/rhei) as a ticket carrying everything the
watch already knew, and then keeps the ledger: what is being done, what it
reached, and whether the item has moved since.

This manual is the whole surface. [README.md](../README.md) is the tour;
[requirements.md](../requirements.md) is why each of these behaves the way it
does, and every `§ID` here points into it.

---

## Contents

1. [Install and first run](#1-install-and-first-run)
2. [The vocabulary](#2-the-vocabulary)
3. [The registry — `workspaces.json`](#3-the-registry--workspacesjson)
4. [The feed — `status.json`](#4-the-feed--statusjson)
5. [Providers](#5-providers)
6. [The inbox](#6-the-inbox)
7. [Actions](#7-actions)
8. [Work](#8-work)
9. [Automation](#9-automation)
10. [Extending ephor](#10-extending-ephor)
11. [Reference](#11-reference)
12. [Troubleshooting](#12-troubleshooting)

---

## 1. Install and first run

```bash
just install            # cargo install --path . → ~/.cargo/bin/ephor
just link-skills        # link ai/skills/* into the global agent skill dirs

mkdir -p ~/.config/ephor
cp config/workspaces.example.json ~/.config/ephor/workspaces.json
cp config/status.example.json     ~/.config/ephor/status.json
```

Then edit both, and:

```bash
ephor validate      # the registry parses, and its paths exist
ephor refresh       # fetch everything into the cache
ephor tui           # read it
```

ephor needs nothing else running. It has no daemon, no database, and no
network service of its own: everything it knows lives in three JSON files it
reads and one directory it writes.

### 1.1 The two configuration files

| File | Holds |
|---|---|
| `~/.config/ephor/workspaces.json` | the **registry**: your organizations, projects, checkouts, branches |
| `~/.config/ephor/status.json` | the **feed**: which providers watch which project, plus actions and work recipes |

Both are yours and neither belongs in a repository — they name your employer's
hosts and accounts ([§FS-001-forge-interface.5](../requirements.md#5-no-site-specific-data-in-the-repository)).
The repository carries `config/*.example.json` only.

There is a third place facts can live, and it is not yours: the **checkout**
itself — an `ephor.json` a project chose to write (§4.2.1) and the well-known
names it carries anyway. Every fact the interface uses lives in exactly one of
the three, and one order resolves them all
([§FS-006-project-interface.1](../requirements.md#1-the-three-homes)):

| Home | Holds | Example |
|---|---|---|
| the **registry row** | description and identity: where the forest is, how a branch becomes a workspace, what the project's matters are recognized by | `root`, `branch_root_template`, `territory` |
| **site configuration** | operational bindings: which command fills which verb, which runtime runs work, your actions and recipes | `providers`, `checkout`, `work.runner` |
| the **checkout** | what the project says (`ephor.json`) and the conventions it carries for its own sake | `checks`, `ci`, `./check.sh`, `panta/` |

**Site configuration over manifest over probe**
([§REQ-001-boundary.2](requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)):
a probe is a default, a manifest is the project declaring what probing would
have guessed, and you always have the last word.

### 1.2 Resolution order

| What | Order |
|---|---|
| Registry | `--registry` → `$EPHOR_REGISTRY` → `~/.config/ephor/workspaces.json` → `$EPHOR_HOME/config/workspaces.json` |
| Schema | `--schema` → `$EPHOR_SCHEMA` → the copy embedded in the binary |
| Feed config | `$EPHOR_STATUS_CONFIG` → `~/.config/ephor/status.json` → `$EPHOR_HOME/config/status.json` |
| State | `$XDG_STATE_HOME/ephor` → `~/.local/state/ephor` |
| Secrets | `~/config/secrets/ephor/<name>.json` |

`~` and `$VAR` / `${VAR}` are expanded in every path ephor reads from
configuration; an unknown variable is left as written rather than emptied.
Template paths inside the registry resolve relative to the registry file.

### 1.3 What ephor writes

Everything, in one directory:

```
~/.local/state/ephor/
  feed/<project>.json     one cache per project: items, per-provider status
  seen.json               unread tracking: item id → when you last read it
  work.json               the work ledger (§8)
```

Deleting any of it is safe. The feed comes back on the next `ephor refresh`;
`seen.json` coming back empty means everything reads as unread once;
`work.json` coming back empty forgets which plans belong to which item, and
the plans themselves stay where they are.

---

## 2. The vocabulary

Eleven words, and the rest of the manual is about them.

**Organization** — a group of projects with a folder of checkouts, e.g.
*Foundation — `~/f`*. Only for grouping and for the inbox's top level.

**Project** — one thing you work on: an id, a display name, a **root**
directory, a main branch, and a list of branches. It is the unit everything
else is keyed by.

**Project type** — the shape a project's checkout has: which repositories live
under the root, at which paths, how they are updated, and what its `AGENTS.md`
should say. Several projects share one type.

**Branch workspace** — a checkout per branch, when a project has
`branch_root_template`. `{project_root}/{branch}` gives
`~/c/g/you/ABC-42-retry/` for branch `you/ABC-42-retry`, holding the whole
project's repositories at that branch. Projects without a template are one
checkout at the root.

**Provider** — one source of items in one project: `github-prs`,
`custom-status`, an external forge. It is configured per project; the same
provider may watch several projects with different settings.

**Item** — one thing in the feed. It has a stable `id`, a `kind`
(`pr`/`ci`/`issue`/`message`/`status`), an optional `role`
(`author`/`reviewer`), a title, a url, a state as its forge spells it, a
`needs_response` flag, a last-activity time, and a `raw` blob its provider
filled with whatever else it knows — conversation, gate, branch.

**Gate** — a pull request's CI: job counts (passed / failed / running) per
repository the gate covers, plus, where the forge reaches a verdict of its own,
whether it blocks the merge and the reasons it gives. A gate is red when
something failed **or** the forge refuses.

**Category** — where an item lands in the inbox. Ephor's, never a provider's,
so every forge sorts the same way ([§FS-003-feed-categories](../requirements.md#fs-003-feed-categories-the-feed-sorts-itself-into-categories-and-finished-work-lands-in-recent)).

**Manifest** — the `ephor.json` a project may place at its forest root, saying
what it is called, how it is checked, how its gate is asked, where its tickets
are, and what it offers your menu. Offered, never required (§4.2.1).

**Recipe** — which items deserve work, and what the ticket asks for. Five ship;
you can add or replace any.

**Ticket** — one dispatch: a task inside a rhei plan, carrying the item's
dossier and the recipe's brief.

---

## 3. The registry — `workspaces.json`

The registry describes where your code is. ephor uses it to group the inbox, to
resolve the checkout an action runs in, and to place work.

```jsonc
{
  "organizations": [
    { "id": "acme", "name": "Acme", "root": "~/c/acme" }
  ],
  "project_types": [
    {
      "id": "monorepo",
      "layout": "monorepo",
      "repos": [
        { "id": "repo", "path": ".", "role": "Repository root",
          "required": true, "update_mode": "branch",
          "default_branch": "{branch}",
          "agents_description": "The repository root contains the full project." }
      ],
      "agents": {
        "template": "config/templates/root_agents.md.tmpl",
        "structure_intro": "This project uses a single repository root:",
        "summary_template": "This project root tracks `{display_name}` on `{branch}`."
      },
      "update_hooks": { "pre": ["fetch-all"], "post": ["reindex"] }
    }
  ],
  "hook_sets": [
    { "id": "fetch-all", "hooks": [ { "command": ["git", "fetch", "--all"], "cwd": "." } ] }
  ],
  "projects": [
    {
      "id": "widget",
      "organization": "acme",
      "type": "monorepo",
      "display_name": "Widget",
      "root": "~/c/acme/widget",
      "main_branch": "main",
      "branch_root_template": "{project_root}/{branch}",
      "tags": ["daily"],
      "release_branches": [ { "id": "widget-main", "branch": "main", "active": true } ],
      "branches": [
        { "id": "widget-retry", "branch": "you/ABC-42-retry", "ticket": "ABC-42", "active": true }
      ]
    }
  ]
}
```

### 3.1 Fields that matter most

| Field | Meaning |
|---|---|
| `projects[].root` | where the checkout is. Everything about a project resolves from here |
| `projects[].branch_root_template` | how a branch becomes a directory. Omit for a single-checkout project |
| `projects[].main_branch` | what branches are measured against ("N behind" in the inbox) |
| `projects[].branches[]` | the branches you have going: `id`, `branch`, `active`, and optionally `ticket` |
| `projects[].aliases` | other names this project answers to |
| `projects[].tags` | for `--tag` selection |
| `project_types[].repos[]` | the repositories under a root, their paths, and how `update` treats them (`update_mode: branch \| skip`) |
| `project_types[].agents` | how this project's root `AGENTS.md` is rendered |
| `hook_sets[]` | named command lists that `update` runs before and after |

`ticket` is optional: ephor infers a ticket key from the branch name when the
branch is named for one. The ticket is what links a feed item to a branch —
an item whose title or id contains `ABC-42` belongs to the branch that carries
that ticket.

### 3.2 Registry commands

```bash
ephor list                        # the table of registered projects
ephor validate                    # schema + semantics + the paths exist
ephor ensure-agents               # (re)render every root AGENTS.md
ephor update [--debug] [--skip-agents]
```

`update` walks each selected workspace: **pre hooks**, then per repository a
fetch / checkout / pull according to its `update_mode`, then **post hooks**,
then the root `AGENTS.md`. `--skip-agents` leaves the file alone; `--debug`
passes a debug flag through to hooks that declare `pass_debug`.

All four take the global selectors:

| Flag | Selects |
|---|---|
| `--workspace ID` | one project or derived workspace id, repeatable |
| `--tag TAG` | projects carrying a tag, repeatable |
| `--org ID` | one organization |
| `--all` | every branch entry, not only the active ones |

`ensure-agents` also renders an ad-hoc workspace that is in no registry:

```bash
ephor ensure-agents --type monorepo --root ~/tmp/scratch \
                    --display-name Scratch --var branch=main
```

---

## 4. The feed — `status.json`

```jsonc
{
  "defaults": {
    "ttl_seconds": 600,               // how old a cache may be before status refetches
    "provider_timeout_seconds": 30,   // per provider call
    "github_user": "you",             // skips one `gh api user` per refresh
    "recent_days": 7,                 // how long finished work stays under Recent
    "window": "tmux"                  // where a program of its own runs (§8.16)
  },
  "sources": [ /* §4.3 — fetched once, placed by ephor */ ],
  "actions": [ /* §7 */ ],
  "work":    { /* §8 */ },
  "projects": {
    "widget": {
      "providers": [ { "provider": "github-prs", "repos": ["acme/widget"], "reviews": true } ],
      "actions":   [ /* §7 */ ],
      "checkout":  { "command": "git worktree add \"$EPHOR_WORKSPACE\" \"$EPHOR_BRANCH\"" },
      "work":      { /* §8 */ }
    }
  }
}
```

Every block is validated strictly: an unknown key is an error, not a shrug, so
a typo fails at load instead of quietly doing nothing.

Only projects named under `projects` are watched. A project must also exist in
the registry — that is where its root and branches come from.

### 4.1 Commands

```bash
ephor refresh [PROJECT ...] [--quiet]      # fetch into the cache
ephor status [PROJECT] [--refresh|--cached] [--max-age SECS] [--json] [--check]
ephor feed [--project P ...] [--unread] [--kind pr|ci|issue|message|status] [--json]
ephor mark-read PROJECT | --all | --id ITEM_ID [--kind K]
ephor failures --project P --source S --repo R --number N
ephor rebase [--checkout DIR] [--project P] [--onto BRANCH | --upstream] [--item ID] [--dispatch] [--report PATH]
ephor checkout [--project P] [--branch B] [--item ID] [--from BRANCH] [--report PATH]
ephor capabilities [PROJECT] [--json]      # what a project can do, rung by rung
ephor doctor [--project P] [--skip-self|--self-only] [--json]
```

- **`refresh`** is the only command that fetches by default. It is what the
  timer runs.
- **`status`** with no project prints one row per project (items, unread,
  needs-response, failing). With a project, it prints that project's feed.
  It refetches anything staler than the TTL — pass `--cached` when you want
  the cache and nothing else, which is also much faster.
- **`feed`** is the flat cross-project stream, newest first, from the cache.
- **`mark-read`** marks items read. An item is unread until then, and
  **resurfaces when it changes again** — reading is per version, not per item.
  A full `--all` sweep also prunes entries for items that no longer exist.
- **`failures`** prints what went wrong under one red gate; it is what the
  quick action on a red gate runs.
- **`rebase`** replays a checkout onto its main branch — or, with
  `--upstream`, onto the branch's own published copy — and is what the quick
  actions on a branch that has fallen behind run
  ([§8.11](#811-rebasing-a-branch-that-has-fallen-behind)). Its exit codes are
  its own — `3` is a conflict, not a failure.
- **`checkout`** makes the branch workspace that is not there yet, one working
  tree per repository, and is what the quick action on a missing checkout runs
  ([§7.1](#71-quick-actions)). It needs nothing but the registry: the project
  says where the workspace goes, which repositories it holds, and what a new
  branch grows from. A workspace that is *partly* there is completed rather
  than reported as already there — this is the one command whose exit code
  answers "is this workspace whole", which is why every other fold can name a
  repository that is not on disk and carry on
  ([§8.11](#811-rebasing-a-branch-that-has-fallen-behind)).
- **`capabilities`** prints the ladder of §7.5 for a project — held rungs, and
  missing ones with the reason a refused action would show ([§4.3.2](#432-what-a-project-can-do--capabilities)).
- **`doctor`** asks whether any of this still works, and says so in its exit
  code ([§4.3.1](#431-is-it-still-working--doctor)).

### 4.2 Shared sources

A source that asks about specific repositories belongs to the project that
owns them, and lives under `projects.<id>.providers`. A source that asks
*nothing* — GitHub's notification list, a mailbox — answers about every
project at once, so it is declared at the top level in `sources` and fetched
once per refresh
([§AR-008-pipeline.1](architecture/AR-008-pipeline.md#1-fetch)):

```jsonc
{ "sources": [ { "provider": "github-notifications" } ] }
```

Where each of its findings belongs is then ephor's to decide, not the
source's: one matching engine weighs what the conversation carries against
what each project's registry row declares — its repositories, the
**territory** it claims beyond them, its ticket prefixes, the names it answers
to ([§FS-008-attribution](../requirements.md#fs-008-attribution-every-conversation-finds-its-project-or-says-that-it-could-not)).
An explicit venue wins outright, a reference places next, and resemblance only
argues; two projects claiming the same thing equally is **not** settled by
order — it goes to the unattributed bucket carrying both, because a guess that
lands wrong amends someone else's row silently.

```bash
ephor feed --unattributed     # what nothing claimed, and what two projects claimed
```

Declaring a shared source under a project still works and says so once per
refresh. The difference it makes: declared per project, a mention lands on
whichever project happened to fetch it; declared at site level, it lands where
it belongs.

### 4.2.0 Pointing work at a different runtime

`work.runner` names the command that runs a plan. Unset, it is the runtime
ephor ships wired and ready — choosing one is a property of how *you* work,
which is why one comes bound rather than demanded
([§FS-005-dispatch](../requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime),
[§DA-001-runtime-bound-default](decisions/architectural/DA-001-runtime-bound-default.md#da-001-runtime-bound-default-the-runtime-is-a-bound-default-not-a-named-coupling)):

```jsonc
{ "work": { "runner": "my-runtime" } }
```

The whole coupling is a plan file in a documented plain-text language, the
command configured to execute it, and the verdict read back from its results.
With nothing installed, every part of dispatch except the running still
holds — tickets are written, read and reopened, readable and diffable on
disk — and only running refuses, naming the runner it looked for.

### 4.2.1 What a project can say about itself — `ephor.json`

A project that wants to speak places one file at its forest root
([§FS-006-project-interface.2](../requirements.md#2-the-manifest-is-offered-never-required)).
It is **offered, never required**: every field is optional, an empty `{}` is
valid, and a project that places nothing is fully watchable exactly as it
stands. It may declare identity hints, its forest's own layout, check and gate
verbs, task stores, and offers — menu entries you invoke.

```jsonc
{ "identity": { "aliases": ["widget"], "territory": ["acme-labs"] },
  "forest":   [{ "name": "ce", "path": "ce" }],
  "checks":   { "check": "./check.sh",
                "smoke": { "command": "./ci/smoke.sh", "features": "list" } },
  "ci":       { "status": "./ci/gate.sh", "failures": "./ci/gate-failures.sh" },
  "tasks":    [{ "kind": "rhei", "path": "docs/plans" }],
  "actions":  [{ "id": "rebuild", "description": "rebuild it",
                 "command": "./build.sh", "cwd": "repo:ce",
                 "when": { "kinds": ["pr"] },
                 "requires": ["checkout-able"], "confirm": true }] }
```

| Block | Says | Where it is documented |
|---|---|---|
| `identity` | names, aliases, ticket patterns, repositories, territory, addresses — hints your row adopts or overrides ([§FS-008-attribution.1](../requirements.md#1-identity-is-declared-and-the-row-has-the-last-word)) | §4.2.2, [the registry](registry.md#identity-and-territory) |
| `forest` | the repositories under the root, as the project declares them ([§AR-004-forest.1](architecture/AR-004-forest.md#1-folds)) | §5.1, `EPHOR_REPOS` |
| `checks` | what fills `check`, `style`, `smoke` ([§FS-006-project-interface.5](../requirements.md#5-checks-are-verbs-and-every-script-is-self-contained)) | §4.2.3 |
| `ci` | what answers `status`, `failures`, `restart` ([§FS-006-project-interface.6](../requirements.md#6-the-gate-is-the-projects-in-three-verbs)) | §4.2.4 |
| `tasks` | task stores kept somewhere other than the probed names — `tickets` is the older spelling and is still read ([§FS-006-project-interface.7](../requirements.md#7-the-projects-own-tasks-are-read-where-they-live)) | §4.2.5 |
| `actions` | menu entries the project offers ([§FS-006-project-interface.9](../requirements.md#9-offers-the-projects-actions)) | §7.6 |

An offer is a menu entry in the same shape yours have (§7.2), selected by the
same `when` language and gated by the same rungs — it sits between the shipped
entries and your own, and yours wins on a shared `id` (§7.6). It takes the
terminal while it runs, which is what lets an offer be a pager or an editor;
one that needs no reader says `"background": true` and runs as a job instead
([§8.14](#814-jobs--what-ephor-runs-beneath-the-screen)).

Every command a block binds is a **summons**: it runs with `sh -c` in the place
the binding names (`cwd` is `root` — the default — or `repo:<name>`), it is
told whatever the caller knows in the usual `EPHOR_*` variables
([§7.3](#73-the-environment)) — a gate verb asked about a pull request gets
`EPHOR_REPO`, `EPHOR_NUMBER` and `EPHOR_BRANCH`, while `ephor check` in a bare
checkout has no matter to carry and passes none — and it answers with an exit
code and, optionally, the envelope it writes to `$EPHOR_ANSWER` (§4.2.6). `0`
is done, `75` is parked — not applicable now, ask again later — and anything
else failed
([§FS-006-project-interface.3](../requirements.md#3-a-summons-environment-in-exit-code-and-answer-out)).

Two rules make it safe to read. **The row is authoritative**: identity fields
are hints your registry adopts where it says nothing of its own and overrides
where it does, because attribution keys must not be forgeable by a checkout.
And **the row sets the trust**: `manifest_trust` is `full` (the default —
its commands run with the trust you extend to the project's own build),
`descriptions` (read what it says about itself, run none of it), or `ignore`.

Resolution is always **site configuration over manifest over probe**: probing
is defaulting, the manifest is the project declaring what probing would have
guessed, and your configuration overrides both.

```bash
ephor validate --manifest .        # check a manifest where it sits
ephor schema manifest              # the published schema, verbatim
ephor schema answer|registry|forge
```

The schemas are the interface's stability surface: what a release may change
is answerable by diffing them
([§FS-006-project-interface.11](../requirements.md#11-the-interface-is-versioned)).
Each validates offline — nothing in one refers to another by URL.

### 4.2.2 Territory

`territory` on a registry row names repositories and organizations that are
the project's business without being in its forest — `"acme/plugin"` for one,
`"acme"` for a whole organization. It is what places the general case: a
mention of you on some repository of the project's ecosystem, an issue filed
there, none of it in any checkout.

A manifest may hint the same things — `identity.aliases`, `identity.repos`,
`identity.ticket_patterns`, `identity.territory`, `identity.addresses` — and
the row adopts a hint where it says nothing of its own and overrides it where
it does. The row has the last word because a checkout must not be able to claim
another project's conversations
([§FS-008-attribution.1](../requirements.md#1-identity-is-declared-and-the-row-has-the-last-word)).

### 4.2.3 Check verbs — how a project says whether it is well

Three verbs, and a project fills the ones it has
([§FS-006-project-interface.5](../requirements.md#5-checks-are-verbs-and-every-script-is-self-contained)):

| Verb | Probed at the root | Is |
|---|---|---|
| `check` | `./check.sh` | the aggregate: everything the project considers a check |
| `style` | `./check-style.sh` | the fast style pass |
| `smoke` | `./smoke-test.sh` | the smoke, which may enumerate features |

A manifest's `checks` binds the same three under whatever paths the project
prefers, and your site configuration overrides both. **Each is
self-contained**: a smoke test that needs a build performs its build, because
how a project builds is the project's knowledge and stays there.

Smoke may enumerate **features** — `"features": "list"` runs the command with
`--list` (one id per line, or a `features[]` envelope), or the manifest lists
them outright. A feature id given as an argument runs that feature's smoke
alone; a smoke that enumerates nothing is one opaque verb, and that is a
complete implementation.

```bash
ephor check                          # the aggregate, or whatever else is declared
ephor check --verb style --verb smoke
ephor check --feature retry          # one feature's smoke
ephor check --list-features --json   # what a CI matrix fans out over
```

`ephor check` takes a checkout and nothing else — no registry, no site
configuration, no credentials — which is what lets the shipped CI step stand on
it ([§9.3](#93-ci-steps-ephor-ships)). Which verbs run and in what order is
policy above the interface: with none named it runs the aggregate where the
project declares one, and whatever else it declares where it does not, because
the aggregate is defined as everything the project considers a check and
running all three would run the style pass twice. A verb named and not there is
refused rather than skipped — a check nobody ran that nobody was told about is
what that rule exists to prevent.

### 4.2.4 Gate verbs — how a project's CI is asked what it is doing

How to ask a project's CI is project truth, the same for everyone who works on
it, so its home is the manifest's `ci` block, with your configuration
overriding where credentials or variants demand
([§FS-006-project-interface.6](../requirements.md#6-the-gate-is-the-projects-in-three-verbs)):

| Verb | Answers |
|---|---|
| `status` | what the gate is doing, per repository of the forest — the `gate` of an envelope |
| `failures` | what actually failed: the job, its log, the error where it can be had — the expensive question, asked on demand |
| `restart` | re-run the gate at the scope asked for — `EPHOR_RESTART` is `failed` (the failing gate and everything downstream of it) or `all` — committing nothing; `75` means "still running, ask again later" ([§FS-005-dispatch.11](../requirements.md#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed)) |

**A forge-hosted gate needs no manifest at all**: the provider's own gate
capability is the shipped default binding, which is why a pull request on
GitHub arrives with its counts and its failures without anybody writing
anything down. A project with an internal gate binds these three commands
instead, and the seam answers the same three questions from them.

Either way the *gated* rung holds ([§7.5](#75-why-something-is-not-offered)),
and that is what buys failure dossiers and the restart.

Today the row, the `✗ see the CI failures` entry and the two `⟳ restart …`
entries are all still drawn from what the **source** reported: `ephor failures`
and `ephor restart` ask the provider that reported the item
([§4.1](#41-commands)), so a manifest-bound gate is what a state machine's
program state and the capability table read, and the inbox's own views follow
the forge. Nothing in ephor invokes the bound `restart` verb yet — `EPHOR_RESTART`
is the name it will be handed the scope under, and the name a script in front
of the agent can read today. Binding these verbs is therefore worth doing where a script
in front of the agent asks them ([§8.5](#85-a-script-in-front-of-the-agent));
where your gate is the forge's, there is nothing to write.

### 4.2.5 The project's own tasks

A project may keep its own work in its own checkout — a plan directory, a
git-backed issue store — and ephor reads a **task store** it recognizes through
the store's own files, into the same feed under the same rules as anything a
forge reported ([§FS-006-project-interface.7](../requirements.md#7-the-projects-own-tasks-are-read-where-they-live)):

| Store | Probed | Read as |
|---|---|---|
| `rhei` | `panta/` | every open task heading in every plan, keyed `rhei:<plan>.<task>` |
| `beads` | `.beads/` | recognized; the reader is not written yet, so it reports nothing rather than pretending |

They are **tasks** and not tickets or issues: a ticket is what a remote tracker
keys, an issue is what a forge files, and these are the project's own work in
the project's own checkout.

A manifest's `tasks` adds a store the project keeps elsewhere, and declaring
one does not hide a probed one — a project may keep two, and both are read. The
older spelling of that key, `tickets`, is still read. Attribution needs no
configuration: a store in a checkout is about that checkout's project. The
tasks arrive under the source name the store carries (`rhei`, `beads`), so they
sit in **Tasks** ([§6.1](#61-the-categories)), carrying the state the store gave
them. Nothing is ever written back — the store is the project's, and ephor only
reads it.

A task in a **final** state is not read at all — final as the store's own
`states.yaml` says, or, where it declares none, as the runtime's built-in
default machine says (`pending`, and `completed` final) — because the store is
the record of the finished work and the feed shows what is open
([§FS-006-project-interface.7](../requirements.md#7-the-projects-own-tasks-are-read-where-they-live)).
A store whose machine cannot be read reports as a source that did not answer,
exactly like a plan ephor cannot read.

Finding a store is a capability, never an obligation: it buys the *tasks* rung
and nothing about a project without one degrades
([§7.5](#75-why-something-is-not-offered)).

### 4.2.6 What a verb may answer — the envelope

Every command ephor summons may write a JSON **envelope** to the file named by
`$EPHOR_ANSWER`, and that is the only structured channel: standard output is
never parsed for structure
([§FS-006-project-interface.4](../requirements.md#4-the-answer-envelope)).

```json
{ "v": 1,
  "summary": "12 passed, 1 failed",
  "gate": { "repos": [{ "repo": "app", "passed": 12, "failed": 1, "running": 0 }],
            "blocked": false },
  "failures": [{ "job": "unit / retry", "repo": "app",
                 "trace": "expected 3 attempts, saw 1", "log": "logs/unit.txt" }],
  "features": [{ "id": "retry", "description": "the retry window" }],
  "matters":  [{ "key": "acme/widget#42", "title": "…", "state": "open" }],
  "discussions": [{ "matter": "acme/widget#42", "messages": [{ "author": "Ada", "text": "…" }] }] }
```

Everything but `v` is optional, and a verb answers only what it has: a check
verb that writes `summary` and `failures` is complete, and one that writes
nothing at all still answered — with its exit code. Unknown fields are ignored,
which is what lets the envelope grow by addition, and relative paths in it
(`failures[].log`) resolve against the directory the command ran in.

The envelope is validated against the published schema before anything reads a
field of it, so a mistyped field is an error rather than a value that quietly
never arrives:

```bash
ephor schema answer > answer.schema.json    # validate your verb's output offline
```

### 4.3 Exit codes

| Code | Means |
|---|---|
| 0 | fine |
| 1 | the command failed |
| 2 | configuration or registry error |
| 3 | every provider failed — nothing could be fetched at all |
| 4 | some providers were lost (`refresh`, `doctor`), or unread needs-response items exist (`status --check`) |

`status --check` is built for a shell prompt: it prints nothing and exits 4
when something is waiting on you.

### 4.3.1 Is it still working? — `doctor`

Everything that makes the watch untrue is quiet
([§FS-010-doctor](../requirements.md#fs-010-doctor-ephor-can-be-asked-whether-it-still-works-and-answers-in-one-screen)):
a credential that expired, an extension that left `PATH`, a checkout somebody
deleted. None of them announces itself — each one simply makes a section of
the feed empty, which is the one thing an empty section must never mean. So
ask:

```bash
ephor doctor                  # the site, then ephor itself
ephor doctor --skip-self      # only ask the world
ephor doctor --self-only      # only ask the binary; reads nothing of yours
ephor doctor --json           # for whatever runs it on a timer
```

Two passes. **The site pass** refreshes every configured source — a cached
answer cannot say whether a source still answers — and then reads each
project's ladder (§7.5). It adds no opinion of its own: the sentence it
prints for a missing rung is the same one a greyed menu entry shows.

**The self pass** builds a throwaway project in a temporary place — its own
state directory, registry, configuration and checkout — and walks the seams
against it: a forge reached out of process, a refresh that categorizes what
came back, a summons answering by exit code and by envelope, check verbs
probed and declared, the checkout and the rebase, a dispatch whose ledger is
read back out of its plan, and a task store. It reaches no forge and
reads nothing of yours, so it is the half that works on a machine with no
site at all. Then it takes the temporary place away.

The exit code is the answer, and nothing is repaired on the way:

| Code | Means |
|---|---|
| 0 | well |
| 1 | the self pass failed — ephor itself is wrong |
| 3 | nothing on the site could be reached at all |
| 4 | degraded — a rung is missing, or a source was lost |

Running it on a schedule is yours to arrange; `systemd/` holds the two units
ephor already ships as the pattern to copy.

### 4.3.2 What a project can do — `capabilities`

The ladder of §7.5 for one project, or every configured one, without a
sweep and without asking any forge anything
([§FS-010-doctor.2](../requirements.md#2-the-ladder-is-answerable-on-its-own)).
It is the cheap answer to "why is this action not offered here".

```bash
ephor capabilities widget     # or `caps`, or with no project for all of them
ephor capabilities --json
```

```text
widget
  ✓ observable, placed, branch-addressable, checkout-able, workable
  ✗ checkable          /home/you/c/widget holds none of check.sh, check-style.sh,
                       smoke-test.sh, and its manifest binds none
  ✗ gated              no source reports a gate for widget, and no gate verbs are bound
```

It reads the last refresh rather than running one, so a project nobody has
refreshed says exactly that rather than reporting its sources as silent.

Under the ladder comes **who can be asked** — the roster the bound runtime
enumerates, one entry per agent-and-model combination it can actually serve
([§FS-005-dispatch.14](../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)):

```text
who can be asked (rhei)
  ✓ claude-code        claude-code · its own default model · efforts: yolo
  ✗ codex              codex · its own default model · efforts: yolo — codex is not on PATH
  ✓ pi                 pi · its own default model · efforts: high
```

ephor never builds that list itself. Which models an agent can carry, and
which efforts it declares, are the runtime's knowledge, so a cross-product
ephor assembled would be mostly combinations it could not know were invalid
([§DA-004-roster-is-asked-not-configured](decisions/architectural/DA-004-roster-is-asked-not-configured.md)).
An entry whose binary is missing is listed with that reason rather than
dropped, and with no runtime bound the section says so in the workable rung's
own words. `--json` carries the same two halves as `{ "projects": …,
"roster": … }`.

### 4.4 What a failing source does

A provider that cannot deliver **fails explicitly**
([§FS-001-forge-interface.6](../requirements.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)).
It never substitutes an empty answer, because an empty section has to mean
"nothing is waiting" and never "this source could not be read".

- Its last-good items stay in the feed, **marked `(stale)`** wherever they
  appear, and the provider that failed is named beside them.
- A host that could not be reached at all — DNS, refused connection, a VPN
  that is down — is reported as **unreachable**, which asks you for a network
  rather than for a fix.
- Every failure is named with its project on stderr and in the interactive
  header, and a run that lost any provider exits non-zero.

---

## 5. Providers

A provider block always has `provider`; the rest is its own.

| Provider | Source | Needs a response when |
|---|---|---|
| `github-prs` | `gh search prs` by every role: authored, and with `reviews` the ones you are in a thread on, cited in, asked to review, or assigned | authored: changes requested; reviewing: a review asked of you, or an unanswered citation |
| `github-ci` | `gh pr list` + `gh pr checks` per open PR | a check is failing |
| `github-issues` | `gh search issues` by role, plus comments; with `labels`, the open issues carrying them whoever is in them | a comment awaits your reply |
| `github-notifications` | `GET /notifications` — everything GitHub says is directed at you (§5.2) | GitHub's reason is a mention, a review request, an assignment, a broken gate, or an advisory |
| `github-threads` | GraphQL unresolved review threads | the last comment is not yours |
| `custom-status` | any shell command in the workspace | the JSON says so |
| `<anything else>` | an external forge executable (§10.1) | ephor's policy, over what it answered |
| `slack`, `discord`, `email` | stubs; activate by adding secrets | mentions and DMs (planned) |

Two more sources need no provider block at all: a **task store** in the
checkout is read on every refresh where one is there, and reports under its own
name — `rhei`, `beads` (§4.2.5). Nothing configures them; finding one is what
makes them a source.

### 5.1 Options

```jsonc
{ "provider": "github-prs",
  "repos": ["acme/widget"],     // empty searches the whole forge, not a list
  "reviews": true,              // include PRs you did not open, by every role
  "gates": true,                // record each PR's gate (one extra call per PR)
  "updated_within_days": 30,    // 0 removes the bound
  "limit": 30,                  // per search, per role
  "host": "github.example.com" } // GitHub Enterprise; sets GH_HOST

{ "provider": "github-ci", "repos": ["acme/widget"], "host": null }

{ "provider": "github-issues",
  "repos": [],                  // empty searches the whole forge, not a list
  "authored": true,             // issues you opened
  "participating": true,        // issues you are in but did not open
  "labels": [],                 // issues carrying any of these labels, whoever is in
                                //   them — open only, one search per label
  "updated_within_days": 30,    // 0 removes the bound
  "limit": 30,                  // per search
  "comments": true,             // fetch comments (one call per issue that has any)
  "host": null }

{ "provider": "github-notifications",
  "repos": [],                  // empty keeps every repository — see §5.2
  "read": false,                // include notices GitHub considers read
  "reasons": ["mention", "team_mention",   // empty keeps every reason
              "review_requested", "security_alert"],
  "updated_within_days": 30,    // 0 removes the bound
  "limit": 500,                 // a runaway guard; reaching it fails the source
  "host": null }

{ "provider": "github-threads", "repos": ["acme/widget"], "host": null }

{ "provider": "custom-status",
  "command": "git status --short | head -5",
  "format": "text",             // "answer", or the legacy "text" / "json"
  "cwd": "{project_root}/app" } // defaults to the project root
```

**Following a label.** `labels` asks a different question from the two role
searches: not *which issues am I in* but *which issues carry this word* —
`priority`, `regression`, whatever a project calls the work it wants followed.
Each label is one `gh search issues --label <name> --state open`, so issues
nobody has ever touched arrive too, and each lands under the role its author
gives it: **My Issues** where you opened it, **Participating** otherwise
([§FS-001-forge-interface.1](../requirements.md#1-capabilities)). Only open
issues are asked for — a label search that took the closed too would spend its
`limit` on history rather than on the queue. And a label search that comes back
with exactly `limit` issues **fails the source** rather than showing you part
of a queue as if it were the whole of it: raise `limit`, or narrow `labels`. A
block with `authored` off, `participating` off, and no `labels` asks nothing
and is refused when it is read.

**`custom-status`** runs its command as a summons like everything else ephor
asks of a project
([§FS-006-project-interface.3](../requirements.md#3-a-summons-environment-in-exit-code-and-answer-out)):
it runs in `cwd`, it is told about the project in the usual `EPHOR_*`
variables ([§7.3](#73-the-environment)), and its exit code is read the one
way — `0` reported, non-zero failed, `75` nothing to report just now.

With `format: "answer"` the command writes the envelope to the file named by
`$EPHOR_ANSWER`
([§FS-006-project-interface.4](../requirements.md#4-the-answer-envelope)); each
`matters[]` entry becomes an item, and an answer carrying only a `summary`
becomes one status line. This is the form every other verb speaks, and the one
to write today.

The two older forms read standard output instead and stay supported: `text`
makes one item whose title is the command's first line, and `json` reads an
object — or an array of them — printed on stdout:

```json
{ "title": "3 flaky tests quarantined", "state": "warn", "needs_response": true }
```

A command that writes `$EPHOR_ANSWER` is read through the envelope whatever
its configured format says, because writing one is unambiguous.

**Answered detection.** A citation or a thread stops needing a response once
you answered it — with a message afterwards, with a reaction on the message, or
by ticking the task it was waiting on. This is ephor's policy, applied
identically over every provider
([§FS-001-forge-interface.3](../requirements.md#3-policy-lives-above-the-interface-never-in-an-implementation)),
which is why reacting or ticking from the inbox is often enough to clear an
item. Task state outranks the last word in both directions: an open box awaits
you however the conversation ended, and a ticked one settles it even where
every message in the thread belongs to a robot.

### 5.2 The completeness net

`github-prs`, `github-ci`, `github-issues`, and `github-threads` all ask
questions you composed: *these* repositories, *these* roles. Each of those can
be wrong in a way nothing tells you about, because a question never asked and a
question answered "nothing" look identical on screen — an empty feed.

`github-notifications` asks nothing. It reads GitHub's own notification list —
the same one the bell icon shows — and reports whatever is on it. That is the
only way some things reach the feed at all:

- **a team mention.** `@acme/reviewers` names you, and no search qualifier will
  ever return it: `mentions:@me` finds what named *you*.
- **anything ephor has no capability for** — a discussion, a release, a
  security advisory, a repository invitation.
- **a repository you never configured.** The one worth catching is the one
  nobody thought to list.

It is meant to overlap the others, and overlapping costs you nothing: a pull
request two sources both found is one row, keeping the fuller report and
carrying over the reason only the notice knew
([§FS-003-feed-categories.5](../requirements.md#5-one-subject-is-one-row-however-many-sources-reported-it)).
So a pull request you were already tracking does not turn into two rows when
GitHub also mentions it — it turns into the same row, now saying that your team
was named on it.

Two ways to configure it. Leave `repos` empty on **one** project and that
project becomes the catch-all, holding everything no other project claimed;
this is the setup that makes an empty feed mean something. Or name `repos` per
project and each project's notices land in its own feed, at the cost of the
guarantee — a notice from a repository no project lists is then in no feed.

Reading a notification on github.com clears its flag here too: `read` notices
are skipped by default, and one that arrives already read never asks for a
response.

**`reasons` is what keeps it readable.** GitHub notifies about far more than it
asks of you. The default keeps the reasons that mean somebody is waiting on
*you in particular* — `mention`, `team_mention`, `review_requested`,
`security_alert` — and drops `subscribed`, `comment`, `author`, `state_change`,
`ci_activity`, and `assign`.

`assign` is the one worth explaining, because it is a real request and it is
still off. On a busy repository assignment is a bulk mechanism: one that assigns
every incoming contribution to its maintainers produces hundreds of unread
assignment notifications, and a net that catches all of those is a list nobody
reads — which is the same as no net. What is genuinely assigned to you already
arrives through `github-prs` and `github-issues` under its own role, with its
conversation and its gate attached. Add it back with
`"reasons": ["mention", "team_mention", "review_requested", "assign"]`, or set
`"reasons": []` to keep everything.

The vocabulary is ephor's once it lands: GitHub's `review_requested` and
`github-prs`'s own `review-requested` are one fact and appear once on a merged
row.

**Cost.** One paginated API call per refresh, whatever the size of your feed —
it is the cheapest source ephor has. If more notices than `limit` match, the
source **fails** rather than showing you an unknown fraction of them
([§FS-001-forge-interface.6](../requirements.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)):
its whole value is that you can believe it.

---

## 6. The inbox

```bash
ephor tui        # alias: ephor inbox
```

Everything is organized per organization → project → category → branch. Items
nest under the branch they belong to, matched by branch name first and by
ticket key after, with a *(not linked to a branch)* group for the rest.

A project's branches are the ones its registry row names **and** the ones it
has a workspace for on disk. You do not have to write a branch down to see its
work gathered under it: check it out — with `ephor checkout` or by hand — and
the next read picks it up, named for the directory it sits in. A branch the row
names keeps the row's word on it (its ticket, whether it is `active`); a branch
only the disk knows is shown as inactive and cannot widen what the project
claims as its own (§FS-008-attribution.2).

### 6.1 The categories

| Category | Holds |
|---|---|
| Status | project status lines |
| My Pull Requests | pull requests you authored |
| Reviewing | pull requests you are on as a reviewer |
| CI | gate and build results |
| My Issues | issues you opened |
| Participating | issues you are in but did not open |
| Tasks | the project's own tasks, from a store in its checkout (§4.2.5) |
| Messages | conversations attached to nothing else |
| Recent | finished work, for `recent_days` |

An item is in **exactly one**, so the size of a category is the size of that
pile of work. Finished work — closed, merged, done, resolved, declined —
leaves its category for **Recent** and leaves the feed entirely once it ages
past the window. Finished work never awaits a response: it is news, not a task.

### 6.2 Reading a row

```
! 5h  ✓ #24898 Fix lambda preservation  [open]  ✓253 ✗3 ⊘ blocked  (app ✓75 · plugins ✓178 ✗3)  ⚙ fix-gate · review
│ │   │                                  │       │              │    │                            │
│ │   │                                  │       │              │    └ per-repository breakdown    └ work (§8)
│ │   │                                  │       │              └ the forge refuses the merge
│ │   │                                  │       └ passed / failed / running across the whole gate
│ │   │                                  └ state, as its forge spells it
│ │   └ the branch workspace is on disk (∅ = it is not; blank = not knowable)
│ └ age of the last activity
└ needs a response ( * = merely unread, blank = read )
```

Branch rows say whether they are checked out, how far they trail the main
branch — summed across every repository in the workspace — and **as of when**:
`· 13 behind as of Jul 28`. Nothing in the inbox fetches, so the distance is
measured against the copy of the base this machine last pulled down, and the
date says when that was: the last time `origin/<main>` moved here, taken across
the workspace's repositories and reported as the oldest of them. A checkout
level with the base says `· level as of Jul 28` rather than *up to date* —
level as of a day you can see is a fact, "up to date" was a claim. Where no day
was ever recorded, which is what a fresh clone that has never fetched looks
like, the qualifier is simply left off: `· 13 behind`. A checkout that also
trails its own **published copy** — the pushed branch of the same name, read
per repository from each checkout's `HEAD` — carries a second distance in a
second color: `· 13 behind as of Jul 28 · ↓2` is thirteen commits behind the
project's main branch and two behind what was pushed of this branch. The arrow
carries no date: it is news that somebody pushed, and a stale reading of it can
only under-report. They are different
facts — main moved under you; someone (a teammate, another machine, the
forge) moved the branch itself — and a rebase answers each onto a different
ref, refreshing the reading as it goes. A tracking config that names the main branch is not a publication: it
records where the branch was cut. A repository parked on the main branch and
tracking it counts toward the first number alone — one distance never wears
both. A copy that is level shows no arrow, and a
branch never pushed shows none either — there is no copy to trail.

**On a reviewing row the state also says where you stand.** Before the colon is
the forge's word; after it is yours — `[open:approved]` is a change you have
approved, `[open:changes-requested]` one you sent back, `[open:review-requested]`
one still waiting on you. Being asked again outranks having answered, so a
re-requested review reads `review-requested` however you voted before. Where
your forge reports no verdict of yours — or where you have not reviewed — the
colon carries the reason the row is yours instead: `mentioned`, `assigned`,
`in-thread`. A verdict of your own is a capability an implementation declares
([§10.1](#101-a-forge-out-of-process)); `github-prs` reads it, and a forge that
does not report one simply never shows it.

### 6.3 Keys

**Navigator** — Stream (everything), Projects (one row per project), Detail
(one project and its branches):

| Key | Does |
|---|---|
| `j` `k`, arrows | move (skips headers) |
| `g` `G` | first / last |
| `Tab` | Stream ↔ Projects |
| `Enter` `l` | thread screen; on a project row, drill in |
| `o` | open in the browser |
| `v` | thread screen, strictly |
| `c` | the gate screen |
| `w` | the work screen (§8) |
| `x` | the action menu (§7) — commands and work alike, on an item and on a branch row |
| `m` `d` `Space` | mark done |
| `a` | mark everything visible done |
| `u` | unread-only ↔ everything |
| `;` | the operations board (§8.13) — from any screen |
| `[` `]` | previous / next project (Detail) |
| `Esc` `h` | back |
| `r` | refresh underneath the screen (in Detail, only that project) |
| `q`, `^C` | quit |

`r` does not take the terminal. The fetch runs on a thread of its own, so
every other key still answers while it is in flight — read, act, mark done,
or quit, and a run you walk out on is abandoned rather than waited for. Each
project takes its place in the feed as its own sources answer, rather than
the whole run landing at the pace of the slowest forge, and the header carries
`Refreshing graal (3/7)…` for as long as it is running: a screen that stays
live is also a screen that looks finished, and a half-filled feed read as the
whole answer is the same lie as an empty section that only means "not asked
yet". The projects are still asked one at a time, so the refresh costs your
forges exactly what it did before. Pressing `r` again during a run says
`Already refreshing` rather than starting a second one.

**Thread screen** — the recorded conversation in full, each message a card with
its author, age, text and reactions:

| Key | Does |
|---|---|
| `j` `k` | previous / next message |
| `f` `b` | page |
| `g` `G` | first / last |
| `+` | react (`←`/`→` or `1`-`8` choose, `Enter` posts) |
| `t` | tick the selected task |
| `e` `p` | edit / post a drafted reply (§8.12) |
| `x` | actions · `o` open · `m` done · `;` ops · `Esc` back |

`+` and `t` are offered on the selected message rather than on the screen, so
the footer changes as you move: a message its forge will not take a reaction
for never advertises `+`, and `t` appears only on a task still open. The
palette is GitHub's eight: 👍 👎 😄 🎉 😕 ❤️ 🚀 👀. A forge that does not
declare the `reactions` capability is display-only.

**Tasks.** Where a forge tracks tasks — a checklist item, a blocker comment, a
review task — the message carrying one renders with its box, ☐ or ☑, and `t`
ticks it in place. That is the whole of a bot checklist: read the sentence,
agree with it, move on, without a browser.

A box also answers the thread. An unresolved task keeps its conversation
awaiting you however it ended, and a resolved one settles it even though a
robot had the last word — which is what keeps a pull request whose boxes are
all ticked from sitting in the inbox forever
([§FS-003-feed-categories.4](../requirements.md#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).

**Gate screen** (`c`) — the per-repository counts spelled out and the forge's
own reasons for refusing, verbatim: `j`/`k` scroll, `x` actions, `o` open,
`Esc` back.

**Work screen** (`w`) — §8.7.

---

## 7. Actions

`x` on any item summons its menu: `j`/`k` + `Enter`, or `1`-`9`, `Esc` cancels.
Everything that can be done about the row is on it — the commands that run
here, and the work that can be handed to an agent (§7.6) — so *what can I do
about this* has one answer rather than depending on which key you knew. The
footer says what `Enter` would do on the entry you are standing on, because it
is not the same thing on all of them.

`x` on a **branch row** summons the branch's own: what ephor recognizes about
the checkout — the two rebases (§7.1), and the checkout where the workspace is
not there yet. Nothing else is on it, because a source's, a project's and a
person's entries — and the recipes — are selected against an item, and a
branch row has none.

### 7.1 Quick actions

Entries ephor has without being told, on an item where it already knows what
the problem is ([§FS-004-quick-actions](../requirements.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)).
They lead the menu, and configuration adds to them rather than replacing them.

Today, six:

**`✗ see the CI failures`** on a pull request whose gate is red — the check
list as the forge reports it, then every failed job's log, paged. It is offered
only where it would work: the gate is failing, the item still names its pull
request, and the tool that reaches it is installed.

**`⟳ restart what failed`** and **`⟳ restart the whole gate`** on an item
carrying a gate ([§FS-004-quick-actions.9](../requirements.md#9-a-gate-is-offered-the-restart-in-two-shapes)).
The first is the ordinary case — a runner died, a mirror was unreachable, the
same flake landed on the same job again — and it asks for that work back
without re-running the whole gate. *How much* less is the forge's to say, and
the row says which (below). The second is for when the merge commit itself is suspect: the
base moved under the change, a cache was poisoned, and the green results are as
untrustworthy as the red ones. Re-running everything to recover one job is an
hour of a shared machine pool, which is why it is a separate keystroke, and why
it asks before it runs.

A **red** gate gets both. A gate that is not red — green, still running,
blocked on an approval — gets only *restart the whole gate*, the one that still
has something to do there. An item with no gate at all gets neither.

Both run beneath the screen as jobs ([§8.14](#814-jobs--what-ephor-runs-beneath-the-screen)):
a restart asks nothing and the gate answers minutes later, so the interface
stays where it is and the log says what was asked for. On GitHub they resolve
the pull request's head commit and re-run its workflow runs — `--failed` is
GitHub's own *rerun failed jobs*, so *restart what failed* really is per-job —
and a check that is not a workflow run, an external status somebody else's
system wrote, is named rather than silently skipped. On a project whose forge
is reached through an extension, the same two entries run `ephor restart`
([§11.1](#111-every-command)) and the forge answers what it actually asked for.

Read the row before you press it. Where a forge starts its gate as a whole
rather than job by job, *restart what failed* is the failing gate **and
everything downstream of it** — the entry says `restart what failed, and
downstream` there, because on a gate spanning a tree that can be most of the
tree ([§FS-006-project-interface.6](../requirements.md#6-the-gate-is-the-projects-in-three-verbs)).

**`⤴ rebase onto <main> (13 behind as of Jul 28)`** wherever there is a branch
workspace on disk and the project names a `main_branch` to replay onto
([§FS-004-quick-actions.6](../requirements.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
It is offered whether the branch measured behind or level — `(level as of Jul
28)`, or just `(level)` where no fetch was ever dated — because that reading is
only as fresh as the last fetch and this is the move that would refresh it: the
replay fetches first, so a branch that went stale without this machine hearing
about it is exactly the branch you want the entry on. A branch genuinely level
replays onto nothing and says so.
Any row that resolves to that workspace carries it — a pull request, an issue,
a status a source filed about the same change — and **so does the branch row
itself** in the detail view, which is where the `13 behind as of Jul 28` you
are reacting to is written. It runs `ephor rebase` in that checkout — fetch, replay, and an
answer per repository, every repository in a poly-repo workspace. Where the
replay stops in a conflict, it opens the ticket about it instead of leaving you
with a half-finished rebase and no record of it ([§8.11](#811-rebasing-a-branch-that-has-fallen-behind)).
A project whose registry row names no `main_branch` is not offered this one:
there is no base to name, and the entry would have nothing to say.

**`⤴ rebase onto <remote>/<branch> (2 behind as of Aug 11)`** on the same
checkouts and the same rows, about its own **published copy** instead —
somebody else pushed to your branch ([§FS-004-quick-actions.8](../requirements.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)).
It follows the entry above in both respects: offered wherever there is a copy
to replay onto, behind it or level, and dated from that copy's own ref rather
than the base's — a fetch dates only the refs it actually brought down, so the
two days rarely match.
This one needs no `main_branch` at all: each repository resolves its own copy,
so a project that names no main branch is still offered it.
It runs `ephor rebase --upstream`, which replays each repository onto its own
copy rather than onto one branch name for the whole workspace, and reports a
repository that has published nothing as exactly that. A repository whose copy
simply *is* its base — a workspace repository parked on the main branch and
tracking it — has its distance counted by the entry above alone, so it neither
inflates this entry's number nor names its ref; a workspace of nothing but
such repositories is not offered this entry at all, because it would run the
entry above under another name.

**`⇣ check out <dir>`** on an item whose branch workspace is *not* on disk, for
a project that keeps one checkout per branch
([§FS-004-quick-actions.7](../requirements.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)).
It runs `ephor checkout`, which needs nothing configured — the project's
`branch_root_template` says where the workspace goes, its type says which
repositories it holds, and its `main_branch` says what a new branch grows from.
Each repository gets its own working tree: the branch itself where that
repository has it, and a new branch of the same name off the main branch where
it does not, which is what a change touching one repository of a tree looks
like on disk. A repository whose branch another working tree is already holding
is reported and left alone — git refuses that, and it is right to. Run it on a
workspace that is already there and it says so and changes nothing; run it on
one holding only some of the project's repositories and it makes the rest,
because a directory is not a workspace and this is the command that answers
whether one is whole.

It is also the step that runs *before* any other action on a missing workspace.
Pick `⧉ open the diff` on a branch you have never checked out and ephor checks
it out first, then runs what you picked in it. Configure a `checkout` command
for the project and yours runs instead ([§7.2](#72-configured-actions)); the
difference is only whether anyone expects to want their own.

### 7.2 Configured actions

```jsonc
{
  "actions": [
    { "icon": "⎇", "description": "check out the PR branch",
      "command": "gh pr checkout -R \"$EPHOR_REPO\" \"$EPHOR_NUMBER\"",
      "kinds": ["pr"] },
    { "icon": "🧪", "description": "run the tests", "command": "just test" }
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

| Field | Meaning |
|---|---|
| `id` | what an entry of the same name overrides (§7.6); empty is anonymous |
| `icon`, `description` | the menu row |
| `command` | run with `sh -c` in the item's checkout |
| `agent` | ask for work instead of running a command — see below |
| `cwd` | where it runs: `workspace` (default), `root`, or `repo:<name>` |
| `kinds` | restrict to item kinds; empty offers it everywhere |
| `when` | which items it is offered on, in the language recipes use (§8.3) |
| `requires` | capability rungs it needs (§7.5); an unmet one shows its reason |
| `requires_checkout` | the action needs the item's branch workspace on disk |
| `confirm` | ask before running it: the second Enter on the row runs it |
| `background` | run it beneath the interface as a job rather than taking the terminal (§8.14) |
| `window` | run its program in a window of your own instead of taking the terminal (§8.16) |

`kinds` is the older spelling of `when.kinds` and still works; `when` is the
whole language — roles, gate, `needs_response`, sources, `behind` — so an
action can be offered exactly where a recipe would be.

**An entry may ask for work instead of running a command.** Write an `agent`
block and no `command`, and the entry becomes a ticket rather than a process
([§FS-005-dispatch.1](../requirements.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)):

```jsonc
{ "id": "changelog", "icon": "✎", "description": "write the changelog bullet",
  "when": { "kinds": ["pr"], "roles": ["author"] },
  "agent": {
    "brief": "Write the changelog bullet for {title}, in house style.",
    "state": "fix",          // optional; the shipped machine's working state
    "hand": "luna:high"      // optional; who does it (§8.4)
  } }
```

| Field | Meaning |
|---|---|
| `brief` | what the ticket asks for, with `{placeholders}` filled from the item (§8.2) |
| `state` | the state a fresh ticket starts in; the shipped machine's working state unwritten |
| `hand` | who does it — the second of the seven steps in §8.4 |

That entry **is** a recipe (§8.3): the same selector, the same brief, the same
hand, dispatched by the same path the work screen's key uses, into the same
plan and the same ledger. It takes its id, icon, description and `when` from
the entry, which is why an agent entry needs an `id` — the id names its ticket
and is the key a `work.hands` table answers by. `command` and `agent` are
mutually exclusive and one of them is required: an entry that has both, or
neither, is refused when the file is read.

**The checkout dependency.** A project may define one `checkout` command whose
contract is to make `$EPHOR_WORKSPACE` exist — ephor verifies the directory
afterwards rather than trusting it
([§FS-006-project-interface.8](../requirements.md#8-the-checkout-contract)). Actions marked `requires_checkout` are
gated on it: when the workspace is missing the menu annotates them *(will check
out first)* and running one chains checkout → action. On an item linked to no
branch there is no workspace to make, so they show *(unavailable)* with the
reason on the row itself — and `Enter` there leaves the menu standing rather
than taking it down to repeat in the header what you are already reading.

**Where a command runs.** In the item's checkout, resolved org → project →
branch: the item is matched to its registry branch, and if that branch
workspace exists on disk the command runs there; otherwise it runs in the
project root. The interface leaves the screen entirely while it runs — so
`lazygit`, an editor, a pager all work — and returns on Enter.

**Unless the entry says it needs nobody.** `"background": true` runs it as a
job instead: the interface stays where it is, the job gets a row on the
operations board, and its output goes to a log you can read then or later
([§8.14](#814-jobs--what-ephor-runs-beneath-the-screen)). The default is the
terminal precisely because of the sentence above — an editor started beneath
the screen is a program nobody can type into. ephor's own `⤴ rebase` entries
set it themselves: a replay asks nothing.

### 7.3 The environment

| Variable | Value |
|---|---|
| `EPHOR_PROJECT`, `EPHOR_ROOT` | project id and its registry root |
| `EPHOR_WORKSPACE` | the checkout the command runs in (also the cwd) |
| `EPHOR_BRANCH`, `EPHOR_TICKET` | provider-recorded branch, or the matched registry branch and its ticket |
| `EPHOR_ITEM_ID`, `EPHOR_SOURCE`, `EPHOR_KIND` | item identity (`pr`/`ci`/`issue`/`msg`/`status`) |
| `EPHOR_TITLE`, `EPHOR_URL`, `EPHOR_STATE` | display fields, empty when absent |
| `EPHOR_REPO`, `EPHOR_NUMBER` | best-effort `owner/name` and number |
| `EPHOR_RAW` | the item's whole raw JSON, for `jq` |
| `EPHOR_ANSWER` | a file to write a structured answer to, if the command has one ([§FS-006-project-interface.4](../requirements.md#4-the-answer-envelope)) |
| `EPHOR_REPOS` | the workspace's repositories, one per line, in the order the project declares — what to fold over ([§AR-004-forest.1](architecture/AR-004-forest.md#1-folds)) |

Exit codes are read the same way wherever a command is summoned from: `0`
done, non-zero failed, and `75` **parked** — not applicable now, ask again
later ([§FS-006-project-interface.3](../requirements.md#3-a-summons-environment-in-exit-code-and-answer-out)).

### 7.4 One-off commands

The last entry of every menu is **`⌨ run a command here…`**. Type a shell
command and it runs exactly as a configured one does — same checkout, same
environment, same handover of the terminal. The menu opens even when nothing is
configured, because that is when this entry matters most
([§FS-005-dispatch.10](../requirements.md#10-what-ephor-offers-is-not-a-limit-on-what-can-be-asked)).

### 7.5 Why something is not offered

What a project can do is a ladder, and every feature names the rungs it needs
([§FS-006-project-interface.10](../requirements.md#10-capability-rung-by-rung)).
A rung you do not hold degrades exactly the features that named it, and the
reason is shown where the feature would have been — never an error, never
silence. `ephor capabilities` prints the whole table for a project, which is
the cheap way to ask this question outside the inbox
([§4.3.2](#432-what-a-project-can-do--capabilities)).

| Rung | Held when | Buys |
|---|---|---|
| observable | a registry row and at least one source **answering** at the last refresh | the watch |
| placed | the project's root is on disk | actions and update |
| branch-addressable | the row has a `branch_root_template` | a workspace per branch |
| checkout-able | a checkout command is bound, or a checkout is on disk to grow one from | work that edits |
| checkable | `check.sh`, `check-style.sh`, or `smoke-test.sh` at the root, **or** a manifest `checks` block (§4.2.3) | verification that means something |
| gated | a source reports a gate, **or** a manifest `ci` block binds one (§4.2.4) | failure dossiers and the restart |
| tasks | a `panta/` or `.beads/` store at the root, in any branch workspace on disk, or one a manifest declares (§4.2.5) | the project's own tasks as matters |
| workable | the configured runner is on `PATH` | running the work |

The ladder is resolved per project when the inbox loads, when a refresh
finishes, and after a checkout — it costs a handful of `stat` calls and never
runs anything. What it says is nonetheless what *was* true when it was
resolved, so a command about to run re-checks the two things it leans on (its
directory, and its script if it names one) and fails as the world rather than
from the table
([§AR-005-capabilities.3](architecture/AR-005-capabilities.md#3-the-table-is-honest-about-time)).

An entry — yours or the project's — names the rungs it needs in `requires`,
and an unmet one leaves the row where it is, marked with the ladder's own
sentence. A word that is not a rung is *also* refused, by name: a requirement
nobody checks would be worse than one nobody wrote.

### 7.6 Four places a menu entry comes from

The menu is assembled in provenance order
([§FS-006-project-interface.9](../requirements.md#9-offers-the-projects-actions)):

1. **what ephor recognized** — a source's own quick actions (§7.1), the rebase
   on a branch that is here, the checkout on a workspace that is not there;
2. **what the project offers** — the `actions` of its `ephor.json` (§4.2.1),
   under the trust your row extends to it;
3. **what you configured** — `actions` and `projects.<id>.actions` (§7.2);
4. **what can be handed over** — the recipes that apply to this item (§8.3),
   marked with `→` and the hand that would get them
   ([§FS-005-dispatch.1](../requirements.md#1-a-recipe-decides-which-items-deserve-work-and-what-to-ask-for)).

On an entry that hands work over, **`t` opens the picker**
([§FS-005-dispatch.14](../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)):
the roster's hands in one column and, beside a hand that declares efforts,
those efforts in a second — absent where it declares none, which is every
hand on a machine with no model profiles. Arrows move between the columns,
`j`/`k` within one, `Enter` dispatches the entry to what is highlighted,
`Esc` returns to the menu. An unavailable hand is listed with its reason and
cannot be chosen; a hand the project's `permitted_hands` excludes is not
listed at all; with an empty roster there is no picker and the entry
dispatches as it always did. The pick is for that one dispatch alone — the
first of §8.4's seven steps — and nothing remembers it: the next dispatch of
the same action resolves from the tables again.

Where two entries share an `id`, the later one wins **in the place the earlier
one held**: yours beats the project's beats the shipped one, and the key that
ran a thing goes on running that thing. An entry with no `id` overrides nothing
and is overridden by nothing, which is what every action written before ids
existed is.

Recipes are the exception to that rule, and in the other direction: a recipe
whose id the menu already carries is **dropped**, because that entry is what
hands the work over when it cannot finish. `rebase` is the case — the key runs
the replay and opens the ticket only where git stops (§8.11) — so a stale
branch shows one rebase row, not two. `x` and `w` therefore answer the same
question: `w` is the whole work screen for an item, with the plan and what each
ticket reached; `x` is the same recipes among everything else you could do
about the row.

That is the whole difference between the four: an offer is selected, gated,
run, and refused exactly as your own action is — it is only invoked by you, and
never runs on its own.

---

## 8. Work

An item plus a **recipe** becomes a ticket in a rhei plan, written into the
checkout that item's branch resolves to. ephor writes files and nothing else —
no comment, no push, no pull request — and then keeps the ledger
([§FS-005-dispatch](../requirements.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)).

### 8.1 The loop

```bash
ephor refresh                                     # what happened
ephor work dispatch --dry-run --updated-within 14 # what would be handed over
ephor work dispatch --updated-within 14           # hand it over
ephor work run                                    # let the runtime work it
ephor work                                        # what it made of it
```

and afterwards, whenever the world moves — a new comment, a gate that turned
red — `ephor refresh && ephor work sync` writes the next round and
`ephor work run` works it.

### 8.2 What a ticket carries

The **dossier**: the state, the branch and the checkout, the gate's counts per
repository with the forge's own blocker text verbatim, and the conversation
quoted as messages with authors and dates. All of it was fetched during a
refresh already, so the work starts where you would have started rather than
spending its first move re-fetching what ephor had.

The dossier is bounded — messages per thread, messages in total, characters per
message — and every thread keeps a share before any thread takes a second, so a
bot posting the review policy twice cannot crowd out the human question under
it. Where anything was dropped, the ticket says so and links to the whole. An
item whose conversation was never recorded says *that*, rather than showing an
empty section that reads as silence.

```markdown
# Rhei: #17 Humanize durations in the log reader
**States:** ephor-work

<!-- ephor:dossier -->
## The item

- **project**       demo
- **kind**          pr (author)
- **state**         open
- **url**           https://forge.example/demo/pull/17
- **branch**        main
- **checkout**      /home/you/c/demo
- **waiting on**    an answer from me

## The gate

✓1 ✗2 ⊘ blocked

The forge gives these reasons:

- Requires approvals — you still need one approval.

## The conversation

**Ada** · 2026-08-11
…
<!-- /ephor:dossier -->

## Tasks

### Task fix-gate-1: fix the red gate — #17 Humanize durations in the log reader
**State:** fix

The gate on #17 … is red. Find out what actually failed …
```

The dossier lives between markers and is **rewritten** on every later dispatch,
so the top of the plan is always current. Tickets are only ever appended: their
`**State:**` lines belong to the runtime, which may be advancing one right now.

### 8.3 Recipes

Five ship and apply with no configuration at all:

| Recipe | Applies to | Needs the branch on disk |
|---|---|---|
| `fix-gate` 🛠 | a pull request of yours whose **jobs failed** | yes |
| `answer` 💬 | anything owing a reply — pr, issue, message | no |
| `review` 👓 | a pull request you are reviewing | no |
| `implement` 🧩 | an issue you opened | no |
| `rebase` ⤴ | anything whose branch is here and **trails main** | yes |

Recipes are also entries of the action menu (§7.6): `x` on a row offers the
work that can be handed over about it beside the commands that can be run on
it, each saying who would get it. `w` is the same recipes with the plan and
the tickets around them.

Add your own, or replace a shipped one by reusing its id:

```jsonc
{
  "work": {
    "root": "{workspace}/panta",       // where plans go
    "states": "~/my/states.yaml",      // a machine of your own, instead of the shipped one
    "recipes": [
      {
        "id": "fix-gate",
        "icon": "🛠",
        "description": "fix the red gate",
        "state": "fix",                 // the state a fresh ticket starts in
        "needs_checkout": true,
        "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
        "brief": "The gate on {title} is red. Run `just check` in {workspace} …",
        // The runtime's own words, unchecked — or "model": "…". Mutually
        // exclusive with "hand" (§8.4): a recipe carrying both is refused.
        "target": "claude-code[yolo]:anthropic:claude-sonnet-4-6"
      }
    ]
  }
}
```

Per project, `projects.<id>.work` takes the same three keys and its recipes are
appended to the global ones.

**The selector.** Every field that is set must hold; an empty one asks nothing.
Finished work never matches.

| Field | Values |
|---|---|
| `kinds` | `pr`, `ci`, `issue`, `message`, `status` |
| `roles` | `author`, `reviewer` — an item whose source reported no role matches only when this is empty |
| `gate` | `failing` (jobs failed) · `blocked` (the forge refuses) · `red` (either) · `green` · `any` |
| `needs_response` | `true` / `false` |
| `sources` | provider names |
| `behind` | `true` — the branch trails the project's `main_branch` · `false` — level with it |
| `behind_upstream` | `true` — the branch trails its own **published copy** · `false` — level with it |

`failing` and `blocked` are separate because they ask for different work: jobs
that failed are something a checkout can fix, while a forge refusing an
otherwise green change is usually waiting on a person.

`behind` is the recipe's own question — *is there anything to replay?* — and so
it is still `false` on a branch measured level, even though the menu entry
beside it is offered there (§7.1): handing a level rebase to an agent is a
ticket to do nothing, while pressing the key runs git and finds out. It is
measured in your own checkout, not asked of a forge: each of the
branch workspace's repositories is counted against `<its remote>/<its base>` as
it was last fetched — the remote read off the repository, and the base its own
`default_branch` where the row names one that is a branch rather than a
template, the project's `main_branch` otherwise, and what its remote calls its
default where neither says. An item ephor cannot measure — no branch, or nothing on
disk — matches neither `true` nor `false`, so a recipe that asks is never
offered blind.

`behind_upstream` is the same measurement against the other ref: the branch's
own published copy ([§6.2](#62-reading-a-row)), read per repository from its
`HEAD`. A branch published nowhere matches neither value, for the same reason
— and a repository whose copy is simply its base again counts toward `behind`,
not here: one distance answers one question.
The two are different questions — a branch level with main can be well behind
what was pushed of it — and a recipe may ask both.

**The brief** takes `{title}`, `{url}`, `{repo}`, `{number}`, `{branch}`,
`{ticket}`, `{state}`, `{gate}`, `{workspace}`, `{root}`, `{project}`,
`{source}`, `{kind}`, `{id}`, and `{reply}` — the file a drafted answer belongs
in, named absolutely ([§8.12](#812-an-answer-comes-back-as-a-proposal)). An
unknown name is left as written, so a typo is visible in the ticket instead of
becoming a blank.

### 8.4 Where work goes, and what runs it

`work.root` — default `{workspace}/panta` — is a rhei project directory in the
item's checkout, one plan per item, named for the item.

`work.runner` is what runs a plan there. It comes bound: unset, it is the
runtime ephor ships wired and ready, and naming another is how somebody who
works differently points work at theirs
([§4.2.0](#420-pointing-work-at-a-different-runtime),
[§DA-001-runtime-bound-default](decisions/architectural/DA-001-runtime-bound-default.md#da-001-runtime-bound-default-the-runtime-is-a-bound-default-not-a-named-coupling)).
`ephor work run` invokes it as a summons from the checkout the work is about —
`<runner> run <work root> --rhei <plan>… [--agent <agent> [--agent-mode
<effort>]]` — and reads its exit code the one way (`75` is parked, not failed).
The agent flags appear only when a chosen hand could not be written on the
tickets themselves; see the hands paragraphs below. With nothing on `PATH` under that name, every
part of dispatch except the running still holds: tickets are written, read and
reopened, and only running refuses, naming the runner it looked for
([§7.5](#75-why-something-is-not-offered)).

**Who does which action** — `work.hands` maps an action's id to the hand that
does it, with `default` answering for every id it does not name
([§FS-006-project-interface.9](../requirements.md#9-offers-the-projects-actions)).
A hand id is one of the names the roster prints
([§4.3.2](#432-what-a-project-can-do--capabilities)), optionally at one of the
efforts it declares. Every printed id is unique: where a model profile in the
runtime's settings claims an agent's very name, the profile holds it and the
agent standing alone is listed as `@<agent>`:

```jsonc
{
  "work": {
    "hands": {
      "default": "sonnet",
      "rebase": "luna:high",
      "fix-gate": { "agent": "our-agent", "model": "our-proxy-model" }
    }
  },
  "projects": {
    "widget": {
      "work": {
        "hands": { "default": "review-deep" },
        "permitted_hands": ["review-deep", "sonnet"]
      }
    }
  }
}
```

Seven steps answer *who does this*, each displacing the ones under it: what
you picked for this dispatch alone, the `hand` the action or recipe carries,
this project's entry for the action id, this project's `default`, the site's
entry for the action id, the site's `default`, and — where nobody named
anyone — whatever the runtime picks unasked. That is the runtime's own
resolution order mirrored deliberately, so the two cannot come to disagree
about one configuration.

**Picking for one dispatch.** The first step has two spellings of one
operation: `t` on a menu entry that hands work over (§7.6), and `--hand
<hand>[:<effort>]` on `ephor work dispatch` and on `ephor rebase --dispatch`
(§8.11). Either displaces every table for exactly that dispatch and is
remembered by nothing — the next dispatch of the same action resolves from
the tables again. A pick outside the project's `permitted_hands` is refused
like any other named choice.

The long form `{ "agent", "model", "effort" }` is for a pair the runtime's
registry never listed — a proxy serving a model it does not know about. It is
accepted **with a note** rather than refused, because ephor cannot prove such a
pair invalid. A name the roster does have is checked against it: a typo, or an
effort the hand does not declare, is refused before anything is written.

**Naming a hand without an effort** is settled by what the hand declares. A
hand declaring no efforts is asked plainly — there is nothing an ask could
drop. One declaring exactly one is asked at it, and dispatch says so in a
note: a single declared effort is a fact about the hand, not a choice left
open. One declaring several is refused with the list — name one, as
`<hand>:<effort>` — because the runtime's two spellings disagree about what
an effort-less ask would mean: a per-ticket selector without a mode runs
without one, silently, while a bare per-run `--agent` flag lets the state
machine's own mode fall in and fails the run outright where the agent does
not declare that mode. Neither is what you chose, so ephor never emits
either.

**A hand that names an agent and no model of its own** — which is every hand
on a machine whose runtime settings declare no model profiles — has no line
in the plan language, and the cheapest fix is not ephor's at all: **declare a
model profile in the runtime's own settings**

```jsonc
// ~/.config/rhei/settings.json
{
  "models": {
    "luna": { "provider": "openai", "model": "gpt-5", "default_agent": "pi" }
  }
}
```

and the agent-only hand becomes a model hand: `"rebase": "luna:high"` now
writes the runtime's own target line onto each ticket it dispatches — per
ticket, with no ephor machinery involved beyond what dispatch already does.
If you stop reading here, that is the whole fix, and the better one.

Without a model profile the hand still binds
([§FS-005-dispatch.14](../requirements.md#14-who-does-the-work-is-chosen-and-defaulted-per-project)):
the ticket pins nothing — dispatch says so in a note — and `ephor work run`
carries the choice as the runtime's own `--agent` / `--agent-mode` flags,
resolved when the run is invoked, which is the same moment the runtime reads
its own settings. The two per-ticket lines rank differently against those
flags, and ephor follows the runtime exactly. A ticket carrying a full
`**Target:**` line cannot be re-aimed — the runtime resolves it from the
line alone, and the run's agent flags are invisible to it — so such a ticket
rides beside a flagged run untouched. A ticket carrying `**Model:**` alone
can be: the flags would supply its carrier, and one run advances several
tickets. So the flags ride a run only when every open, unclaimed ticket
without a line of its own resolves to the same hand and none pins a bare
model: a plan whose tickets disagree runs unflagged, and `work run` says the
hand went unbound for that run. The inbox's run key
([§8.7](#87-the-work-screen)) binds the same way: it
resolves the hand exactly as `work run` does, over the one plan it runs.

`permitted_hands` narrows a project to the hands that may work on it at all,
which is what a repository under a policy about which models may see its code
needs. Anything outside the list is refused with that reason wherever it was
named — the tables, the action's pin, your own choice at the moment of asking
— never silently dropped. What a narrowing cannot bind is the runtime's unasked
pick, so a project that narrows and names no `default` is told that much.

With no table anywhere, nobody is named and the runtime picks exactly as it
does today. With nothing on `PATH` under `work.runner` there is no roster to
name a hand from: a configured hand resolves to nothing, says so in the
workable rung's own words, and the ticket is written all the same.

ephor creates it when it is missing: the manifest, a `.gitignore` that ignores
the directory itself so your repository stays clean, and the shipped state
machine as `states.yaml`. **An existing `states.yaml` is never replaced** —
edit it for a different agent, model or timeout. A project that already holds
plans of its own and declares no machine is refused rather than filled in,
since a state machine governs every plan in a project; `ephor work states`
prints ephor's for installing deliberately.

The shipped machine is two agent passes:

```
fix ──► review ──► done
```

- **`fix`** does what the ticket asks, in that checkout, and writes a report:
  what changed and why, what was run and what it said, what could not be done.
  It commits locally and touches nothing outward.
- **`review`** reads that report against the ticket and the actual diff, fixes
  what is small and wrong, and writes a verdict whose first line is
  `VERDICT: done | partial | blocked`.
- ephor reads that line back onto the row.

`review` requires the report `fix` writes, so work that produced nothing stops
at the boundary instead of arriving at `done` with an empty result.

### 8.5 A script in front of the agent

The most useful thing to put in front of an agent working a failing gate is
usually what a *script* can fetch — the log, the failing job, the forge's
analysis. A state machine can run one before the agent: rhei calls it a
**program state**, and it needs no agent at all.

For that, the script has to be told which item it is about, and prose is not an
input. So every ticket also carries the item as structured metadata
([§FS-005-dispatch.8](../requirements.md#8-the-ticket-carries-the-item-as-data-not-only-as-prose)),
under the same names a shell action gets in its environment:

```markdown
# Rhei: #24407 Fix condition metadata checks
**States:** ephor-work

---
metadata:
  tasks:
    fix-gate-1:
      project: "widget"
      source: "forge"
      kind: "pr"
      id: "forge:widget/24407"
      url: "https://forge.example/widget/pull/24407"
      state: "open"
      repo: "widget"
      number: "24407"
      branch: "you/ABC-42-retry"
      workspace: "/home/you/c/widget/you/ABC-42-retry"
      root: "/home/you/c/widget"
      title: "…"
---
```

A state reads those as `{meta.<key>}`, names its output with
`{output.<name>.path}`, and the next state declares the same file as an
`input` — which is how a script hands its answer to an agent:

```yaml
  collect:
    program:
      command: "~/.config/ephor/ci-failures.sh"
      env:
        PROJECT: "{meta.project}"
        SOURCE: "{meta.source}"
        ITEM: "{meta.id}"
        REPO: "{meta.repo}"
        NUMBER: "{meta.number}"
        CHECKOUT: "{meta.workspace}"
        REPORT: "{output.failures.path}"
    poll: { interval: 10m, max_attempts: 36 }
    outputs:
      - name: failures
        path: "runtime/ephor/{task_id}.failures.md"

  fix:
    agent: claude-code
    inputs:
      - name: failures
        path: "runtime/ephor/{task_id}.failures.md"
    instructions: |
      What failed is in {input.failures.path} — read that first.
```

**The exit code picks the next state**, which is what makes a script a decision
and not just a fetch:

| Exit | `config/ci-failures.example.sh` means | Goes to |
|---|---|---|
| `0` | the failures are in the report | `fix` |
| `3` | the gate is green; nothing to fix | `gate-green` (final) |
| `75` | the gate is still running | `collect` again, after `poll.interval` |
| other | the forge could not say; the report says why | `fix` anyway |

Order the transitions so the specific codes lead — the first match wins — and
give the polling state a `poll:` block: `rhei run` then releases its slot
between attempts, so a gate that takes three hours does not hold a worker.

Two things to know about programs, both learned the hard way:

- A program's working directory is the **plan directory**, not the checkout,
  and `RHEI_CHECKOUT_ROOT` is not exported to it. Pass the checkout explicitly
  as `{meta.workspace}` rather than assuming a layout.
- The *agent's* working directory is the checkout, which the runtime finds as
  the git repository enclosing the plan — so for a **multi-repo workspace**,
  whose root holds several repositories and may not be one itself, ephor runs
  the runtime **from the checkout it recorded** rather than leaving it to that
  lookup. The root of a multi-repo project is the directory its repositories
  sit in, and that is where the work runs.
- Artifact paths resolve relative to the plan directory too, so a script that
  writes to `$REPORT` as given lands exactly where the next state's `input`
  expects it.

**Waiting for every subtask, then landing.** A super-task cannot wait for its
subtasks in the runtime's language: `**Prior:**` points from a task to its
prerequisites, so the edge runs the wrong way, and a task in a terminal state
may not hold non-terminal descendants. The join is a **peer** whose
`**Prior:**` names every subtask — it becomes ready exactly when all of them
have finished — and the plan itself is the super-task.

What "finished" means turns on one rule worth knowing: a prerequisite in **any**
final state satisfies its edge *except* one whose state is named `cancelled`.
So a machine that spells its give-up state `gave-up` will let the join fire
over work that was abandoned; spelling it `cancelled` is what makes "wait for
all of them" mean *all of them succeeded*. The runtime then says so plainly —
`Task land-1 waiting on Task cause-2 (cancelled)`.

Write the join with a program rather than an agent: the `**Prior:**` list has
to be complete and spelled right, which is not a thing to improvise.
`config/plan-join.example.py` does it, and copies the item's metadata onto the
ticket it writes, since `{meta.*}` belongs to the ticket ephor dispatched.

**A question only a person can answer.** An agent that meets a product
decision, or a trade-off it cannot weigh, should stop rather than guess. Give
it a state the runtime will not leave on its own —

```yaml
  needs-human:
    description: A question only a person can answer; the ticket waits here.
    gating: true
```

— and let it route itself there: the agent writes `NEEDS-HUMAN: <question>` as
the first line of its artifact, and the program that reads that artifact exits
`2` for it. `rhei run` then finishes everything else in the plan and stops at
this ticket ([§FS-005-dispatch.9](../requirements.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).

ephor reads `gating` out of the machine, so the item's row says
`⚠ fix-gate · waiting on you · cause-2` ahead of anything else, and the work
screen prints the question and the command that resumes it. Answer **in the
plan** — `e` opens it in your editor — then:

```bash
rhei transition <ticket> --from needs-human --to propose
```

The question and its answer stay next to each other in the file that is the
record of this item's work, where the next round can read them.

**A failure that was never the change's fault.** A runner dies, a mirror is
unreachable, the same flake lands again — and what the item needs is not a fix
but another run
([§FS-005-dispatch.11](../requirements.md#11-a-failure-that-is-not-the-changes-fault-is-restarted-not-fixed)).
A loop that cannot make that move pays for it twice: a model diagnoses
something that was never wrong, and a commit gets landed whose only purpose was
to make the gate start again.

So restarting is its own state, its own ticket, and its own marker. Whichever
state notices — triage while classifying, or `analyze` and `critique` through a
`NOT-OURS:` first line that `route.example.sh` exits `3` for — **opens a ticket
in `restart-gate` and leaves the list of what to restart beside it**. The list
is a declared input, so the state is handed what to do rather than working it
out:

```yaml
  restart-gate:
    program:
      command: "~/.config/ephor/restart-gate.sh"
      env:
        JOBS: "{input.jobs.path}"
        GATE_RESTART: 'acme-cli gate review --platform bitbucket
                         --project ACME --repo "$EPHOR_GATE_REPO"
                         --pr "$EPHOR_GATE_NUMBER"'
        MAX_RESTARTS: "2"
    inputs:
      - name: jobs
        optional: true
        path: "runtime/ephor/{task_id_local}.jobs.md"
```

Four things are worth knowing about that shape:

- **The path is `{task_id_local}`, not `{task_id}`.** Whoever decided the
  failure was not the change's fault also authored the ticket, so it knows the
  local id — the qualification prefix is applied at load time and may not be
  authored.
- **The format is exact**, because a program that guesses at a job name
  restarts the wrong thing. One job per line, two fields: `<repo> <job-key>`,
  or `<repo> -` for that repository's whole gate. Anything else stops the
  restart and names the line.
- **The list is `optional`** so that a ticket arriving without one is refused
  by the program, which can say what is missing, rather than by an input check
  that can only decline to move.
- **It restarts downstream too.** A gate spanning several repositories fails
  downward, so after the named jobs every repository the forge reports as not
  green gets its whole gate restarted. One that is green again by the time the
  state runs is left alone — somebody already re-ran it, and restarting it
  would re-red a passing gate.

`restarted` is a *successful* final state, so it satisfies the join and `land`
runs, finds nothing to push, and re-enters `collect` to wait out the run the
restart started. The budget is counted **out of the plan** rather than with
`visits` — every round opens a new ticket and a per-state counter resets with
it — and past `MAX_RESTARTS` the ticket parks for a person, because at that
point the infrastructure is the thing that is wrong.

`config/ci-failures.example.sh` and
`config/ephor-work-ci.example.states.yaml` are a working pair: the script
refreshes the project, reads the gate out of `ephor feed --json`, asks
`ephor failures` when something failed, and exits with one of the codes above.
Copy both, point `work.states` at the machine and `program.command` at the
script, and give the recipe `"state": "collect"` so tickets start there.
`config/ci-green.example.states.yaml` is the fuller machine, with triage,
review and restart wired together.

### 8.6 A moved item reopens its own work

ephor fingerprints an item when it dispatches — last activity, state, gate, how
much conversation. When any of that changes, the work is **stale**, and
`ephor work sync` appends a ticket to the *same* plan saying what changed,
ordered after the last one:

```markdown
### Task answer-1: answer the conversation — #17 …
**State:** fix
**Prior:** Task fix-gate-1

Since the previous ticket: 1 new message; the jobs pass now, but the forge
still refuses the merge. The item above has been rewritten to what it is now.
```

What is asked for is chosen against the item **as it is now**, preferring what
was asked last while that still applies: a pull request whose gate went green
and whose reviewer asked a question is no longer a red gate. Where nothing
applies any more — it merged, it closed — the work is not reopened, and the
ledger goes on saying the item moved past it.

### 8.7 The work screen

`w` on any item:

```
 ephor — work — #17 Humanize durations in the log reader
  the plan
    /home/you/c/demo/panta/forge-demo-17.rhei.md

  what has been asked for
    ✓ fix-gate-1    fix the red gate  [done]
        done — the ticket is answered and the change is right
    ⚙ answer-1      answer the conversation  [fix]

  ⟳ since that was asked: 1 new message
    s reopens it with what changed

  what can be asked for
  ▸ 1 💬 answer the conversation

      #17 … is waiting on an answer from me.
      The conversation is above, last message last. …
```

| Key | Does |
|---|---|
| `1`-`9`, `Enter` | open work under that recipe |
| `a` | ask for something no recipe covers |
| `s` | reopen work whose item has moved |
| `c` | take a ticket back (§8.7.1) |
| `R` | hand **this item's plan** to the runtime |
| `e` | read the plan in `$EDITOR` |
| `L` | read the newest job ephor ran here (§8.14), where it ran one |
| `o` | open the item in the browser |
| `j` `k` | move between recipes · `f`/`b` page · `;` ops · `Esc` back |

The recipe rows show the words each would actually send, rendered against this
item — dispatching is cheap to press and expensive to run — and the hand each
would go to, resolved the way dispatch will resolve it (§8.4), in the same
sentence the action menu's entries carry. To pick a different hand for one
dispatch, use the menu's picker (`x`, then `t` — §7.6) or `--hand` on the
command line.

`R` leaves the interface entirely: the runtime's own dashboard takes the
terminal while it works, and coming back re-reads the plans. With no runtime
bound it is not offered at all — the footer drops it, and pressing it says why
rather than handing the terminal to a command that cannot start. Everything
else on the screen is unchanged: the ticket is written, read, reopened and
edited whether or not anything can run it.

It is the same run `ephor work run` starts, over this one plan: the hand is
resolved before the terminal is ceded and rides the run as the runtime's own
agent flags where it names an agent and no model ([§8.4](#84-where-work-goes-and-what-runs-it)).
The header line above the run names who is getting it — `rhei run — <item> ·
agent pi at high` — and anything the resolution had to say, such as a hand
that went unbound, waits for you on the message line when the run returns
rather than scrolling away with it.

#### 8.7.1 Taking a ticket back

The same recipe pressed twice, an ask on the wrong item, a question the item
moved past — not every ticket should run to its end. `c` on the work screen
takes one back
([§FS-005-dispatch.16](../requirements.md#16-work-that-should-not-go-on-is-cancelled-and-the-plan-says-so)):
the open tickets are numbered, `j`/`k` or a digit picks one, and a one-line
prompt asks why — the reason becomes the ticket's result, `Enter` on an empty
line records that no reason was given, `Esc` keeps the ticket.

```
  what has been asked for   — which one to take back?
  ▸ 1 ⚙ fix-gate-1    fix the red gate  [collect]
    2 ⚙ fix-gate-2    fix the red gate  [collect]

┌ cancel fix-gate-2 — why? ────────────────────────────────────────┐
│ asked twice by mistake▌                                           │
│ the reason becomes the ticket's result · enter cancels it · esc keeps it │
└──────────────────────────────────────────────────────────────────┘
```

Afterwards the ticket reads `⊘ fix-gate-2 … [cancelled]` with the reason
beneath it, and the row's badge says `⊘ fix-gate · cancelled` where that is
the last word on the item. **Nothing is deleted** — the plan is the record,
and taking an ask back is a decision it keeps.

Cancelling is **the runtime's move, in its own words**: ephor asks the bound
runner for the transition into `cancelled` — `rhei transition <plan> --task
<ticket> --from <state> --to cancelled --result "<why>"` — captured, and
tells you what it answered. It never rewrites a `**State:**` line by hand,
because the plan language reserves a ticket's state to the runtime's verbs
once the ticket is written (the compare-and-swap, the artifact checks, the
callbacks, the audit trail;
[§DA-005-cancel-is-the-runtimes-move](decisions/architectural/DA-005-cancel-is-the-runtimes-move.md)).
So with no runtime bound `c` is not offered and says why, exactly as `R` does;
the plan stays hand-editable for anyone who wants to make the move themselves.

Three things are refused before the runtime is asked, in one sentence each:
a ticket **a live run holds** — the run's to finish; wait for it or stop it
where it is running — a ticket that is **already over**, and a machine that
**declares no final `cancelled` state** (ephor's shipped machine and the
examples beside it declare one and a `from: "*"` transition into it; a
machine of your own must too — `ephor work states` prints ephor's to copy
from). What the runner itself refuses comes back in its own sentence.

Order follows from the state's name. `cancelled` is the one final state that
satisfies no `**Prior:**` — which is why the join in
[§8.5](#85-a-script-in-front-of-the-agent) relies on it — so a ticket
ordered after one you cancel will not start, and the message names it:
`⊘ fix-gate-1 cancelled — fix-gate-2 is ordered after it and will not start
while it stands cancelled`. Cancel that one too if you mean to; ephor does
not decide for it. The next ticket ephor writes — a reopen, an ask — is
ordered after the last one *not* cancelled, so ephor's own chain never hangs
off abandoned work.

From the shell:

```bash
ephor work cancel --item github-prs:acme/widget#42 fix-gate-2 --why "asked twice"
ephor work cancel --item ID fix-gate-1 fix-gate-2          # several at once, each reported
ephor work cancel --item ID fix-gate-2 --dry-run           # what would be cancelled, nothing moved
```

### 8.8 By hand

Recipes are for the work that repeats; most work does not. `a` on the work
screen takes one line and makes an ordinary ticket of it — same dossier, same
plan, same order, your words as the brief:

```
┌ ask for something ───────────────────────────────────────────────┐
│ bump the retry timeout to 30s and re-run the flaky test▌          │
│ becomes a ticket with the dossier · enter opens it · esc cancels  │
└──────────────────────────────────────────────────────────────────┘
```

An ask is **refused for nothing but being unrunnable**. Selectors say what
ephor volunteers, not what you may ask for: a merged pull request, an item no
recipe covers, a second ask on work already running are all fair.

From the shell:

```bash
ephor work ask --item github-prs:acme/widget#42 "bump the retry timeout to 30s"
ephor work ask --item github-prs:acme/widget#42 < ask.md      # composed in an editor
ephor work ask --item ID --state review "…"                   # start elsewhere in the machine
```

### 8.9 Commands

```bash
ephor work                                  # = work list
ephor work list [--project P] [--open] [--json]
ephor work dispatch [--project P] [--item ID] [--recipe R] [--kind K]
                    [--again] [--hand H] [--updated-within DAYS] [--dry-run]
ephor work ask --item ID [WORDS…] [--state S] [--dry-run]
ephor work sync [--project P] [--dry-run]
ephor work cancel --item ID TICKET… [--why WORDS] [--dry-run]
ephor work run [--project P] [--item ID] [--watch] [-- RHEI_ARGS…]
ephor work workflows [--project P] [WORKFLOW] [--json]
ephor work lay ENTRY --item ID [--set INPUT=VALUE]… [--hand H] [--dry-run]
ephor work forget [--item ID | --done | --missing]
ephor work states
```

- **`list`** reads each ticket's state out of its plan every time. The ledger
  never caches it — a watch reporting on itself is the one thing this must not
  do. `--open` hides work that is finished and current.
- **`dispatch`** is the sweep: every item that matches a recipe and has no work
  yet. It takes the *first* matching recipe unless `--recipe` names one. It
  skips items that already have work — naming `--recipe` asks for that work
  specifically and lands as another ticket; `--again` overrides the skip
  entirely. `--hand <hand>[:<effort>]` is your pick of who does it, for this
  invocation alone (§8.4).
- **`run`** groups by work root and names the plans ephor opened, so a runtime
  project you keep in the same checkout for your own work is not swept in. One
  root at a time: tickets in one root are about one checkout, and two agents in
  one working tree edit the same files. Pass runtime flags after `--`.
  It **starts the run detached** and prints the id it was given
  ([§FS-005-dispatch.20](../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching));
  `--watch` keeps your terminal and watches the run here, which is also what a
  runner with no detached shape does unasked, saying so.
- **`workflows`** and **`lay`** are the runtime's own workflows, offered as
  actions (§8.15). `lay` writes a plan of its own beside the matter's and runs
  nothing; `--dry-run` shows what would answer every input first.
- **`forget`** drops ledger entries only. The plans stay on disk: they are the
  record of what was done.

### 8.10 The ledger

`~/.local/state/ephor/work.json`, keyed by item id — the same key unread
tracking uses:

```json
{ "version": 1,
  "entries": {
    "github-prs:acme/widget#42": {
      "project": "widget",
      "title": "Retry window",
      "root": "/home/you/c/acme/widget/panta",
      "rhei": "github-prs-acme-widget-42",
      "plan": "/home/you/c/acme/widget/panta/github-prs-acme-widget-42.rhei.md",
      "dispatches": [
        { "ticket": "fix-gate-1", "recipe": "fix-gate", "at": "2026-08-12T06:10:00Z",
          "snapshot": { "updated_at": "…", "state": "open", "passed": 12,
                        "failed": 2, "running": 0, "blocked": false, "messages": 3 } },
        { "ticket": "", "recipe": "review-change", "at": "2026-08-12T09:31:00Z",
          "plan": "github-prs-acme-widget-42-review-change",
          "snapshot": { "…": "…" } }
      ]
    }
  } }
```

A dispatch carrying a `plan` laid a workflow down beside the matter's own
plan rather than a ticket inside it (§8.15); its `ticket` is empty because
there is none.

It answers one question — has this been handed over? — and holds the
fingerprint that answers the next: has the item moved since. An entry whose
plan has been deleted is reported as missing, never repaired.

### 8.11 Rebasing a branch that has fallen behind

A rebase is two git commands and a question. The commands are the same every
time and a model is not needed to type them, so ephor runs them first and hands
over only the question ([§FS-005-dispatch.12](../requirements.md#12-work-an-algorithm-can-finish-does-not-start-with-a-model)).

```bash
ephor rebase                                  # the working directory
ephor rebase --project widget --checkout ~/c/widget/you/ABC-42-retry
ephor rebase --onto release/24 --checkout .   # some other base
ephor rebase --upstream --checkout .          # onto the branch's published copy
ephor rebase --item forge:widget/42 --dispatch    # and open a ticket on conflict
ephor rebase --item forge:widget/42 --dispatch --hand luna:high   # …to that hand
```

It fetches and replays **every repository in the checkout** — the project
type's `repos` where the registry names them, otherwise every git working tree
directly under it — onto the project's `main_branch`, and reports per
repository. Nothing is stashed: a repository with uncommitted work is named and
left alone. A declared repository whose working tree is not on disk is named
too — in the summary and per repository in the report — and gates nothing:
retrying the rebase will never replay a tree that is not there, the missing one
holds none of your change, and the condition was as true before you ran it as
after. It is a fact about the checkout rather than an outcome of the run, and
the command whose exit code answers for it is `ephor checkout`, which completes
a workspace that is missing repositories. Nothing is pushed either; a replayed
branch cannot fast-forward,
and forcing is a decision that belongs to a state that says so
([§8.5](#85-a-script-in-front-of-the-agent)).

**`--upstream`** replays onto the other ref: each branch's own published copy,
resolved per repository from its `HEAD`
([§FS-004-quick-actions.8](../requirements.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it)).
That is a different ref in every repository, so it takes no branch name and
excludes `--onto`. It is what answers the checkout a poly-repo workspace
actually leaves behind: a branch grown with `git worktree add -b`, pushed, and
carrying no tracking configuration — where bare `git rebase` refuses to start
at all. A repository that has published nothing is reported as *nothing
published* and the run still succeeds; there was simply nothing to replay onto.
Replaying onto your own copy rewrites commits that copy already has, so landing
it needs the same leased force push the rebase onto main does.

| Exit | Means | The machine sends it to |
|---|---|---|
| `0` | every repository that is here is on the base — replayed, already there, or with nothing published; a declared one that is not here is named | land it |
| `3` | one stopped in a conflict, left mid-rebase with the files named | an agent |
| `1` | uncommitted work, no repository, or git refused | a person |

Each argument can arrive as an environment variable instead — `CHECKOUT`,
`PROJECT`, `ONTO`, `UPSTREAM` (set to any non-empty value), `ITEM`, `HAND`,
`REPORT` — which is how a program state passes it `{meta.*}`. The refusal of
`--upstream` with `--onto` holds in these spellings too: the flag parser
cannot see the environment, and silently preferring one would run a different
rebase than the state asked for. `config/ci-green.example.states.yaml` wires
the whole path:

```
rebase ──0──► land-rebase ──► rebased        (FORCE_WITH_LEASE=1 on land.sh)
   └───3───► resolve-conflicts ──► verify-rebase ──► land-rebase
   └─nonzero─► needs-human
```

`land-rebase` is a landing state of its own precisely because it forces:
`land` proper never does, and a replayed branch is the one case where the push
has to rewrite what the remote already has. It is `--force-with-lease`, so a
remote that moved under you refuses the push instead of overwriting somebody
else's commits.

The `⤴ rebase onto …` entries in the action menu run exactly this command,
and run it as a job: the interface stays where it is and the replay is watched
from its row ([§8.14](#814-jobs--what-ephor-runs-beneath-the-screen)).

Give the `rebase` recipe `"state": "rebase"` in your `status.json` for tickets
to start there; with the shipped two-state machine they start in `fix`, where
the brief tells the agent to run `ephor rebase` itself and resolve what it
stops on.

### 8.12 An answer comes back as a proposal

Often the next move on a matter is not a change but a reply. The shipped
`answer` recipe asks for one, and it needs no checkout: the plan is written at
the branch workspace where one resolves and at the forest root where none
does, so a conversation is answerable on a project whose branch is not on this
machine ([§FS-005-dispatch.13](../requirements.md#13-a-communication-is-work-too-and-its-answer-comes-back-as-a-proposal)).

The reply is asked for as a file of its own — `{reply}` in a brief expands to
it, `<work root>/runtime/ephor/<plan>.reply.md` — and **nothing posts it**. The
run writes it, ephor reads it back, and the thread screen shows it under the
conversation it answers:

```
▍ Ada  2h ago
▍ does the retry window reset between attempts?

▍ proposed reply — not posted
▍ Yes — it resets per attempt; the test in retry_test.rs covers it.
▍ p posts it · e edits it first · /home/you/c/demo/panta/runtime/ephor/…reply.md
```

`e` opens it in `$EDITOR` — what you leave there is what goes out, and leaving
it empty withdraws it. `p` posts it through the same provider a reaction goes
through, and then the card says `posted` and the key stops being offered: the
file is moved aside so the same words cannot go out twice.

`p` appears only where the channel **said** it can carry a reply
([§FS-007-matters.4](../requirements.md#4-a-channel-says-what-it-can-do)) — a
forge declares the `replies` capability and puts a `reply` descriptor on the
threads that take one ([§10.1](#101-a-forge-out-of-process)). Where it does
not, the card is still there and names the file: the proposal is what you copy,
which is the offer narrowing rather than the feature failing.

### 8.13 The operations board

`;` from anywhere in the interface opens the board; `Esc` (or `;` again)
returns exactly where you were, and every screen's footer says so. The
exceptions are the things you are already inside — a prompt, an open action
menu, the thread screen's reaction picker — where `;` is a keystroke meant for
them. It is the answer to "what is ephor doing right now", in one place
([§FS-005-dispatch.15](../requirements.md#15-every-operation-is-visible-in-one-place)):

```
 ephor — operations
  beneath the reading
    ⟳ Refreshing graal (3/7)…

  operations
  ▸ ▶ demo · ~/c/demo/you/ABC-42-retry/panta   running · quiet 12m · dashboard (o)
        ⚠ acmeforge-app-101.answer-2  · Retry window on app 101  [needs-human]  waiting on you
        ⚙ acmeforge-app-101.fix-gate-1  · Retry window on app 101  [fix]  running
        ‖ acmeforge-app-101.answer-1  · Retry window on app 101  [fix]  queued
        ✓ 2 finished
    ✋ demo · ~/c/demo/panta   claimed, not scheduled
        ✋ forge-demo-17.fix-1  · Humanize durations  [fix]  claimed by luna — free it: rhei release forge-demo-17.fix-1
```

Within one operation the tickets read in order of urgency: what waits on you
first — it is the one part of the work nobody else will move
([§FS-005-dispatch.9](../requirements.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking))
— then what a dead run dropped, then what runs, then claims, then the queue.
Each ticket line carries the matter's own title beside the ids, so a row
means something before you open it. And a ticket is a ticket at any depth:
a subtask the runtime split off reads from the plan like its parent —
`widget-42.fix-gate-1.1` is a row of its own when it parks.

**A live row names its run.** A run publishes a descriptor beside its lock, so
the row carries the id it calls itself, `a` puts the runner's own surface on
it, and — where the runner has one — the row shows the command that *stops* it,
in the runner's own words. Shown, never run: a key that stopped a run would be
a channel to the run ephor promised never to hold
([§FS-005-dispatch.20](../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)).
`ephor operations --json` carries the same four facts — `run`, `control_url`,
`attach`, `stop`.

**Watch-only.** The board starts nothing and stops nothing — attaching is
reading, and everything that would change a run belongs to the run itself.
What ephor runs *itself* is started by the menu entry you pressed and merely
shown here ([§8.14](#814-jobs--what-ephor-runs-beneath-the-screen)); jobs list
first, above the runtime's roots, because a job is usually the thing you asked
for a moment ago:

```
  operations
  ▸ ▶ demo · rebase onto master (1582 behind as of Nov 21)   job · e reads it
        ⚙ replaying substratevm-enterprise-gcs onto origin/master
```

**Rows are execution roots, and liveness is the runtime's lock.** The runtime
holds a lock per execution root for exactly as long as a run is live there,
and the OS releases it if the run dies — so the board probes the lock (without
ever waiting on it) instead of guessing from output. A run deep in a long tool
call is legitimately silent: that is the `quiet` badge, never a death notice.
Because the lock is per root and ephor's work root is per branch workspace,
two items in one workspace are one operation, and a ticket in a root a run
already holds reads as **queued**.

**Claimed is not running.** An `**Assignee:**` on a ticket means somebody took
it and the runtime skips it. With no run live on its root, the board shows it
as *claimed, not scheduled*, with the runner's own release command beside it —
reported, not offered as a key.

**Work parked for you keeps its row.** The usual end of a run that parks a
ticket is the run exiting — nothing else was schedulable — so the lock goes
free and no assignee marks the spot: parking is a transition, not a taking.
The ticket stays on the board all the same, *waiting on you*, until you move
it ([§FS-005-dispatch.9](../requirements.md#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)).
A run that died mid-slot leaves a row too, but not that one: the journal it
left still names the ticket it was holding, and that ticket shows as
**dropped by a run that died** — a parked ticket is a question about the
work, a dropped one is a run that wants starting again, and conflating them
would send you reading a plan when what it needs is a fresh run. What a
dead run held is not trusted
blindly, either — a journal entry no run ever released stops counting the
moment the ticket's own state says it moved on, so a crashed run's ticket
never reads *running* under a run that came later. And a work root whose
`states.yaml` cannot be read is not guessed at: running, claimed, and
dropped still show — the lock, the journal, and the plans carry those on
their own — but nothing there is called queued or finished on the word of a
machine that is not there, and the row says so itself: `no states.yaml —
nothing judged queued or finished`.

| Key | Does |
|---|---|
| `j` `k` | move between operations (the view follows the selection) |
| `Enter` | the matter's thread — or the plan, where the operation has no matter — or, on a job, its log |
| `e` | read the plan in `$EDITOR`; on a job, its log in `$PAGER` |
| `a` | watch the run on this root by attaching to it (§8.17) |
| `o` | open the run's dashboard, where a live run published one |
| `r` | refresh underneath, exactly as everywhere else |
| `Esc` `;` `q` | back to where you were |

The refresh reports here **additionally** — the header line on whatever screen
you are reading keeps carrying it
([§FS-001-forge-interface.7](../requirements.md#7-a-fetch-runs-beneath-the-reading-never-in-front-of-it)).
With no runtime bound the board is the refresh row, ephor's own jobs, and the
workable rung's sentence, which is the shape most installations see — correct,
not broken. A job needs no runtime: it is ephor running a command.

The board, and every work badge in the feed, keeps itself current: between
key reads ephor glances at the plans and run artifacts it already knows by
name (a clock gates the stats, a changed timestamp gates the re-read), so a
ticket the runtime parks for you resurfaces when it parks
([§FS-005-dispatch.15.1](../requirements.md#151-the-board-keeps-itself-current))
— no refresh needed, and no forge asked anything. A run *starting* writes no
file that glance would notice — the OS takes its lock and that is the whole
event — so while the board is open the locks are probed too, and a root that
came alive gets its row. Rows arriving above the cursor do not move it: it
stays on the operation it was on, not on the line number that operation had.

**The rows are found by looking, never by remembering.** The board
enumerates the work roots themselves — the configured `work.root`, resolved
at each project's checkout and again in every branch workspace on disk — so
a plan written by hand, a project's own planning tickets, and a run somebody
started in another terminal on a root ephor never dispatched into all appear,
judged by the same artifacts as dispatched work
([§FS-005-dispatch.15](../requirements.md#15-every-operation-is-visible-in-one-place)).
An operation ephor never dispatched has no matter behind it, so `Enter`
opens its plan — titled in the plan's own words — instead of a thread.
The walk runs when the board is built (opened, or rebuilt because the
glance saw something move), never on the bare 2-second tick, and is bounded
by the registry's own places: between builds the tick stats only what the
last walk found.

### 8.14 Jobs — what ephor runs beneath the screen

Some menu entries need nobody watching. A replay across a poly-repo checkout
weeks behind its base is minutes of output that asks nothing and decides
nothing; handing it the whole interface costs you the watch and then asks for
a keypress to give the screen back. Those entries run as **jobs**
([§FS-005-dispatch.17](../requirements.md#17-a-move-that-needs-nobody-runs-beneath-the-screen)).

Press `⤴ rebase onto master` and the menu closes onto the row you were on,
with one line in the status bar:

```
⤴ rebase onto master (1582 behind as of Nov 21, 2025): started · ; to watch
```

**It is its own process, in its own process group.** Quitting ephor does not
take it down, and neither does closing the terminal — a move that needed
nobody watching does not suddenly need somebody staying. Start one, quit,
come back tomorrow: `ephor job list` still answers.

**Watch it from `;`.** A live job is a row on the operations board with the
last line of its log under it, so *still going* and *stuck* are not the same
word. `e` or `Enter` on the row opens the log in `$PAGER` — with `less` that
is `less +F`, following the job as it writes; `q` stops following, `F`
resumes.

**When it ends, it says so where you are.** Whatever screen you are reading,
the outcome lands in the status bar, and the row leaves the board — an inbox
of every finished thing is the pile the board exists to avoid. What the move
changed is picked up with it: the branch's distance from its base is
re-measured, and a conflict handed over is a ticket that was not there before.

**Afterwards it stays with the item.** The work screen (`w`) lists what ephor
ran there, with what each came to, and `L` reads the newest one's log.
Records are swept a week after they end.

```bash
ephor job list          # what is running, and what recently ran
ephor job list --live   # only what is running now
ephor job list --json   # the same, for a script
ephor job log <id>      # everything one job wrote, in order
```

A job is a directory under `~/.local/state/ephor/jobs/` — `job.json` (what it
is), `log` (what it wrote), `lock`, and `outcome.json` when it ends — and that
is the whole record. **Liveness is the lock**, probed and never waited on,
exactly as it is for a run: a job that died holds no lock and wrote no
outcome, and is reported as *died* rather than as running, because "it
started" is a claim about the past. Jobs are found by listing that directory,
so one started by another ephor — a second terminal, a session that has since
exited — is a row like any other.

**The chain travels with the job.** An entry that needs the branch workspace
runs its `checkout` as the job's first step, and the directory the checkout
was supposed to make is verified rather than trusted: if it did not appear,
the job ends there naming the step, and the action after it never runs.

**Your own entries keep the terminal unless they ask not to** — `lazygit`, an
editor, a pager are legitimate menu entries, and one of those started beneath
the screen is a program nobody can type into. Write `"background": true` on an
action or an offer that needs no reader (§7.2). ephor's own `⤴ rebase` entries
set it themselves. An entry that *is* such a program has a third place to run:
`"window": true` puts it in a window of your own, beside ephor rather than in
its place (§8.16).

### 8.15 Workflows the runtime offers

A recipe is one ticket in your own words. The runtime carries something
larger: **workflows** — parameterized plans that lay down tasks of their own,
under a state machine of their own, with fan-out, review loops and human gates
nobody had to write here. A workflow is an entry in the same action menu as
everything else.

Ask what there is:

```bash
ephor work workflows                    # what the runtime offers here
ephor work workflows changeset-review   # and what that one takes
```

The list is the runtime's, asked at the moment of asking rather than kept as a
copy — for the same reason the roster is (§8.4). With no runtime bound there
are no workflows, said in the *workable* rung's own words.

**An entry makes a workflow an action.** It is the same entry shape actions
and offers already use, with `workflow` where a `command` would be:

```json
{ "id": "review-change", "icon": "⌥", "description": "review this change",
  "workflow": "changeset-review",
  "when": { "kinds": ["pr"] },
  "requires_checkout": true,
  "inputs": { "change_ref": "{branch}" },
  "hands": ["smart_target"] }
```

Three places it may live, narrow beating broad:

| Where | Who it is for |
|---|---|
| `.ephor.json` beside the workflow itself | travels and versions with the workflow |
| the project's manifest, under `actions` | everyone working on this project |
| `actions` in your `status.json` | you, everywhere |

A workflow the runtime ships ranks with what ephor ships, one the project
keeps with the project's offers, one you keep with your own — the provenance
the menu already orders by.

**Answering the inputs.** Five steps, each displacing the ones after it:

1. what you answered for this laying alone — `--set <input>=<value>`, or the
   line you typed in the interface;
2. what the entry's `inputs` says;
3. ephor's answer for an input that names who does the work;
4. the workflow's own default;
5. nobody — a required input still unanswered, which is asked for rather than
   written with a hole in it.

A string is rendered with the matter's own fields, exactly as a brief is
(§8.3), plus `{dossier}` and `{item}` — the paths of two files ephor writes
beside the plan, one the dossier as prose and one the identifiers as JSON, for
a workflow whose first task wants to read them. Anything that is not a string
is passed on as it stands, so an input wanting a number, a flag or a list gets
one; strings nested inside a list or a record are rendered too.

**Who does it is your answer, not the workflow's.** Most of a workflow's
inputs are its agents, each defaulted to whatever model its author happened to
be running. An input the workflow declares an execution target for — or one
your entry lists under `hands` — resolves through the seven steps of §8.4
instead, so `work.hands`, your pick at the moment of asking and
`work.permitted_hands` all mean here what they mean everywhere. Configuration
names a hand by its id: `"inputs": { "review_targets": ["luna", "sol"] }` is
two hands, resolved and rendered into whatever the runtime spells them as. A
hand a narrowing does not permit is refused with that reason — the workflow's
own default included, since a default is a naming.

**Laying one down.**

```bash
ephor work lay review-change --item github-prs:acme/widget#42 --dry-run
ephor work lay review-change --item github-prs:acme/widget#42
ephor work lay venue-intake --item … --set conference='ECOOP 2027'
```

`--dry-run` prints what would answer every input and where each answer came
from, plus the runtime's own account of what it would render, and writes no
plan. Without it, what lands is a **plan of its own beside the matter's** —
`<matter>-<entry>` in the same work root, never a ticket inside the matter's
own plan. Which means the operations board (`;`) finds it by looking like
every other plan there, and a workflow and a ticket about one change queue
behind the root's one run rather than editing the same tree at once.

Laying one down writes files and nothing else. Running it is the move after,
from the board or with `ephor work run`. A second laying of the same entry is
`<matter>-<entry>-2`: two runs of one workflow about one item are two records,
not a correction of the first.

In the interface, a workflow entry is an ordinary row in the action menu. One
missing scalar input is one line typed where you are standing; anything more —
or anything wanting a list or a record — opens a file in `$EDITOR` with
everything ephor already resolved in it and each unanswered input named with
what it wants. Leave that file alone and nothing is laid down.

### 8.16 A window of your own

ephor has one terminal and is sitting in it. Handing it over works everywhere
and stays the floor. But if you are inside a multiplexer, or in a terminal that
opens windows on request, there is a better move: the program in a window of
its own, ephor still on screen, and *open* from then on meaning **bring that
window forward**
([§FS-005-dispatch.22](../requirements.md#22-a-window-of-the-readers-own-where-one-is-bound)).

The window is a **seam**, with the anatomy every seam has: two commands, one
that opens a window running a given command and prints a handle for the window
it made, one that brings a handle forward. Which one fills it is `window` under
`defaults` in `status.json`:

```jsonc
{
  "defaults": {
    "window": "tmux"        // or "wezterm", or "kitty"
  }
}
```

```jsonc
{
  "defaults": {
    "window": {             // or a pair of your own — anything that can do the two verbs
      "open":  "my-terminal --new-window --title {title} -- {command}",
      "focus": "my-terminal --raise {handle}"
    }
  }
}
```

`{title}` and `{command}` are filled in the open template, `{handle}` in the
focus one, each quoted as one shell word; every other brace is left alone, so a
product's own format language survives. `open` must print the handle on
standard output and exit **when the window exists**, not when the program in it
ends.

**Unset, ephor recognizes where it is running.** Each of the three shipped
bindings sets a variable for exactly this purpose (`$TMUX`, `$WEZTERM_PANE`,
`$KITTY_WINDOW_ID`), and ephor reads it — it never spawns one of them to find
out. Where nothing is bound and nothing is recognized there is no window, and
the terminal is handed over as it always was, with the outcome line saying so.

**An entry may ask for a window.** Write `"window": true` on an action or an
offer whose program is something you type into — an editor, a pager, a coding
agent's own session (§7.2). It then runs as a job whose supervisor lives inside
that window: it holds a lock like any job (§8.14), its record keeps the
handle, and the window is its inspection where a log would have been, because
what it writes is on that screen and nowhere else. That is what makes an agent
started from the menu a row that says *running* and opens to the agent, rather
than a program ephor handed the terminal to and forgot.

**ephor never closes a window and never ends what is in it.** It opens one and
brings one forward. A window you closed is a job that ended, and the lock says
so without being asked.

### 8.17 What is already going, where it could be started again

The menu says what can be done about a row; the board says what is being done.
Kept apart they forget each other — you open the menu on an item whose rebase
is already replaying, press it, and either get refused by a lock or start a
second one. So **every entry that has work going about its subject is marked
running, and set apart**
([§FS-005-dispatch.21](../requirements.md#21-what-is-already-going-is-shown-where-it-could-be-started-again)):

```
 actions — Widen the retry window
  running
 1   ▶ ⚙  fix the red gate       12m · acmeforge-app-101.fix-gate-1 [fix]
 2   ⤴  rebase onto master (3 behind as of Jul 28)
 3   ✎  leave a note about it
```

The running rows stand first, under a line that says so, a step further in, in
one colour used for nothing else on that screen. Each says how long it has been
going and what it is at right now — the job's own last line, the ticket a run
holds and the state it is in, *queued* where the root's run will reach it.

**What counts as going is found by looking.** A command entry is running where
a job started from that entry, about this subject, still holds its lock — which
is why a job records which entry it came from and, on a branch row, which
branch. An entry that hands work over is running where the ticket it would open
is open and its root is live. An entry whose program runs in a window is
running while that window holds it. Nothing is remembered from the keypress: a
second ephor sees the same rows, and a job that died is not running whatever
started it.

**Pressing a running entry opens it; it never starts it again.** `Enter` (or
`l`) on such a row goes to the thing that is running — a job's log, followed as
it writes; a run of the runtime, attached; a program in its own window, brought
forward — and the footer says *open* rather than *run*. A second copy is not
what you meant, and where you do mean it the command line starts it and the
refusal is the lock's own sentence.

**Both surfaces say it.** `ephor actions [--json]` carries the same mark with
the same facts — what is running, since when, what it is at, and **the way in**:
a job's log path, a run's id with the runner's own attach command and its
control address, a window's handle. And `ephor actions open <id>` is that key as
a command:

```bash
ephor actions open rebase --item "$id"    # follows the job's log
ephor actions open fix-gate --item "$id"  # attaches to the run holding it
ephor actions open edit --item "$id"      # brings its window forward
```

It starts nothing. Where the entry has nothing going it refuses by name and
tells you which command would start it.

**Watching a run is attaching to it.** A run of the runtime starts detached —
`R` on the work screen, and `ephor work run`, both print one line saying the run
began and what it is called — and what watches it afterwards is the runner's own
surface, opened on the run
([§FS-005-dispatch.20](../requirements.md#20-a-run-of-the-runtime-starts-beneath-the-screen-and-is-watched-by-attaching)).
Leaving that surface **detaches and never stops the run**: the reflex that ends
a foreground command must not end a run another screen may also be watching.
Stopping stays out of the screen — the row carries the runner's own stop
command, shown and never run. Where the runner has no detached shape, the run is
watched as it always was, the terminal handed over, and the line says so.

---

## 9. Automation

### 9.1 Timers

```bash
mkdir -p ~/.config/systemd/user
ln -sf ~/f/ephor/systemd/ephor-refresh.{service,timer}   ~/.config/systemd/user/
ln -sf ~/f/ephor/systemd/ephor-work-sync.{service,timer} ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now ephor-refresh.timer
systemctl --user enable --now ephor-work-sync.timer
```

`ephor-refresh` fetches every ten minutes. `ephor-work-sync` refreshes and then
reopens everything whose item has moved, half-hourly — it writes tickets and
runs nothing, because spawning agents stays something you ask for.

### 9.2 A shell prompt

```bash
ephor status --check          # exit 4 when something needs a response
ephor status widget --cached  # one project, no fetching
```

Use `--cached` in anything that runs often: without it, `status` refetches
whatever is older than the TTL and blocks until every provider answers.

### 9.3 CI steps ephor ships

Three steps ship, and they version with ephor
([§FS-009-shipped-actions](../requirements.md#fs-009-shipped-actions-what-ephor-ships-for-ci-runs-from-the-repository-alone)):
pin the release you consume, as you pin any dependency.

| Step | Does |
|---|---|
| `setup` | installs a pinned ephor release, checksum-verified, and puts it on `PATH` |
| `validate` | holds this repository's `ephor.json` — and a committed registry, if it keeps one — to the published schemas |
| `check` | runs the check verbs this repository declares (§4.2.1), or one feature's smoke |

The rule that selects them: **a shipped step runs from repository-committed
material and workflow inputs alone**. None of them reads a registry of yours,
your bindings, or a credential for your sources — so none of them is the watch.
The loop stays on machines that have a site; shipping it hosted would be
shipping someone's configuration.

As whole jobs:

```yaml
jobs:
  materials:
    uses: vjovanov/ephor/.github/workflows/ephor-validate.yml@v0.1.0
    with:
      version: "0.1.0"
      registry: infra/workspaces.json   # omit where you keep none

  gate:
    uses: vjovanov/ephor/.github/workflows/ephor-check.yml@v0.1.0
    with:
      version: "0.1.0"
      per-feature: true                 # one job per feature your smoke lists
```

Pin `version` to the same release you pinned the workflow at. The jobs fetch
their own steps at `v<version>` — a reusable workflow's relative `uses:`
resolves against *your* checkout, not against ephor's — so a `version` that
disagrees with the `@ref` runs one release's steps against another's, and a
tag that does not exist fails at the fetch rather than three steps later.

Or as steps, when you are composing something of your own:

```yaml
      - uses: vjovanov/ephor/.github/actions/setup@v0.1.0
        with: { version: "0.1.0" }
      - uses: vjovanov/ephor/.github/actions/check@v0.1.0
        with: { verbs: "style smoke" }
```

`check` is the same command you can run yourself:

```bash
ephor check                        # the aggregate, or what else is declared
ephor check --verb style --verb smoke
ephor check --feature reflection   # one feature's smoke
ephor check --list-features --json # what a matrix fans out over
ephor validate --manifest .        # what `validate` runs
ephor --registry infra/workspaces.json validate --schema-only
```

It reads the project's declaration and nothing else, exits non-zero when a
verb fails, and treats `75` as parked rather than failed (§7.3). A project
that declares nothing is told so — with both ways to declare something named
— rather than passing an empty gate.

---

## 10. Extending ephor

### 10.1 A forge, out of process

Any `provider` name that is not built in names a **forge extension**: an
executable `ephor-forge-<name>` on `PATH` (or an explicit `"command"`), which
ephor runs once per capability with a JSON request on stdin and JSON on stdout
([§FS-001-forge-interface.2](../requirements.md#2-two-transports-one-interface)).
A shell script with `jq` is a complete implementation.

```text
ephor-forge-<name> capabilities   <<< '{"config":…,"project":…}'
ephor-forge-<name> pull-requests  <<< '{"config":…,"tickets":[…],…}'
ephor-forge-<name> issues         <<< '{"config":…,"tickets":[…],…}'
ephor-forge-<name> failures       <<< '{"config":…,"repo":…,"number":…}'
ephor-forge-<name> restart        <<< '{"config":…,"repo":…,"number":…,"scope":"failed"|"all"}'
ephor-forge-<name> react          <<< '{"config":…,"target":…,"emoji":…}'
ephor-forge-<name> resolve-task   <<< '{"config":…,"target":…}'
ephor-forge-<name> reply          <<< '{"config":…,"target":…,"text":…}'
```

The whole provider block is passed through as `config`, so an extension takes
its own options. `capabilities` declares what the rest will answer, and ephor
degrades to that — but a capability probe that *fails* is a broken forge, not a
forge that does very little.

A pull request the user is reviewing may carry `review`, the verdict *they*
gave (`approved`, `changes-requested`, `commented`), declared as `"review":
true`. Only the forge knows it: a reviewer list says who was asked, a
conversation says who spoke, and neither says who answered — an approval leaves
no message behind. Report it and a reviewing row can tell a change the reader
has dealt with from one they have not ([§6.2](#62-reading-a-row)); leave it out
and nothing else changes. `failures` is the one call a refresh never makes:
it is asked when a reader opens a red gate, so it may take as long as it needs.

`restart` runs the gate again, declared as `"restart": true`
([§7.1](#71-quick-actions)). The `scope` is the caller's word and never the
implementation's guess — `failed` is what is not green plus everything
downstream of it, `all` is everything the gate covers — because the two differ
by an hour of somebody else's machines. Answer with what you actually asked
for: `{"asked": 12}`, plus `"skipped"` for anything the forge cannot re-run.
Where a gate is started as a whole and executed elsewhere, **omit `asked`** and
answer `{"note": "…"}` in the forge's own words: reporting a zero there would
read as *nothing needed restarting*, which is the one thing it must not say. A
request the forge declines is a non-zero exit, never an empty answer.

`react`, `resolve-task` and `reply` receive back, verbatim, the `react` and
`task` descriptors the extension put on a message and the `reply` descriptor it
put on a thread: they are its own, and ephor reads only `task.state` (`open` /
`resolved`) out of them. A message with no descriptor gets no key on the thread
screen, which is how a read-only implementation says so — there is nothing to
declare beyond leaving it out. `reply` carries the words a person settled on
and posts them as they stand ([§8.12](#812-an-answer-comes-back-as-a-proposal)).

Policy is never an extension's business: what counts as answered, what needs a
response, how threads and gates roll up, how items match branches, what is
unread — all of that is ephor's, applied identically over every implementation.

### 10.2 A provider, in process

One module in `src/feed/providers/` implementing `Provider`, plus a match arm
in `providers::build_provider`. A provider may also offer **quick actions** on
items it produced, and answer `failures` and `restart` for a gate. Implementing the
`Forge` trait instead gets the whole item-building policy for free.

### 10.3 A state machine of your own

`ephor work states > my-states.yaml`, edit, and either drop it in a work root
as `states.yaml` or point `work.states` at it. Recipes then name its states in
their `state` field; one naming a state the machine does not declare is refused
by name rather than written.

### 10.4 An agent of your own

Which agent and model each state runs on is the machine's business, not
ephor's: a state names one with `target: "<agent>[<mode>]:<provider>:<model>"`.
The runtime knows a handful of agents already; anything else — or any agent
that needs fixed flags — is a few lines in `~/.config/rhei/settings.json`:

```json
{ "agents": { "pi": {
    "command": ["/home/you/.local/bin/pi", "--provider", "openai-codex"],
    "prompt_flag": "-p", "model_flag": "--model", "skill_flag": "--skill",
    "modes": { "high": ["--thinking", "high"] } } } }
```

Two things to check before trusting one: the runtime spawns processes
directly, so a shell alias or function is not enough — give it the executable's
path. And **the agent must be able to write**: a state's output artifact is how
work advances, so an agent whose sandbox denies file writes leaves every ticket
stalled on *"required outputs are missing"* no matter how well it reasons.

---

## 11. Reference

### 11.1 Every command

```
ephor list | validate | ensure-agents | update            # the registry
ephor refresh | status | feed | mark-read | failures      # the feed
ephor restart --scope failed|all                          # run a gate again
ephor rebase | checkout | branches                        # the checkout
ephor check | validate --manifest | schema                # the project interface
ephor actions [list] | actions run <id> | actions open <id>  # what may be done here
ephor thread <id> | react | tick | reply                  # a conversation
ephor work list | offers | dispatch | ask | sync | cancel | lay | run | forget
ephor work workflows | states                             # what the runtime carries
ephor operations [attach <run>]                           # the board — alias: ops
ephor job list | log <id>                                 # what runs beneath the screen
ephor tui                                                 # alias: inbox
```

Every one of these takes `--json` and prints the same answer as JSON
([§REQ-002-parity](../docs/requirements/REQ-002-parity.md),
[§11.4](#114-the-machine-form)). The two exceptions are the two that are not
readings: `schema` prints a published JSON document already, and `tui` is the
interface itself.

Most of them are also an ability the interactive interface offers. `list`,
`validate`, `ensure-agents`, `update`, `check`, `schema` and `work states` are
not, and are not meant to be: they set the site up or ask a repository about
itself, which is work you do before there is anything to watch rather than
while watching it. Parity runs the other way for those — the interface is owed
a key for an *ability* it would otherwise be the only way to reach, not for
every command
([§REQ-002-parity.2](requirements/REQ-002-parity.md#2-parity-runs-both-ways)).

`check`, `validate --manifest` and `schema` are the three that need no site at
all.

A checkout is enough for those three, which is why CI can run them
([§FS-006-project-interface.11](../requirements.md#11-the-interface-is-versioned),
[§9.3](#93-ci-steps-ephor-ships)).

### 11.2 Environment

| Variable | Effect |
|---|---|
| `EPHOR_REGISTRY` | registry path |
| `EPHOR_SCHEMA` | registry schema path |
| `EPHOR_STATUS_CONFIG` | feed config path |
| `EPHOR_HOME` | legacy config root (`$EPHOR_HOME/config/*.json`) |
| `XDG_STATE_HOME`, `XDG_CONFIG_HOME` | where state and config live |
| `NO_COLOR` | plain output |
| `PAGER`, `EDITOR` | used by quick actions, `e` on the work screen, and reading a job's log |

### 11.3 Files

| Path | What |
|---|---|
| `~/.config/ephor/workspaces.json` | registry |
| `~/.config/ephor/status.json` | feed, actions, work |
| `~/.local/state/ephor/feed/*.json` | one cache per project |
| `~/.local/state/ephor/seen.json` | unread tracking |
| `~/.local/state/ephor/work.json` | the work ledger |
| `~/.local/state/ephor/jobs/<id>/` | one job: `job.json`, `log`, `lock`, `outcome.json` (§8.14) |
| `~/config/secrets/ephor/*.json` | provider secrets |
| `<forest root>/ephor.json` | the project's own manifest, if it wrote one (§4.2.1) |
| `<checkout>/panta/` | work roots: plans, state machine, runtime artifacts |
| `<work root>/runtime/ephor/<plan>.reply.md` | a drafted answer, until you post it (§8.12) |

### 11.4 The machine form

Nothing lives behind the screen alone. Every ability the inbox offers is also
a command, and every command that prints a reading takes `--json`
([§REQ-002-parity](requirements/REQ-002-parity.md)) — which is what makes ephor
usable by the runtime it hands work to.

| The key | The command |
|---|---|
| `x` — what may be done here | `ephor actions --item ID` (or `--project P --branch B`) |
| `enter` / `1`–`9` — run one | `ephor actions run <id> --item ID` |
| `enter` / `l` on a row that says *running* — open it | `ephor actions open <id> --item ID` |
| `t` — pick who gets the work | `ephor actions run <id> --item ID --hand ada:high` |
| the freehand row | `ephor actions run --item ID --command '…'` |
| `v` — the conversation | `ephor thread ID` |
| `+` — react | `ephor react ID THUMBS_UP --message 0` |
| `t` — tick a task | `ephor tick ID --message 1` |
| `p` — send the drafted reply | `ephor reply ID` (or `ephor reply ID some words`) |
| `w` — what is being done about it | `ephor work offers --item ID` |
| `a` / `s` / `c` / `R` — ask, reopen, cancel, run | `ephor work ask` / `sync` / `cancel` / `run` |
| `z` — the tickets that are over | `ephor work offers --item ID --all` |
| `c` — the gate spelled out | `ephor failures --item ID` |
| `;` — the operations board | `ephor operations` |
| `a` on the board — watch a live run | `ephor operations attach <run>` |
| `L` — what a job wrote | `ephor job log <id>` (`--follow` keeps up; with `--json` it waits and then answers) |
| `m` / `d` / space — mark read | `ephor mark-read --id ID` |
| `u` — only what is unread | `ephor feed --unread` |
| `r` — fetch | `ephor refresh` |
| the branch rows | `ephor branches [PROJECT]` |

Moving a cursor, folding a section, and opening your own pager, editor or
browser on a path a reading already names are presentation, and owe no
command. The list of both is in `src/api/parity.rs`, and `just check` fails on
a key that is on neither.

Under `--json` the reading is **alone** on standard output — notes, progress
and provider failures go to stderr, so a program never parses them by
accident. A command that changes something prints what it changed:

```bash
ephor actions run rebase --item "$id" --json   # {"ok":true,"says":…,"steps":[…]}
ephor rebase --project widget --json           # per repository: rebased, conflicted, dirty…
ephor work dispatch --item "$id" --json        # {"opened":1,"items":[{"ticket":…}]}
ephor schema views                             # what every one of them may print
```

The shapes are published: `ephor schema views` prints the schema, and it is
the stability surface — a field is added freely, and renaming or removing one
is a release note. It is also checked: the end-to-end suite runs every command
that takes `--json` and validates what it prints against its own entry, so the
schema describes the answer rather than merely naming it. `ephor feed`,
`ephor status` and `ephor list` print documents another schema already
describes (`ephor schema forge`, `ephor schema registry`), and their entries
point there rather than repeating it.

A command that is **refused** answers too. Under `--json` it prints
`{"ok": false, "says": "…"}` on standard output — the same shape a move that
happened prints — and keeps the exit code it had. The reason is on stderr as
well, for whoever is watching; a script that reads only standard output still
learns that the thing did not happen and why.

Two names are ephor's own and configuration may not take them: `@command` is
the freehand row and `@workflows` is the row that opens the runtime's
workflows. An entry or a recipe whose `id` starts with `@` is refused when the
configuration is read, rather than quietly standing beside the row it shadows.

---

## 12. Troubleshooting

**A section is empty and I do not believe it.** Check the header and stderr:
a failed provider is always named, its last-good items are marked `(stale)`,
and the process exits non-zero. An empty section with no warning really is
empty.

**`ephor status` hangs.** It is refetching everything older than the TTL, in
series. Use `--cached`, or run `ephor refresh` on a timer.

**"Project 'x' has no feed configuration."** It is in the registry but not
under `projects` in `status.json`, or the two spell its id differently.

**"x is not checked out (… is missing)."** The item's branch workspace does not
exist. Configure a `checkout` command for the project and the menu will offer
to make it, or check it out by hand.

**An action shows "(unavailable)".** The reason is on the row: usually it
declares `requires_checkout` and the item is linked to no branch, so there is
no workspace to make. `Enter` on it does nothing on purpose — the menu stays
where it is, with the reason still under your eye.

**Dispatch says a recipe "starts in state 'x', which the machine … does not
declare".** The work root's `states.yaml` governs. Either use a state it has,
or install ephor's with `ephor work states > <root>/states.yaml`.

**Dispatch says a project "already holds plans of its own".** Another rhei
project is at `work.root` with plans and no state machine. Point `work.root`
elsewhere, or install a machine there deliberately.

**`ephor work list` says `⚠ plan missing`.** The plan the ledger points at was
deleted. `ephor work forget --missing` drops the entry; dispatching again
starts a fresh plan.

**Work looks finished but the item moved on.** That is what `⟳` means. `s` on
the work screen, or `ephor work sync`, writes the next ticket.

**Cancel says the machine "declares no final 'cancelled' state".** The work
root's `states.yaml` governs, and cancelling moves the ticket into a state it
must declare. Add a `cancelled` state with `final: true` and a `from: "*"`
transition into it — `ephor work states` prints ephor's machine, which has
both — to that root's `states.yaml` and to the file `work.states` points at,
so the next root gets it too. An existing `states.yaml` is never rewritten
for you (§8.4).

**Cancel says the runtime "refused: … cannot leave state …".** That is the
runtime's own sentence, relayed. The shipped runtime enforces a state's
declared `outputs:` on every transition out of it, the edge into `cancelled`
included, so a ticket parked in a state whose artifacts were never written
cannot be moved through the verb until the runtime exempts cancellation the
way it exempts its own failure routes. ephor does not edit around it
([§DA-005-cancel-is-the-runtimes-move](decisions/architectural/DA-005-cancel-is-the-runtimes-move.md));
the plan is yours to hand-edit if you must.

**An agent never runs.** ephor writes tickets; the runtime runs them.
`ephor work run`, or `rhei run` in the checkout.
