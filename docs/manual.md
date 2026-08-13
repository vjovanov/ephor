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

Ten words, and the rest of the manual is about them.

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

**Recipe** — which items deserve work, and what the ticket asks for. Four ship;
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
    "recent_days": 7                  // how long finished work stays under Recent
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
ephor rebase [--checkout DIR] [--project P] [--onto BRANCH] [--item ID] [--dispatch] [--report PATH]
ephor checkout [--project P] [--branch B] [--item ID] [--from BRANCH] [--report PATH]
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
- **`rebase`** replays a checkout onto its main branch, and is what the quick
  action on a branch that has fallen behind runs
  ([§8.11](#811-rebasing-a-branch-that-has-fallen-behind)). Its exit codes are
  its own — `3` is a conflict, not a failure.
- **`checkout`** makes the branch workspace that is not there yet, one working
  tree per repository, and is what the quick action on a missing checkout runs
  ([§7.1](#71-quick-actions)). It needs nothing but the registry: the project
  says where the workspace goes, which repositories it holds, and what a new
  branch grows from.

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
verbs, ticket stores, and offers — menu entries you invoke.

```jsonc
{ "identity": { "aliases": ["widget"], "territory": ["acme-labs"] },
  "forest":   [{ "name": "ce", "path": "ce" }],
  "checks":   { "check": "./check.sh" },
  "actions":  [{ "id": "rebuild", "description": "rebuild it",
                 "command": "./build.sh", "requires": ["checkout-able"] }] }
```

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

### 4.3 Exit codes

| Code | Means |
|---|---|
| 0 | fine |
| 1 | the command failed |
| 2 | configuration or registry error |
| 3 | every provider failed — nothing could be fetched at all |
| 4 | some providers were lost (`refresh`), or unread needs-response items exist (`status --check`) |

`status --check` is built for a shell prompt: it prints nothing and exits 4
when something is waiting on you.

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
| `github-issues` | `gh search issues` by role, plus comments | a comment awaits your reply |
| `github-notifications` | `GET /notifications` — everything GitHub says is directed at you (§5.2) | GitHub's reason is a mention, a review request, an assignment, a broken gate, or an advisory |
| `github-threads` | GraphQL unresolved review threads | the last comment is not yours |
| `custom-status` | any shell command in the workspace | the JSON says so |
| `<anything else>` | an external forge executable (§10.1) | ephor's policy, over what it answered |
| `slack`, `discord`, `email` | stubs; activate by adding secrets | mentions and DMs (planned) |

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
  "participating": true,        // issues you are in but did not open
  "updated_within_days": 30,    // 0 removes the bound
  "limit": 50,                  // per search, per role
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
nest under the registry branch they belong to, matched by ticket key or branch
name, with a *(not linked to a branch)* group for the rest.

### 6.1 The categories

| Category | Holds |
|---|---|
| Status | project status lines |
| My Pull Requests | pull requests you authored |
| Reviewing | pull requests you are on as a reviewer |
| CI | gate and build results |
| My Issues | issues you opened |
| Participating | issues you are in but did not open |
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

Branch rows say whether they are checked out and how far they trail the main
branch, summed across every repository in the workspace.

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
| `x` | the action menu (§7) |
| `m` `d` `Space` | mark done |
| `a` | mark everything visible done |
| `u` | unread-only ↔ everything |
| `[` `]` | previous / next project (Detail) |
| `Esc` `h` | back |
| `r` | refresh (in Detail, only that project) |
| `q`, `^C` | quit |

**Thread screen** — the recorded conversation in full, each message a card with
its author, age, text and reactions:

| Key | Does |
|---|---|
| `j` `k` | previous / next message |
| `f` `b` | page |
| `g` `G` | first / last |
| `+` | react (`←`/`→` or `1`-`8` choose, `Enter` posts) |
| `t` | tick the selected task |
| `x` | actions · `o` open · `m` done · `Esc` back |

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

### 7.1 Quick actions

Entries ephor has without being told, on an item where it already knows what
the problem is ([§FS-004-quick-actions](../requirements.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)).
They lead the menu, and configuration adds to them rather than replacing them.

Today, three:

**`✗ see the CI failures`** on a pull request whose gate is red — the check
list as the forge reports it, then every failed job's log, paged. It is offered
only where it would work: the gate is failing, the item still names its pull
request, and the tool that reaches it is installed.

**`⤴ rebase onto <main> (N behind)`** on a pull request whose branch workspace
is on disk and has fallen behind the project's `main_branch`
([§FS-004-quick-actions.6](../requirements.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
It runs `ephor rebase` in that checkout — fetch, replay, and an answer per
repository, every repository in a poly-repo workspace. Where the replay stops
in a conflict, it opens the ticket about it instead of leaving you with a
half-finished rebase and no record of it ([§8.11](#811-rebasing-a-branch-that-has-fallen-behind)).

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
is reported and left alone — git refuses that, and it is right to.

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
| `icon`, `description` | the menu row |
| `command` | run with `sh -c` in the item's checkout |
| `kinds` | restrict to item kinds; empty offers it everywhere |
| `requires_checkout` | the action needs the item's branch workspace on disk |

**The checkout dependency.** A project may define one `checkout` command whose
contract is to make `$EPHOR_WORKSPACE` exist — ephor verifies the directory
afterwards rather than trusting it. Actions marked `requires_checkout` are
gated on it: when the workspace is missing the menu annotates them *(will check
out first)* and running one chains checkout → action. Without a configured
checkout command, or on an item linked to no branch, they show *(unavailable)*
and refuse with the reason.

**Where a command runs.** In the item's checkout, resolved org → project →
branch: the item is matched to its registry branch, and if that branch
workspace exists on disk the command runs there; otherwise it runs in the
project root. The interface leaves the screen entirely while it runs — so
`lazygit`, an editor, a pager all work — and returns on Enter.

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
silence.

| Rung | Held when | Buys |
|---|---|---|
| observable | a registry row and at least one source configured | the watch |
| placed | the project's root is on disk | actions and update |
| branch-addressable | the row has a `branch_root_template` | a workspace per branch |
| checkout-able | a checkout command is bound, or a checkout is on disk to grow one from | work that edits |
| checkable | `check.sh`, `check-style.sh`, or `smoke-test.sh` at the root | verification that means something |
| gated | a source reports a gate | failure dossiers and the restart |
| ticketed | a `panta/` or `.beads/` store in the checkout | local matters |
| workable | the configured runner is on `PATH` | running the work |

The ladder is resolved per project when the inbox loads, when a refresh
finishes, and after a checkout — it costs a handful of `stat` calls and never
runs anything. What it says is nonetheless what *was* true when it was
resolved, so a command about to run re-checks the two things it leans on (its
directory, and its script if it names one) and fails as the world rather than
from the table
([§AR-005-capabilities.3](architecture/AR-005-capabilities.md#3-the-table-is-honest-about-time)).

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
| `rebase` ⤴ | a pull request of yours whose branch **trails main** | yes |

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
        "target": "claude-code[yolo]:anthropic:claude-sonnet-4-6"   // or "model": "…"
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

`failing` and `blocked` are separate because they ask for different work: jobs
that failed are something a checkout can fix, while a forge refusing an
otherwise green change is usually waiting on a person.

`behind` is measured in your own checkout, not asked of a forge: the branch
workspace's repositories are counted against `origin/<main_branch>` as they
were last fetched. An item ephor cannot measure — no branch, or nothing on
disk — matches neither `true` nor `false`, so a recipe that asks is never
offered blind.

**The brief** takes `{title}`, `{url}`, `{repo}`, `{number}`, `{branch}`,
`{ticket}`, `{state}`, `{gate}`, `{workspace}`, `{root}`, `{project}`,
`{source}`, `{kind}`, `{id}`. An unknown name is left as written, so a typo is
visible in the ticket instead of becoming a blank.

### 8.4 Where work goes, and what runs it

`work.root` — default `{workspace}/panta` — is a rhei project directory in the
item's checkout, one plan per item, named for the item.

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
| `R` | hand **this item's plan** to the runtime |
| `e` | read the plan in `$EDITOR` |
| `o` | open the item in the browser |
| `j` `k` | move between recipes · `f`/`b` page · `Esc` back |

The recipe rows show the words each would actually send, rendered against this
item — dispatching is cheap to press and expensive to run.

`R` leaves the interface entirely: the runtime's own dashboard takes the
terminal while it works, and coming back re-reads the plans.

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
                    [--again] [--updated-within DAYS] [--dry-run]
ephor work ask --item ID [WORDS…] [--state S] [--dry-run]
ephor work sync [--project P] [--dry-run]
ephor work run [--project P] [--item ID] [-- RHEI_ARGS…]
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
  entirely.
- **`run`** groups by work root and names the plans ephor opened, so a runtime
  project you keep in the same checkout for your own work is not swept in. One
  root at a time: tickets in one root are about one checkout, and two agents in
  one working tree edit the same files. Pass runtime flags after `--`.
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
                        "failed": 2, "running": 0, "blocked": false, "messages": 3 } }
      ]
    }
  } }
```

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
ephor rebase --item forge:widget/42 --dispatch    # and open a ticket on conflict
```

It fetches and replays **every repository in the checkout** — the project
type's `repos` where the registry names them, otherwise every git working tree
directly under it — onto the project's `main_branch`, and reports per
repository. Nothing is stashed: a repository with uncommitted work is named and
left alone. Nothing is pushed either; a replayed branch cannot fast-forward,
and forcing is a decision that belongs to a state that says so
([§8.5](#85-a-script-in-front-of-the-agent)).

| Exit | Means | The machine sends it to |
|---|---|---|
| `0` | every repository is on the base — replayed, or already there | land it |
| `3` | one stopped in a conflict, left mid-rebase with the files named | an agent |
| `1` | uncommitted work, no repository, or git refused | a person |

Each argument can arrive as an environment variable instead — `CHECKOUT`,
`PROJECT`, `ONTO`, `ITEM`, `REPORT` — which is how a program state passes it
`{meta.*}`. `config/ci-green.example.states.yaml` wires the whole path:

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

Give the `rebase` recipe `"state": "rebase"` in your `status.json` for tickets
to start there; with the shipped two-state machine they start in `fix`, where
the brief tells the agent to run `ephor rebase` itself and resolve what it
stops on.

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
ephor-forge-<name> react          <<< '{"config":…,"target":…,"emoji":…}'
ephor-forge-<name> resolve-task   <<< '{"config":…,"target":…}'
```

The whole provider block is passed through as `config`, so an extension takes
its own options. `capabilities` declares what the rest will answer, and ephor
degrades to that — but a capability probe that *fails* is a broken forge, not a
forge that does very little. `failures` is the one call a refresh never makes:
it is asked when a reader opens a red gate, so it may take as long as it needs.

`react` and `resolve-task` receive back, verbatim, the `react` and `task`
descriptors the extension put on a message: they are its own, and ephor reads
only `task.state` (`open` / `resolved`) out of them. A message with no
descriptor gets no key on the thread screen, which is how a read-only
implementation says so — there is nothing to declare beyond leaving it out.

Policy is never an extension's business: what counts as answered, what needs a
response, how threads and gates roll up, how items match branches, what is
unread — all of that is ephor's, applied identically over every implementation.

### 10.2 A provider, in process

One module in `src/feed/providers/` implementing `Provider`, plus a match arm
in `providers::build_provider`. A provider may also offer **quick actions** on
items it produced, and answer `failures` for a red gate. Implementing the
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
ephor rebase                                              # the checkout
ephor work list | dispatch | ask | sync | run | forget | states
ephor tui                                                 # alias: inbox
```

### 11.2 Environment

| Variable | Effect |
|---|---|
| `EPHOR_REGISTRY` | registry path |
| `EPHOR_SCHEMA` | registry schema path |
| `EPHOR_STATUS_CONFIG` | feed config path |
| `EPHOR_HOME` | legacy config root (`$EPHOR_HOME/config/*.json`) |
| `XDG_STATE_HOME`, `XDG_CONFIG_HOME` | where state and config live |
| `NO_COLOR` | plain output |
| `PAGER`, `EDITOR` | used by quick actions and `e` on the work screen |

### 11.3 Files

| Path | What |
|---|---|
| `~/.config/ephor/workspaces.json` | registry |
| `~/.config/ephor/status.json` | feed, actions, work |
| `~/.local/state/ephor/feed/*.json` | one cache per project |
| `~/.local/state/ephor/seen.json` | unread tracking |
| `~/.local/state/ephor/work.json` | the work ledger |
| `~/config/secrets/ephor/*.json` | provider secrets |
| `<checkout>/panta/` | work roots: plans, state machine, runtime artifacts |

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

**An action shows "(unavailable)".** It declares `requires_checkout`, and
either the project has no `checkout` command or the item is linked to no
branch.

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

**An agent never runs.** ephor writes tickets; the runtime runs them.
`ephor work run`, or `rhei run` in the checkout.
