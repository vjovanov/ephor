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
