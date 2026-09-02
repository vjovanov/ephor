# The registry

The registry is one JSON document describing every project you work on and
where its checkouts live. `assets/workspaces.schema.json` is the authority —
it is embedded in the binary and `ephor validate` enforces it — so this page
describes what the concepts *mean* rather than restating the schema.

Your own registry lives at `~/.config/ephor/workspaces.json`;
`config/workspaces.example.json` is a worked example of everything below.

## Four top-level sections

`organizations`, `project_types`, `projects`, `hook_sets`.

## Organizations

A grouping with an `id`, a `name`, an optional `root` (the directory its
projects are checked out under), and `default_tags` inherited by its projects.
The TUI groups the whole tree by organization first, showing each one's root.

## Project types

A type is the *shape* of a checkout, shared by every project using it.

- `layout` is `monorepo` (one repository) or `polyrepo` (several side by side).
- `repos[]` lists the repositories a workspace contains: an `id`, the `path`
  under the workspace root, a human `role`, whether it is `required`, and an
  `update_mode` of `branch` (follow the workspace branch) or `skip` (leave it
  alone — vendored or read-only checkouts). `default_branch` may interpolate
  `{branch}`.
- `agents` drives AGENTS.md rendering: the `template` path (resolved relative
  to the registry file), the `structure_intro` sentence, the summary templates
  used with and without a ticket, and `validations[]` — the commands an agent
  should run to check its work, each with a `cwd` and a `command`.
- `update_hooks` name the `hook_sets` to run before and after an update.

## Projects

A project picks a `type`, belongs to an `organization`, and has a `root`,
a `main_branch`, and `tags`. Paths expand `~`, `$VAR`, and `${VAR}`.

Two branch lists, which the TUI shows separately:

- `release_branches[]` — long-lived lines (`main`, `release/2.1`).
- `branches[]` — the work in progress, each with an `id`, the `branch` name,
  whether it is `active`, an optional `ticket` key, and a `display_name`. When
  `ticket` is absent it is inferred from the branch name: the first
  `UPPERCASE-digits` pair, so `you/ABC-42-retry` yields `ABC-42`. The ticket is
  what links a feed item to a branch, alongside the branch name itself.

`required_branch_ids` lists branch ids that must exist; `ephor validate` fails
when one is missing. This is how a project states "these release lines are not
optional" without the engine hardcoding any particular project's branches.

`branch_root_template` gives each branch its own workspace directory —
`{project_root}/{branch}` puts `you/ABC-42-retry` at
`$root/you/ABC-42-retry`. Without it, the project root *is* the single
checkout. `clone_mode` is `worktree` or `full-clone`, and decides how a new
branch workspace is created.

`repo_overrides`, `agents_overrides`, `vars`, and `aliases` let one project
depart from its type without forking the type.

## Identity and territory

A row does not only say where a project is; it says what the project *is*, and
that is what places a conversation nobody addressed to a repository
([§FS-008-attribution.1](functional-spec/FS-008-attribution.md#1-identity-is-declared-and-the-row-has-the-last-word)).

- `aliases[]` — other names the project answers to. A polyrepo whose
  repositories have their own names lists them here.
- `branches[].ticket`, and the key inferred from a branch name — the ticket
  patterns the project's matters carry.
- The repositories of its forest, which come from the type's `repos[]` and any
  `repo_overrides`.
- `territory[]` — repositories and organizations that are the project's
  business **without being in its forest**: `"acme/plugin"` for one repository,
  `"acme"` for a whole organization. It is what places the general case — a
  mention of you on some repository of the project's ecosystem, an issue filed
  there, a discussion opened there, none of it in any checkout.

Attribution weighs these against what a conversation carries: an explicit venue
wins outright, a reference places next, and resemblance only argues. Two
projects claiming the same thing equally is not settled by order — it goes to
the unattributed bucket carrying both, because a guess that lands wrong amends
someone else's row silently.

## What the row believes about a checkout

A project may describe itself in an `ephor.json` at its forest root
([§FS-006-project-interface.2](functional-spec/FS-006-project-interface.md#2-the-manifest-is-offered-never-required)),
and the row decides what that is worth:

- Identity fields in a manifest are **hints**. The row adopts one where it says
  nothing of its own and overrides it where it does — a checkout must not be
  able to claim another project's conversations.
- `manifest_trust` says how much of the rest to believe: `full` (the default —
  its commands run with the trust you extend to running the project's own
  build), `descriptions` (read what it says about itself, run none of it), or
  `ignore` (do not read it).

Nothing in a manifest can gate a capability that probing or your own
configuration could not establish alone, so a row that ignores one loses
nothing but the project's own convenience.

## Hook sets

Named lists of commands run around `ephor update`. An entry is either the
built-in string `"print-debug-context"` or a command object:

```json
{ "command": ["${TOOLS}/bin/build", "update"], "cwd": "{workspace_root}", "pass_debug": true }
```

Arguments expand `{context}` placeholders first, then `$VAR` / `${VAR}`.
