# Roadmap

What ephor plans to ship next, in priority order. Each item has a stable ID —
`RM-NNN-slug` — and may be cited from anywhere: commits, PRs, the changelog,
other specs. Shipped items move their detail to
[docs/changelog.md](changelog.md) and keep a one-line pointer here so the
citation does not dangle.

## RM-001-forge-interface: put every forge behind the interface

Implements [§FS-001-forge-interface](../requirements.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface).
Most of it has landed — see `## Unreleased` in [docs/changelog.md](changelog.md)
— and what remains is the last mile before anything is published.

### 1. What

**Landed.** The `Forge` trait carries the capability set of
[§FS-001-forge-interface.1](../requirements.md#1-capabilities) and both
transports answer it: GitHub in the default build as the reference
implementation, and anything else as an executable on `PATH`
([§FS-001-forge-interface.2](../requirements.md#2-two-transports-one-interface)),
with policy above both. The committed registry and feed configuration are
examples, the packaging exclude keeps the real ones out of any artifact, the
inherited `docs/` set is gone, and no employer or vendor identifier remains in
source, tests, examples, or documentation
([§FS-001-forge-interface.5](../requirements.md#5-no-site-specific-data-in-the-repository)) —
`scripts/check-no-site-specific.sh` passes both halves today. The vendor CLI
name is confined to its own adapter and held there by the build
([§REQ-001-boundary.5](requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).

**Owed.** The end-to-end proof: a run configured with nothing but the example
configuration, against a public GitHub repository, producing a feed — which is
what says the examples are a starting point rather than a shape. And the first
release itself, which has to be tagged by hand before the bump workflows have
anything to count from ([§FS-002-release](../requirements.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change)).

The boundary half of the same law — the seams a capability is reached across,
rather than the forges reached through them — is
[§RM-003-boundary](#rm-003-boundary-the-seams-the-law-still-owes).

### 2. Why now

It is the last thing between the tree and a first release. It is also what
makes ephor a tool rather than one person's script — the capability set is the
same for GitHub, GitLab, Bitbucket, Forgejo, Jira, and Linear, and only the
transport differs.

### 3. Measurable

`scripts/check-no-site-specific.sh` passes on a clean tree (it does today), the
GitHub implementation answers every capability in
[§FS-001-forge-interface.1](../requirements.md#1-capabilities), and a run
configured with only example configuration produces a working feed against a
public GitHub repository.

## RM-003-boundary: the seams the law still owes

Serves [§REQ-001-boundary](requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-does-not-embody-is-reached-across-a-seam).
The law landed with the interface it describes — the summons executor, the
capability table, the verb seams, the runtime binding, the manifest, and a
literal-confinement check that fails the build
([`## Unreleased`](changelog.md#unreleased)). Four things it names are not
finished, and each is small enough to say exactly.
[§RM-001-forge-interface](#rm-001-forge-interface-put-every-forge-behind-the-interface)
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
  ([§FS-006-project-interface.6](../requirements.md#6-the-gate-is-the-projects-in-three-verbs),
  [§E2E-003-gate-verbs](../e2e/cases/E2E-003-gate-verbs.rs)).
- **One ticket store is recognized and unread.** `beads` is probed and reports
  nothing rather than pretending
  ([§FS-006-project-interface.7](../requirements.md#7-local-ticket-stores-are-read-where-they-live)).
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

Serves [§FS-005-dispatch.2](../requirements.md#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it).

### 1. What

No provider records the text an item opens with — a pull request's description,
an issue's report. The interface's `PullRequest` and `Issue`
([§FS-001-forge-interface.1](../requirements.md#1-capabilities)) carry a title
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

Serves [§FS-001-forge-interface.2](../requirements.md#2-two-transports-one-interface)
and [§FS-006-project-interface.3](../requirements.md#3-a-summons-environment-in-exit-code-and-answer-out).
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

- **An out-of-process forge extension is a shell script.** §FS-001-forge-interface.2
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

`windows-latest` is back in the matrix and green, or §FS-001-forge-interface.2
and §FS-006-project-interface.3 say plainly which platforms they hold on and
this entry is closed as decided rather than done. A port half finished, with a
leg red and a promise that reads as universal, is the outcome this entry
exists to avoid.
