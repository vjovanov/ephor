# AR-004-forest: git is the substrate, and a project is a forest folded over

Git is assumed; no other version control exists for ephor — a git forest on
disk is the one thing a project is required to *be* (§REQ-001-boundary.3).
A project's place is a **Forest**: an ordered set of repositories
`{ name, path, remote, main, role }` under a root — the GraalVM shape (a
thin workspace repository composing ce and ee) is the general case, a
single repository a forest of one (§FS-006-project-interface.1). The layout
comes from the registry row, with the manifest's `forest` adopted where the
row does not override.

## 1. Folds

Every git-facing feature is a fold over the forest, per-repository answers
aggregated and reported per repository — never silently collapsed:

- **staleness** — sum of `rev-list --count HEAD..origin/<main>` over repos;
- **rebase** — fetch and replay each repo, nothing stashed, a conflict
  stopping that repo and reported as it (§FS-004-quick-actions.6);
- **checkout** — a working tree per repo under the workspace directory,
  branch where the forge has it, grown from main where not
  (§FS-004-quick-actions.7);
- **land** — push each repository of the workspace;
- **gate counts** — the per-repository breakdown a gate event carries
  (§FS-006-project-interface.6).

## 2. Probes, not declarations

Because git is assumed, facts are derived rather than configured: which
branches exist, which branch workspaces are on disk, what ticket key a
branch name carries, how far a checkout trails. The registry row keeps only
what probing cannot find — where the root is, the workspace template, and
overrides. A fact that can be probed and is also declared is probed anyway;
the declaration only says where to look.

## 3. Workspace resolution

`workspace(project, branch)` — the row's template applied to the branch —
is one function with one answer, used by the tree's grouping, the summons
executor's place resolution (§AR-002-summons.1), dispatch's plan placement
(§FS-005-dispatch.3), and the checkout offer. Two resolvers would
eventually disagree about where a branch lives, and everything above them
assumes they cannot.

It runs backwards too, and that is what fills the branch table
([§2](#2-probes-not-declarations)): the template split at `{branch}` gives a
prefix and a suffix, and a directory under the workspace base that sits
between them names a branch (§FS-008-attribution.2). So the branches ephor
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
