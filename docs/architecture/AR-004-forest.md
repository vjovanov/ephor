# AR-004-forest: git is the substrate, and a project is a forest folded over

Git is assumed; no other version control exists for ephor — a git forest on
disk is the one thing a project is required to *be* ([§REQ-001-boundary.3](../requirements/REQ-001-boundary.md#3-requirements-on-a-project-are-capabilities-never-artifacts)).
A project's place is a **Forest**: an ordered set of repositories
`{ name, path, remote, main, role }` under a root — the GraalVM shape (a
thin workspace repository composing ce and ee) is the general case, a
single repository a forest of one ([§FS-006-project-interface.1](../functional-spec/FS-006-project-interface.md#1-the-three-homes)). The layout
comes from the registry row, with the manifest's `forest` adopted where the
row does not override.

## 1. Folds

Every git-facing feature is a fold over the forest, per-repository answers
aggregated and reported per repository — never silently collapsed:

- **staleness** — commits each repo's `HEAD` trails its base, counted
  against the last-fetched `<remote>/<base>` of that repo and summed;
- **standing** — where each repo's checked-out branch is published and how
  far `HEAD` sits from that copy, one `for-each-ref` per repo answering
  ref, upstream and ahead/behind at once; the published copy is resolved by
  [§DA-003-upstream-is-the-published-copy](../decisions/architectural/DA-003-upstream-is-the-published-copy.md#da-003-upstream-is-the-published-copy-a-branchs-upstream-is-its-published-copy-not-its-tracking-config), the branch is `HEAD`'s and never
  the workspace directory's name, and a branch never pushed is an answer,
  not an error;
- **rebase** — fetch and replay each repo, nothing stashed, a conflict
  stopping that repo and reported as it ([§FS-004-quick-actions.6](../functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)); what it
  replays onto is one base for the whole forest, or each repo's own
  published copy — a different ref per repo, with one that has published
  nothing reported as such rather than refused
  ([§FS-004-quick-actions.8](../functional-spec/FS-004-quick-actions.md#8-a-branch-that-trails-its-own-published-copy-is-offered-the-rebase-onto-it));
- **checkout** — a working tree per repo under the workspace directory,
  branch where the forge has it, grown from main where not
  ([§FS-004-quick-actions.7](../functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout));
- **land** — push each repository of the workspace;
- **gate counts** — the per-repository breakdown a gate event carries
  ([§FS-006-project-interface.6](../functional-spec/FS-006-project-interface.md#6-the-gate-is-the-projects-in-three-verbs)).

A repository the layout declares and the disk has not got is **named by every
fold and fails one**. Named, because the alternative is the collapse this
section forbids: the summary says how many are not on disk and the report says
which, so no answer reads as if the workspace held fewer repositories than the
reader has. Not fatal to a fold over what is there, because an exit code routes
an outcome to whoever acts next — `3` sends a conflict to an agent, `1` sends
uncommitted work to a person who clears it and retries — and this routes
nowhere: retrying replays no repository that is not on disk, the missing tree
holds none of the change, and the condition was as true before the command ran
as after. It is a fact about the checkout, not an outcome of the run.

`checkout` is where it does fail, and that is the same rule rather than an
exception: there, a declared repository that is not on disk is exactly the
outcome the command was asked to change ([§FS-004-quick-actions.7](../functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)), so its exit
code is the one that answers *is this workspace whole* — which is also what
gives a caller a way to ask at all. So `ephor checkout` on a workspace that
already exists folds over the layout instead of stopping at the directory: it
makes the repositories that are missing, reports the ones already there as
already there, and fails on what it could not make. Asking the question and
fixing what it finds are one move. `ephor update`, which maintains a managed
workspace rather than folding over one, belongs to the same family and already
behaves this way: a repository the layout marks `required` and the disk has not
got is an error there.

The ethic that a partial answer must never be reported as success
([§FS-001-forge-interface.6](../functional-spec/FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)) is not weakened by this. That rule is about an
answer that cannot show its own gap — an empty feed section is indistinguishable
from a source that was never read, so only the exit code can carry it. This gap
is in the answer, by name, per repository, every time it is folded over.

## 2. Probes, not declarations

Because git is assumed, facts are derived rather than configured: which
branches exist, which branch workspaces are on disk, what ticket key a
branch name carries, how far a checkout trails, which remote each
repository fetches from and pushes to, what base each is measured against,
and where each checked-out branch is published. The registry row keeps only
what probing cannot find — where the root is, the workspace template, and
overrides. A fact that can be probed and is also declared is probed anyway;
the declaration only says where to look.

## 3. Workspace resolution

`workspace(project, branch)` — the row's template applied to the branch —
is one function with one answer, used by the tree's grouping, the summons
executor's place resolution ([§AR-002-summons.1](AR-002-summons.md#1-resolving-the-place)), dispatch's plan placement
([§FS-005-dispatch.3](../functional-spec/FS-005-dispatch.md#3-one-rhei-per-item-one-ticket-per-dispatch)), and the checkout offer. Two resolvers would
eventually disagree about where a branch lives, and everything above them
assumes they cannot.

It runs backwards too, and that is what fills the branch table
([§2](#2-probes-not-declarations)): the template split at `{branch}` gives a
prefix and a suffix, and a directory under the workspace base that sits
between them names a branch ([§FS-008-attribution.2](../functional-spec/FS-008-attribution.md#2-two-stages-one-engine)). So the branches ephor
places items under are the row's, then every workspace found on disk that the
row does not already name. Naming a discovered branch by its directory rather
than by its checkout's `HEAD` is what keeps the one answer one: read `HEAD`
instead and a tree at `…/GR-1` holding `you/GR-1-retry` would resolve forward
to `…/GR-1-retry`, a directory it is not in.

A workspace is recognized by holding a repository of the layout, tested
through `.git` rather than by asking git. Discovery looks at every directory
in the workspace area on every load, and a process per directory is a
subprocess storm to answer what a path already answers. The walk is bounded —
a branch name may carry separators, so a workspace sits a few components down
— and stops descending the moment it finds one, because what is under a
workspace is its repositories, not more workspaces.
