# Roadmap

What ephor plans to ship next, in priority order. Each item has a stable ID —
`RM-NNN-slug` — and may be cited from anywhere: commits, PRs, the changelog,
other specs. Shipped items move their detail to
[docs/changelog.md](changelog.md) and keep a one-line pointer here so the
citation does not dangle.

## RM-001-forge-interface: put every forge behind the interface

Implements [§FS-001-forge-interface](functional-spec/FS-001-forge-interface.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface).
Most of it has landed — see `## Unreleased` in [docs/changelog.md](changelog.md)
— and what remains is the last mile before anything is published.

### 1. What

**Landed.** The `Forge` trait carries the capability set of
[§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities) and both
transports answer it: GitHub in the default build as the reference
implementation, and anything else as an executable on `PATH`
([§FS-001-forge-interface.2](functional-spec/FS-001-forge-interface.md#2-two-transports-one-interface)),
with policy above both. The committed registry and feed configuration are
examples, the packaging exclude keeps the real ones out of any artifact, the
inherited `docs/` set is gone, and no employer or vendor identifier remains in
source, tests, examples, or documentation
([§FS-001-forge-interface.5](functional-spec/FS-001-forge-interface.md#5-no-site-specific-data-in-the-repository)) —
`scripts/check-no-site-specific.sh` passes both halves today. The vendor CLI
name is confined to its own adapter and held there by the build
([§REQ-001-boundary.5](requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).

**Owed.** The end-to-end proof: a run configured with nothing but the example
configuration, against a public GitHub repository, producing a feed — which is
what says the examples are a starting point rather than a shape. And the first
release itself, which has to be tagged by hand before the bump workflows have
anything to count from ([§FS-002-release](functional-spec/FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change)).

The boundary half of the same law — the seams a capability is reached across,
rather than the forges reached through them — is
[§RM-003-boundary](roadmap.md#rm-003-boundary-the-seams-the-law-still-owes).

### 2. Why now

It is the last thing between the tree and a first release. It is also what
makes ephor a tool rather than one person's script — the capability set is the
same for GitHub, GitLab, Bitbucket, Forgejo, Jira, and Linear, and only the
transport differs.

### 3. Measurable

`scripts/check-no-site-specific.sh` passes on a clean tree (it does today), the
GitHub implementation answers every capability in
[§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities), and a run
configured with only example configuration produces a working feed against a
public GitHub repository.

## RM-003-boundary: the seams the law still owes

Serves [§REQ-001-boundary](requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy).
The law landed with the interface it describes — the summons executor, the
capability table, the verb seams, the runtime binding, the manifest, and a
literal-confinement check that fails the build
([`## Unreleased`](changelog.md#unreleased)). Four things it names are not
finished, and each is small enough to say exactly.
[§RM-001-forge-interface](roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface)
is the forge half of the same law.

### 1. What

- **`defaults.github_user` is a vendor name in the configuration schema.** Four
  pinned entries on the boundary check's migration ledger carry it
  ([§REQ-001-boundary.5](requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).
  Moving the key into the source's own block is a schema change with a
  migration, which is why it was not done in passing.
- **`forest` is core by the layering and not by structure.** It asks the git
  prober what is on disk, so it is not on the enforced list; it joins when the
  prober moves to sources
  ([§AR-001-layers.3](architecture/AR-001-layers.md#3-from-todays-tree)).
- **The gate seam has no surface.** `status`, `failures`, and `restart` resolve
  and run, and the capability table counts a bound verb — but `ephor failures`
  and the inbox's failures view still ask the provider that reported the item,
  so a project with an internal gate is not yet indistinguishable from a
  forge-hosted one where a person actually looks
  ([§FS-006-project-interface.6](functional-spec/FS-006-project-interface.md#6-the-gate-is-the-projects-in-three-verbs),
  [tests/e2e/cases/E2E-003-gate-verbs.rs](../tests/e2e/cases/E2E-003-gate-verbs.rs)).
- **One task store is recognized and unread.** `beads` is probed and reports
  nothing rather than pretending
  ([§FS-006-project-interface.7](functional-spec/FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)).
  That is an honest degrade and still a reader nobody wrote.

### 2. Why now

Each is a place where the law is observed by intention rather than by
construction, and that is the failure mode the law exists to prevent: a
boundary held by everyone remembering it has already moved. The gate one is
also the visible one — it is the difference between a project's own CI being a
first-class gate and being a fact the capability table knows and nothing shows.

### 3. Measurable

`defaults.github_user` leaves the migration ledger with a schema change and a
migration; `forest` is on the enforced core list in `scripts/check_boundary.py`;
a manifest-bound gate draws the failures view and the restart on a row, with
an e2e scenario that opens it through a surface rather than through the seam;
and a `.beads` store in a checkout produces matters in the feed.

## RM-002-dossier-description: an item's own words belong in its dossier

Serves [§FS-005-dispatch.2](functional-spec/FS-005-dispatch.md#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it).

### 1. What

No provider records the text an item opens with — a pull request's description,
an issue's report. The interface's `PullRequest` and `Issue`
([§FS-001-forge-interface.1](functional-spec/FS-001-forge-interface.md#1-capabilities)) carry a title
and a conversation but no body, so ephor has never had it to give.

### 2. Why now

It is the thickest part of what a dossier is for. Work dispatched on an issue
currently opens on a heading and a metadata table: the ticket says "the issue
and its comments are above" and above is a url. Every other kind of item at
least carries its conversation; an issue with no comments carries nothing at
all, and the first thing the work does is fetch by hand what a refresh could
have kept.

### 3. Measurable

`Issue` and `PullRequest` gain a body, every in-tree provider fills it, and a
dispatched ticket on an issue with no comments still opens with what the issue
says. The dossier's budget covers it as it covers a conversation: bounded,
and saying so where it cut.

## RM-004-windows: ephor runs where there is no POSIX shell

Serves [§FS-001-forge-interface.2](functional-spec/FS-001-forge-interface.md#2-two-transports-one-interface)
and [§FS-006-project-interface.3](functional-spec/FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out).
The CI matrix carried `windows-latest` from the first commit and it was never
once green: every test binary failed to compile, so nothing behind that had
ever been looked at. Compiling it was one line, and what came out from behind
it is a port. `windows-latest` is off the matrix until this is done, because a
leg that is always red is a leg nobody reads
([`## Unreleased`](changelog.md#unreleased)).

### 1. What is already fixed

Four defects the compile break had been hiding, all of them real off Windows
too the moment a path or a `PATH` lookup came from somewhere unusual:
`command_exists` ignored `PATHEXT`, so nothing on `PATH` was ever found and
every project read as not *workable*; `missing_binding` decided what looked
like a path by testing for a leading slash, so a drive-lettered path read as a
bare command; the dossier handed a summoned command paths its own shell could
not parse, losing every structured answer quietly; and the seam's own test
helper built bindings the same way.

### 2. What is left

- **An out-of-process forge extension is a shell script.** [§FS-001-forge-interface.2](functional-spec/FS-001-forge-interface.md#2-two-transports-one-interface)
  says a shell script with `jq` is a complete implementation, and on Windows it
  is not one: `CreateProcess` cannot exec a file whose executability is a
  `#!` line. Either ephor reads the shebang and runs the extension through the
  interpreter it names, or the promise is narrowed to say where it holds. This
  is what `doctor_test` fails on now, 7 of 11.
- **Everything behind it.** `cargo test` stops at the first failing binary, so
  `work`, `feed`, `forge_extension`, `checkout`, `update` and the six e2e cases
  have never run on Windows at all. Each may be one line or another layer;
  nobody knows yet, and saying otherwise would be a guess wearing an estimate's
  clothes.

### 3. Measurable

`windows-latest` is back in the matrix and green, or [§FS-001-forge-interface.2](functional-spec/FS-001-forge-interface.md#2-two-transports-one-interface)
and [§FS-006-project-interface.3](functional-spec/FS-006-project-interface.md#3-a-summons-environment-in-exit-code-and-answer-out) say plainly which platforms they hold on and
this entry is closed as decided rather than done. A port half finished, with a
leg red and a promise that reads as universal, is the outcome this entry
exists to avoid.

## RM-005-scopes: a plan's scope is enforced, not merely written down

Serves [§FS-014-work-root-scopes](functional-spec/FS-014-work-root-scopes.md#fs-014-work-root-scopes-a-plan-lives-in-the-smallest-scope-that-can-see-everything-it-touches).
A work root exists at three scopes — organization, project, checkout — and a
plan belongs in the smallest one that can see everything the work touches. The
rule is written down now, and the mechanism under most of it has shipped
([`## Unreleased`](changelog.md#unreleased)). What is missing is any
construction that holds a reader to it:
[§FS-014-work-root-scopes.7](functional-spec/FS-014-work-root-scopes.md#7-what-is-not-yet-held)
names three parts of the rule that are this program's to enforce and are not
yet enforced, and this entry is where they are tracked.

Roughly half of what it tracks is not in this repository at all. That half is
listed anyway, because an entry that hides its other half reads as stalled when
it is only split.

### 1. What

**Landed.** A scope selector is honoured or refused rather than ignored, so a
verb addressed at one organization can no longer reach into another
(#41, PR #58). The organization is a placement tier of its own, with `{org}`
and `{org_root}` as placeholders and a work-root walk that finds what was laid
there — which is what makes an organization root a dispatch target rather than
a directory written to and never read (#42, PR #59). And one live run per
checkout is enforced over the tree rather than over the root (#45, PR #62),
which is the guard
[§FS-014-work-root-scopes.3](functional-spec/FS-014-work-root-scopes.md#3-upper-scopes-decide-the-checkout-scope-executes)
rests on: it is what makes handing a ticket *down* into a busy checkout a file
that waits, instead of a second agent in a working tree somebody is already in.

**Owed here.** Four, of which three are the ones the declaration names against
itself:

- **Placement is chosen per project, not per entry** (#43). One project's
  dispatches are placed by one answer, so a project cannot send fixes to minted
  checkouts and sweeps to its own root at the same time. Until it can,
  [§FS-014-work-root-scopes.2](functional-spec/FS-014-work-root-scopes.md#2-reach-places-and-nothing-else-does)
  is a rule a person applies by configuring one scope at a time.
- **A mutating verb above a checkout does not report before it acts** (#44).
  [§FS-014-work-root-scopes.3](functional-spec/FS-014-work-root-scopes.md#3-upper-scopes-decide-the-checkout-scope-executes)
  says upper scopes decide and hand down; nothing refuses a verb that would act
  at organization or project scope instead, so the division holds only because
  whoever configured the work kept to it.
- **A handed-down ticket does not name where it came from, and no issue tracks
  that.**
  [§FS-014-work-root-scopes.4](functional-spec/FS-014-work-root-scopes.md#4-a-handed-down-ticket-names-where-it-came-from-and-the-trail-runs-both-ways)
  is owed in both directions — neither the origin on the ticket nor the spawned
  ids on the result is written by anything — and it is the one of the three with
  no ticket behind it. Opening that issue is part of this entry, not a
  precondition for it.
- **`ephor checkout` accepts any branch name** (#46), `panta` included — which
  puts a working tree exactly on top of the work root whose plans that branch's
  work reads — and `../`, which leaves the project altogether. It is not one of
  the three, and it belongs here anyway: a placement rule whose checkout verb
  can land a tree on a work root is a rule the program itself can break.

**Owed elsewhere.** The other half is the directory the projects are checked out
under, which is a repository of its own and not this one. Four things there are
what make the three scopes real rather than describable: an organization root
has to exist as an ordinary flat runtime project; the project-scope plans parked
in checkouts have to move to the project roots they describe; those project
roots have to be tracked by that directory's own repository, since a root
nothing tracks is a root that does not survive a clean; and the script that
reclaims a finished worktree has to fold that run's plan and result up into a
longer-lived scope before removing the tree, which is
[§FS-014-work-root-scopes.5](functional-spec/FS-014-work-root-scopes.md#5-nothing-durable-lives-in-a-checkout-work-root)
with nothing implementing it today. None of the four is a change to ephor, and
none of them can be closed from here.

### 2. Why now

Three open issues have been citing "the three-pantas plan" with nothing in the
tree to point at, and the rule they were citing is the kind that decays quietly:
a plan in the wrong root is not an error anybody sees, it is work whose reach
stops being readable off where it sits. Writing it down is what makes the drift
nameable; the items above are what would make it impossible.

It is last in this file rather than first because most of what it is waiting on
is not here — not because the rule is optional.
[§FS-014-work-root-scopes.7](functional-spec/FS-014-work-root-scopes.md#7-what-is-not-yet-held)
is the honest reading of where it stands, and it is written into the
declaration itself so that the gap travels with the rule.

### 3. Measurable

In this repository: #43, #44 and #46 close, and an issue exists for the origin
trail and closes with it, so a handed-down ticket names the plan and ticket
that laid it and the handing-down work's result names the ids it spawned.
Outside this repository: an organization root exists and holds org-scope work,
no project-scope plan is parked in a checkout, the project roots are tracked
where they live, and a reclaimed worktree's record is found in a longer-lived
scope after the tree is gone.

When only that second set is left, this entry closes here and says where the
rest lives. Half of what it tracks is not this repository's to close, and an
entry that waits forever on another tree is an entry nobody reads.
