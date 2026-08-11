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

## Hook sets

Named lists of commands run around `ephor update`. An entry is either the
built-in string `"print-debug-context"` or a command object:

```json
{ "command": ["${TOOLS}/bin/build", "update"], "cwd": "{workspace_root}", "pass_debug": true }
```

Arguments expand `{context}` placeholders first, then `$VAR` / `${VAR}`.
