# DA-003-upstream-is-the-published-copy: a branch's upstream is its published copy, not its tracking config

**Status:** Accepted
**Date:** 2026-08-14

The upstream of a branch, as ephor means it, is the branch's **published
copy**: the remote ref holding what was last pushed of it. It is resolved per
repository from that repository's own `HEAD` — never from the workspace
directory's name, because a repository need not be on the branch its
directory is named for ([§AR-004-forest.1](../../architecture/AR-004-forest.md#1-folds)) — and in this order:

1. `@{upstream}` where git records one **and it does not name this
   repository's base**;
2. otherwise `<remote>/<HEAD>` where the remote has a branch of that name —
   the untracked-but-pushed shape `git worktree add -b` leaves behind;
3. otherwise the branch is **unpushed**. An answer, not an error: a branch
   never published has no copy to measure against, so nothing shows a
   distance and nothing offers a replay onto it.

## 1. The rejected alternative

Taking `@{upstream}` at face value. git's `branch.autoSetupMerge` records
where a branch was **cut**, not where it is published: a branch started from
`origin/main` carries `origin/main` as its upstream until its first push —
on the `~/c/g` root repository that is exactly the answer `@{upstream}`
gives. Read at face value, "rebase onto upstream" would silently duplicate
the rebase onto the project's main branch sitting next to it in the menu
([§FS-004-quick-actions.6](../../functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)): two entries, one operation, and no way for the
reader to tell which fact either acted on. The exclusion in step 1 is the
whole decision — tracking config that names this repository's base is a
record of where the branch came from, and where a branch came from is what
the base fold already measures ([§AR-004-forest.1](../../architecture/AR-004-forest.md#1-folds)).

## 2. Why the ref is resolved here rather than left to git

Bare `git rebase` leans on the tracking config and fails outright on a
branch that has none — which is the ordinary state of a pushed branch in a
worktree-grown workspace (`ce` and `ee` in
`~/c/g/vj/GR-73955-condition-errors` are the shape). Step 2 recovers the
published copy those repositories actually have, from the remote-tracking
refs git already keeps, so the fact is measurable exactly where git's own
shorthand gives up. A fact that can be probed is probed ([§AR-004-forest.2](../../architecture/AR-004-forest.md#2-probes-not-declarations));
the tracking config is one witness, not the authority.

## 3. The cost

Ephor second-guesses git's own bookkeeping. A person who deliberately set
their branch's upstream to the base — to make bare `git pull` fold main in,
say — will see ephor call their branch's publication something else: the
pushed copy of the same name, or unpushed. And a tracking upstream whose
remote ref has been deleted (`[gone]`) is read the same way, falling through
to step 2. Both are the price of the menu never carrying the same rebase
twice under two names.
