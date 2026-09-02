# FS-008-attribution: every conversation finds its project, or says that it could not

Conversations arrive from places that know nothing of the registry: a
mailbox serves every project a person has, a discussion sits on an adjacent
repository, a notice names a subject nobody configured. Attribution is
ephor's own move — deciding whose business a conversation is — and it is
data matching, never code: evidence the conversation carries against
identity the registry declares ([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).

## 1. Identity is declared, and the row has the last word

A project's identity is the set of signals by which its matters are
recognized: ticket patterns, the forest's repositories, the wider
**territory** the project claims — repositories and organizations that are
its business without being in its forest — names and aliases, addresses. It
lives in the registry row; a manifest may hint it
([§FS-006-project-interface.2](FS-006-project-interface.md#2-the-manifest-is-offered-never-required)), and the row adopts or overrides — a checkout
must not be able to claim another project's conversations. Territory is what
places the general case: a mention of the person on some repository of the
project's ecosystem, an issue filed there, a discussion opened there —
none of it in any forest, all of it the project's business
([§GOAL-003-nothing-lost](../goals.md#goal-003-nothing-lost-the-watch-is-trusted-enough-to-retire-the-sweep)).

## 2. Two stages, one engine

Attribution runs discussion → matter, then matter → project and branch. It
is one matching engine at two scopes: the branch matching that already
places items under a project's branches is this engine confined to one
project, and it is promoted, not duplicated.

**A project's branches are the row's and the disk's together.** The row names
the branches somebody wrote down; the workspaces are wherever branches were
actually checked out, and the two are not the same list — the same gap task
stores are read across ([§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)). A branch whose
workspace is on disk is one ephor can place work on, so it is one items are
placed under, whether or not the row names it. Anything else is ephor
contradicting itself about a fact it measured: a row reading `✓ checked out`
under a heading that says the item is linked to no branch, and — worse — a
checkout ephor made itself ([§FS-004-quick-actions.7](FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)) staying invisible to the
grouping the moment after it was made.

A branch found this way is named by the directory it was found in, never by
what its checkout has at `HEAD`, so the directory a branch resolves to and
the directory it was found in are always the same one. The row keeps the last
word on everything else about a branch — its ticket, whether it is active,
whether it is a release branch — and on identity, which no checkout may widen
([§1](#1-identity-is-declared-and-the-row-has-the-last-word)).

## 3. Venue beats reference beats resemblance

A discussion *on* a subject belongs to that subject's matter. A discussion
*naming* a subject — a ticket key in a mail's text, a pull request's URL in
a chat message — belongs to the named matter, linked onward
([§FS-007-matters.2](FS-007-matters.md#2-same-subject-one-matter-related-subjects-linked-matters)). Only where neither holds may declared aliases place a
conversation, and then as a topic matter, never onto an existing subject:
resemblance may start a new row, it may not amend one. At the second stage
the venue itself is the explicit signal: a matter whose subject sits on a
repository of a project's forest or declared territory
([§FS-008-attribution.1](FS-008-attribution.md#1-identity-is-declared-and-the-row-has-the-last-word)) is that project's before any reference or alias is
consulted.

## 4. Unattributed is a place, not a fate

A conversation that matched nothing lands in a visible unattributed bucket,
in the interactive view and on demand — never dropped. The bucket is the
attribution seam's degrade rule ([§REQ-001-boundary.1](../requirements/REQ-001-boundary.md#1-the-anatomy)): mapping failures are
seen where they can be fixed, by adding the signal the identity was
missing.

