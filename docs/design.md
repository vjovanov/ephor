# The shape of ephor

This is the master design for ephor's boundaries and core model. It is design
material, not specification: `requirements.md` remains the authority until this
document is decomposed into the grund tree (the decomposition map is §12), and
where the two conflict before that lands, the tree wins. IDs written bare here
(REQ-001, FS-006, …) are working names for declarations that do not exist yet.

The stance, from the README's first paragraph: the ephors *"did none of the
governing themselves, but they observed, and they could summon and suspend."*
Everything below is that sentence made mechanical. ephor observes matters and
summons capabilities; it never embodies the doing. Every capacity it lacks is
reached across a seam, and every seam has the same anatomy.

## 1. The conceptual inventory

Eight concepts, and deliberately no ninth. Four nouns describe what ephor
watches; four constructs describe how it acts. If a future feature cannot be
expressed in these eight, the model — not the feature list — is what must be
revisited.

**The nouns:**

- **Matter** — the subject under discussion or observation: a pull request, an
  issue, a local ticket, a periodic build, a custom-status subject, or a bare
  topic. Matters are the feed's rows, the unit of attribution, state,
  fingerprinting, and dispatch. *The dossier is the dossier of a matter.*
- **Discussion** — messages grouped in one channel about a matter: ordered
  messages with authors, reactions, and task boxes. The needs-response calculus
  (answered by a later message, a reaction on the message, or a ticked task;
  task state outranks the last word in both directions) is per discussion and
  channel-generic.
- **Channel** — the venue a discussion lives in: a pull request's review
  threads, a mail thread, a Slack thread, a GitHub Discussion. What "grouped"
  means is the channel adapter's policy (mail groups by `References`, Slack by
  thread or a window around a mention). Channels declare capabilities — reply,
  react, tick — in the §FS-001-forge-interface pattern.
- **Event** — a non-conversational observation that moves a matter's state:
  the gate went red, the pull request merged, a check finished, a ticket
  closed. Events are what fingerprints digest, and why resurfacing can say
  *why* ("⟳ gate went red") instead of merely reappearing.

**The constructs:**

- **Summons** — the single operational primitive:
  `Summons { verb, binding, place, dossier } → Answer { exit_code, answer_file, output }`.
  Run a bound command in a resolved place with the dossier exported as
  `EPHOR_*`; read back the exit code and an optional structured answer.
  Configured actions, custom-status, the checkout command, every check verb,
  every CI verb, local-ticket CLIs, and the runtime's run are all Summonses.
  One executor, one refusal path, one answer reader.
- **Attribution** — one matching engine, two stages:
  discussion → matter (explicit venue beats reference beats semantic match),
  then matter → (project, branch). Evidence on one side, declared identity on
  the other, and a visible unattributed bucket at both stages. Today's branch
  matching is this engine scoped to one project; it is promoted, not
  duplicated.
- **Forest** — git is assumed; other version control is out of scope. A
  project's place is a forest of git repositories under a root — the
  multi-repository shape (ce + ee + a thin workspace repo composing them) is
  the model, a
  single repo is a forest of one. Staleness, rebase, landing, and gate counts
  are folds over the forest. Because git is assumed, branches, checkouts,
  ticket keys, and behind-counts are *derived by probing*, and the registry
  row shrinks to what cannot be probed.
- **Capability table** — per project, resolved at refresh: which rungs of the
  ladder hold (§4). Features declare the rungs they need; menus, refusals, and
  recipe eligibility read the table. Absence is always a stated degrade, never
  an error — as the TUI already practices, now as mechanism rather than
  discipline.

## 2. The boundary law (REQ-001)

One law, stated once, cited from every edge:

1. **Every capacity ephor does not embody crosses a seam**, and every seam has
   four parts: a **contract in materials** (files, commands, environment,
   exit codes — never linked code), a **configured binding** (which
   implementation fills it, and where that choice lives), a **shipped default
   or worked example** (so it works out of the box, shipped *with* ephor,
   never fused *into* it), and a **degrade rule** (what a missing binding
   does, stated, visible, never an error).
2. **Three homes, one resolution order.** Description and identity live in
   the registry row; operational bindings live in site configuration;
   project-native conventions are probed in the checkout. Resolution is
   always *site config > project manifest > probe*. Probing is defaulting;
   the manifest is declaration; site config is override.
3. **Requirements are capabilities, never artifacts.** ephor may require a
   project to *be* things — on a forge, a git forest, checkable by command —
   never to *contain* ephor-specific things. The rung test for any future
   requirement: if a file or behavior would only make sense because ephor
   exists, it cannot be a requirement on the project; it may only be an
   offer (§6) or site configuration.
4. **The footprint rule.** Where ephor writes into a checkout, its presence
   is confined and disposable (a gitignored work root, a generated
   AGENTS.md). Deleting ephor's traces leaves a clean project; deleting a
   project's row leaves a clean ephor.
5. **No product literal outside its adapter.** `rhei`, `gh`, `beads`, and any
   forge or channel name appear only in their adapter module, shipped assets,
   examples, and documentation — enforced by a check, not a convention (§10).

## 3. Matters in detail

**Identity.** A matter's subject key is forge-stated (`gh:owner/repo#123`),
ticket-stated (`ticket:ABC-42`), store-stated (`rhei:panta/boundary.3`,
`bead:…`), or synthesized for topics (`topic:<digest>` — a discussion that
matched a project but no known matter). Identity is the subject the source
stated, never the title.

**Merging vs. linking.** Observations from different sources about the *same
subject key* merge into one matter — the rule the feed already enforces ("one
subject is one row"), now definitional. Matters that *reference* each other —
the pull request implementing a ticket, the bead tracking a gate failure — are
**linked, not merged**, and the branch grouping presents them together: the
registry's ticket inference already relates branch ↔ ticket ↔ pull request.
Most-specific wins attribution: a discussion *on* the pull request belongs to
the pull request matter; a mail thread *naming* the ticket belongs to the
ticket matter, linked to the pull request.

**Kinds and categories.** Matter kinds: `pr`, `issue`, `ticket` (local),
`build` (periodic CI not tied to a pull request), `status`, `topic`.
§FS-003-feed-categories maps kind × role onto the presented categories;
finished matters land in Recent. Today's `ci` kind dissolves into events on a
`pr` matter (or a `build` matter when periodic); `github-threads` stops
minting rows — an unresolved review thread is a discussion of the pull
request's matter.

**State, fingerprint, resurfacing.** A matter folds its events into state
(open/merged, gate counts per repo, ticket state). The fingerprint digests
state plus each discussion's (last activity, message count, task states) plus
the event tail. Mark-done is matter-level; a moved fingerprint resurfaces the
matter and reopens its work with the delta named. Unattributed discussions and
matters are a visible bucket in the TUI and `ephor feed --unattributed` —
mapping failures are seen, never lost.

## 4. The capability ladder

Resolved per project into the capability table; each rung names its features
and its degrade:

| rung | established by | unlocks | absent |
|---|---|---|---|
| observable | registry row + ≥1 source answering | feed, threads, mark-read | not a project |
| placed | forest root exists on disk (probed) | actions, update, staleness | menu refuses with reason |
| branch-addressable | workspace template + conventions (row) | item→workspace resolution, per-branch grouping | items group under "(not linked)" |
| checkout-able | bound checkout command (site config) | `requires_checkout` actions, `needs_checkout` work | "(will check out first)" / "(unavailable: …)" |
| checkable | probed or declared check verbs (§5) | meaningful verify, fix-gate work that trusts itself | guess-list fallback, then opaque |
| gated | bound CI verbs (§5) | gate counts, failure dossiers, restart | counts omitted, restart not offered |
| ticketed | probed local stores (§5) | local tickets in the feed, implement recipes on them | rung absent, no feature |
| workable | bound runner + checkout-able + work root | dispatch, `work run`, the loop | tickets-on-disk; run refuses with guidance |

Git is not a rung: it is the substrate. A placed project *is* a forest.

## 5. The seams

| seam | verbs | contract | binding home | shipped default | degrade |
|---|---|---|---|---|---|
| remote sources | fetch discussions/events since | provider capability set (§FS-001-forge-interface) | site config, shareable across projects | `gh`-backed GitHub (PRs, notifications) | last-good items marked stale |
| attribution | match evidence → matter → project, branch | identity facets, pure data | registry row (manifest hints) | ticket/repo/branch/alias matching | unattributed bucket |
| project checks | `check`, `style`, `smoke [feature]`, `smoke --list` | commands at forest root, `EPHOR_*` env, exit codes, answer file | probed (`./check.sh`, `./check-style.sh`, `./smoke-test.sh`) or manifest | examples in `config/` | guess list, then opaque |
| CI | `status`, `failures`, `restart` | commands with dossier env, answers in files | manifest (project truth) with site override | `gh pr checks` / `gh run view` | counts omitted, restart unavailable |
| local tickets | list, read, (tick) | the store's own files/CLI in the checkout | probed by convention (`panta/`, `.beads/`) or manifest | rhei and beads readers | rung absent |
| checkout | make `$EPHOR_WORKSPACE` exist | command contract, verified afterwards | site config | example | offers gated, reason shown |
| runtime | write plan, run, read verdict | plan format + `work.runner` + `VERDICT:` line | site config | rhei (assets, states machine) | tickets-on-disk; run refuses |
| CI-hosted automation | shipped workflows/actions (§9) | workflow inputs + repo-committed config only | the calling repository | validate / check / setup | plain refusal in the job log |

Design notes on three of them:

**Checks are a verb set, and every script is self-contained.** If the smoke
test needs a build, `smoke-test.sh` builds — ephor never learns project build
sequencing; that was the lesson of the smoke test that needed a build. Feature
enumeration is a discovery contract: `smoke-test.sh --list` (or a static
`features` list in the manifest) yields per-feature smoke as verify steps and
quick actions; without it, smoke is one opaque verb. Verb *composition* (style
→ smoke → check) is policy and lives in the state machine in site config: a
verb is one Summons, orchestration stays downstream. Exit semantics are shared
across all verbs: `0` pass, nonzero fail, `75` park/not-applicable.

**CI verbs are project truth.** How to ask a project's gate what failed is the
same for every developer of the project, so the binding's home is the
manifest, with site override for credentials or variants. The existing
`ci-failures.example.sh` and `restart-gate.example.sh` are the seam's embryo;
restart keeps the §FS-005-dispatch.11 semantics — a failure that is not the
change's fault is restarted, not fixed, and restarting restarts downstream
gates too.

**The runtime stays named, but bound.** rhei is the shipped default and the
documented plan language is the contract — that part of the old
§FS-005-dispatch lead survives. What changes: the *process* is a binding
(`work.runner`), core code holds no `rhei` literal outside the adapter, and
with no runner the loop degrades to tickets-on-disk that remain readable,
diffable, and hand-editable. A recipe's brief, the dossier, and the ledger are
runtime-agnostic; the verdict contract (`VERDICT: done | partial | blocked` as
the first line of the result) is part of the plan-language boundary, not of
rhei-the-binary.

## 6. The project manifest (`ephor.json`)

The one genuinely new architectural element. Offered, never required: an empty
manifest is valid, a missing one costs nothing that site configuration cannot
restore. It is how a project *chooses to speak* — declaring what probes would
have guessed and offering what probes cannot find. It lives at the forest
root; for a multi-repository project that is the thin workspace repo, for a
single repo it is the repo root.

```json
{
  "v": 1,
  "identity": {
    "name": "widget",
    "aliases": ["widget-ce", "widget-ee"],
    "ticket_patterns": ["ABC-\\d+"],
    "repos": ["acme/widget", "acme/widget-enterprise"]
  },
  "forest": [
    { "name": "ce", "path": "ce", "remote": "git@github.com:acme/widget.git", "main": "master" }
  ],
  "checks": {
    "check": "./check.sh",
    "style": "./check-style.sh",
    "smoke": { "command": "./smoke-test.sh", "features": "list" }
  },
  "ci": {
    "status":   { "command": "./scripts/gate-status.sh" },
    "failures": { "command": "./scripts/gate-failures.sh" },
    "restart":  { "command": "./scripts/gate-restart.sh" }
  },
  "tickets": [ { "kind": "rhei", "path": "panta" }, { "kind": "beads", "path": ".beads" } ],
  "actions": [
    { "id": "gate", "icon": "🧪", "description": "run the gate",
      "command": "./gate.sh --tags style", "cwd": "repo:ce",
      "when": { "kinds": ["pr", "ci"] }, "requires": ["checkout"] }
  ]
}
```

Rules: every field optional; commands resolve relative to the forest root;
`cwd` is `root` (default) or `repo:<name>`; identity fields are hints the
registry row adopts unless it overrides — **the row stays authoritative,
because attribution keys must not be forgeable by a checkout**. Trust is
stated, not hidden: manifest commands run as you, with exactly the trust you
extend to running the repo's own build; the row can set `"manifest": "full" |
"descriptive-only" | "ignore"` for checkouts trusted less. Recipes are
deliberately *not* manifest content in v1: an action is run when a person
picks it from a menu, but a recipe spends your agent time on its own match —
a repository does not get to spend that.

## 7. The answer envelope

Every summoned command may answer in structure; none must. The exit code is
always authoritative. Structure goes to the file named by `$EPHOR_ANSWER`
(stdout and stderr stay free for human-readable streaming — checks print
build logs); `custom-status`'s stdout-JSON survives as a legacy binding
option. The envelope speaks the model's nouns:

```json
{
  "v": 1,
  "summary": "style clean, 2 smoke failures",
  "url": "https://ci.example.com/build/4711",
  "needs_response": false,
  "matters": [ { "kind": "ticket", "key": "bead:a1f3", "title": "…", "state": "open", "refs": ["ABC-42"] } ],
  "discussions": [ { "matter": "bead:a1f3", "channel": "beads", "messages": [ { "author": "…", "time": "…", "text": "…" } ] } ],
  "events": [ { "matter": "gh:acme/widget#18774", "kind": "gate", "gate": [ { "repo": "ce", "passed": 72, "failed": 1, "running": 3 } ] } ],
  "failures": [ { "id": "image-hello", "repo": "ce", "summary": "image build OOM", "log": "runtime/logs/image-hello.log" } ],
  "features": [ { "id": "reflection", "description": "reflection metadata" } ],
  "data": { }
}
```

`failures[]` and `features[]` are verb-level sugar for simple scripts; the
reader normalizes them into events and facts. Each verb's contract names the
fields it reads — checks read `summary` + `failures[]`, `smoke --list` reads
`features[]`, `ci.status` reads the gate event, ticket readers read
`matters[]` + `discussions[]`, custom-status reads `summary` /
`needs_response` / `url` / `matters[]`. Paths resolve relative to the
summons's cwd. Unknown fields are ignored everywhere (must-ignore forward
compatibility); `v` bumps only on incompatible change, with a changelog entry
per §FS-002-release.

## 8. Offers: actions and recipes, one language

An offer is `{ id, icon, description, command, cwd, when, requires, confirm }`.
The `when` selector language is shared between actions and recipes — `kinds`,
`roles`, `gate`, `needs_response`, `sources` — a recipe is an offer whose
command opens a ticket. `requires` names capability rungs and unmet rungs
render "(unavailable: …)" per the degrade law. Menu order is provenance
order: ephor's quick actions (built from what it observed — §FS-004-quick-actions),
then the manifest's offers, then yours; same-`id` overrides run the other way —
yours beats the project's beats the built-in, as shipped recipes are already
replaced by id. The last entry is always "⌨ run a command here…".

## 9. Shipped GitHub Actions

The packaging principle (§FS-009-shipped-actions): a shipped action is a
Summons wearing CI clothes,
and anything shipped must run from **repo-committed material alone** — the
manifest and workflow inputs, never a personal site. That selects the
inventory:

1. **validate** — schema-validate `ephor.json` (and a committed registry, for
   registry repos). The schemas ship embedded and printable (`ephor schema
   <name>`), so this is one step.
2. **check** — read the manifest's declared check verbs and run them. This is
   the clean-separation payoff made concrete: because the manifest makes the
   project's checks machine-readable, ephor can ship the generic CI workflow
   that runs them — workflow generic, manifest specific, per-feature smoke as
   a matrix when `features` is declared.
3. **setup-ephor** — install a pinned ephor release; the building block for
   the other two and for anyone composing their own.

The watch-and-work loop does **not** ship as a hosted workflow: refresh and
work sync need a site (registry, credentials, state) and agents need a
machine that is yours; that remains systemd's job, and a reusable workflow
for self-hosted runners can follow once someone actually wants it. Factoring
ephor's own release family into `workflow_call` form (rhei carries a
hand-copy today) is worthwhile housekeeping but is release engineering, not
this boundary — it rides with §FS-002-release.

## 10. Code structure and enforcement

Layered, one binary:

- **`core`** — Matter, Discussion, Channel, Event, Project, Identity, Forest,
  CapabilitySet, Dossier, Summons/Answer, Ledger, fingerprints. Pure, no IO,
  and no product literal — the mechanically enforced clean layer.
- **`sources`** — remote providers (GitHub via `gh`: PRs, notifications) and
  checkout sources (git prober, custom-status, panta reader, beads reader).
  Each declares capabilities; each vendor literal lives in exactly one
  adapter here.
- **`seams`** — binding resolution (site > manifest > probe), the Summons
  executor, the answer reader, and the verb modules: checks, CI, checkout,
  local tickets, runtime (rhei adapter).
- **`engine`** — the pipeline, legible as one function:
  **fetch → attribute → merge → offer → summon → record → resurface** —
  plus cache and refresh.
- **`surfaces`** — CLI, TUI, widget, JSON output.

Enforcement: the literal-confinement check (§2.5) in the mold of
`check-private-words.sh`, wired into `just check` and CI; schema round-trip
tests for manifest, envelope, and registry; one e2e per seam using stub
bindings — a fake forge script, a stub runner, temp git forests — in the
existing `e2e/` harness. The feed cache is a cache: the matter-model
migration rebuilds it rather than converting it. `status.json` is the one
real breaking change (sources move to site level, bindings restructure);
ephor reads the legacy shape with a deprecation note for a release or two.

## 11. Getting there

Strangler order, each step shippable:

1. **Summons executor** extracted from the action-menu code; route
   custom-status and `work run` through it. Add `$EPHOR_ANSWER` and the
   envelope reader.
2. **Capability table**; port TUI refusals and offer gating onto it.
3. **Forest**; port staleness, rebase, and landing to folds.
4. **Matter / Discussion / Event core types**; rebuild the cache; dissolve
   `github-threads` into discussions and `ci` into events.
5. **Fetch/attribute split**, with `github-notifications` as the pilot — it
   already fetches unscoped items and hand-rolls placement.
6. **Check verbs** (probe + manifest), `smoke --list`, shared exit
   semantics.
7. **CI verbs**, promoting the two example scripts to the seam's definition.
8. **Local tickets** — panta reader first (dogfood: it can subsume the
   ledger's verdict-reading), beads second.
9. **Runtime binding** (`work.runner`) and literal confinement.
10. **Schemas embedded**, `ephor validate` grows the manifest target,
    `ephor schema` prints them.
11. **Shipped actions**: validate, check, setup-ephor.
12. **Enforcement and e2e** per seam.

Mail and Slack are not in this list, deliberately: after step 5 they are
providers, not architecture. That is the test the design passes or fails.

## 12. Decomposition into the grund tree

- **REQ** (new kind, `docs/requirements/`): REQ-001, the boundary law of §2 —
  one law, numbered clauses, cited from every seam, every adapter, and the
  enforcement checks.
- **FS**: FS-006 downstream API (three homes, manifest, envelope, offers —
  §5–§8); FS-007 matters, discussions, events (§3, superseding parts of
  §FS-003-feed-categories's mechanics while keeping its categories); FS-008
  attribution and identity; rewrite of §FS-005-dispatch for the bound
  runtime (§5); FS-009 shipped actions (§9). §FS-001-forge-interface stays
  the remote-source law; §RM-001-forge-interface remains its remediation and
  gains a cross-reference to REQ-001.
- **AR**: eight short pages — layers, summons, attribution, forest,
  capability table, matter model, runtime adapter, pipeline. That the
  architecture needs exactly the conceptual inventory of §1 is the design's
  self-check.
- **DA/DF**: the runtime-boundary reversal (supersedes the FS-005 lead
  stance, with the tradeoff recorded); the manifest as offer-never-requirement;
  the fetch/attribute split and its `status.json` cost.
- **E2E**: one case per seam, stub-bound.
- **RM**: this design as a milestone entry, sequenced with RM-001.
