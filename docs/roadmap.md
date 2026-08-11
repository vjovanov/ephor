# Roadmap

What ephor plans to ship next, in priority order. Each item has a stable ID —
`RM-NNN-slug` — and may be cited from anywhere: commits, PRs, the changelog,
other specs. Shipped items move their detail to
[docs/changelog.md](changelog.md) and keep a one-line pointer here so the
citation does not dangle.

## RM-001-forge-interface: put every forge behind the interface

Implements [§FS-001-forge-interface](../requirements.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface).
Today ephor violates it: two providers shell out to a vendor CLI from core
source files, the committed `config/` names an employer's repositories and
accounts, and `docs/` is a set of inherited workflow documents from the
project this was extracted from. Nothing may be published until this lands.

### 1. What

Give the `Provider` trait the capability set of
[§FS-001-forge-interface.1](../requirements.md#1-capabilities) — pull requests
by role, conversation, reactions, gate status, issues — so a provider declares
what it answers and the feed degrades to that. Keep GitHub in the default build
as the reference implementation
([§FS-001-forge-interface.2](../requirements.md#2-two-transports-one-interface)).
Move the vendor-CLI-backed pull request, issue, and gate implementations out
of the default build
([§FS-001-forge-interface.4](../requirements.md#4-site-specific-implementations-ship-separately)),
with the vendor CLI name becoming a configured command rather than a literal in
source. Replace the committed registry and feed configuration with examples,
and add a packaging exclude so no artifact can carry the real ones
([§FS-001-forge-interface.5](../requirements.md#5-no-site-specific-data-in-the-repository)).
Rewrite or drop the inherited `docs/` set in the same pass.

### 2. Why now

It blocks distribution outright. The crate as it stands packages an employer's
internal repository layout, build commands, Bitbucket project keys, and a work
email address; a crates.io version can never be withdrawn once published. It is
also what makes ephor a tool rather than one person's script — the capability
set is the same for GitHub, GitLab, Bitbucket, Forgejo, Jira, and Linear, and
only the transport differs.

### 3. Measurable

A default build contains no occurrence of any employer or vendor identifier,
nor any host name, in source, tests, or packaged files — checked by
`scripts/check-private-words.sh` against a local word list, and by
`cargo package --list`. The GitHub implementation answers every capability in
[§FS-001-forge-interface.1](../requirements.md#1-capabilities). A run configured
with only example configuration produces a working feed against a public
GitHub repository.

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
