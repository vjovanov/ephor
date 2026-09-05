---
name: track-project
description: Add a new project to the registry and initialize its workspace
user-invocable: true
allowed-tools: Read Write Edit Bash AskUserQuestion
argument-hint: "[--org ORG] [--type TYPE] [--path PATH] [project-name]"
---

Add a new project to the project registry and optionally initialize its workspace.

## What This Skill Does

1. **Gathers Project Information**: Asks for or uses provided project details
2. **Updates Registry**: Adds project to `~/.config/ephor/workspaces.json`
3. **Validates**: Ensures the registry is valid
4. **Initializes** (optional): Creates directory structure and generates AGENTS.md

## Usage

```bash
# Interactive mode (asks questions)
/track-project

# With project name
/track-project my-new-project

# With full details
/track-project --org acme --type monorepo --path ~/code/my-project my-project
```

## Instructions

### Step 1: Gather Project Information

If not provided via arguments, use `AskUserQuestion` to gather:

**Required Information:**
- **Project ID**: Unique identifier (lowercase, hyphens, e.g., "my-project")
- **Display Name**: Human-readable name (e.g., "My Project")
- **Organization**: Which org it belongs to (e.g., "widget", "personal")
- **Type**: Project type - either "monorepo" or "product-workspace"
- **Root Path**: Base directory (e.g., "$HOME/code/my-project" or "$USER_CODE/project")
- **Main Branch**: Primary branch name (e.g., "master", "main", "trunk")

**Optional Information:**
- **Clone Mode**: "worktree" or "full-clone" (default: "full-clone")
- **Tags**: Comma-separated tags (e.g., "acme,repo")
- **Initialize**: Whether to create directories and AGENTS.md (yes/no)

### Step 2: Parse Arguments

Parse `$ARGUMENTS` for:
- `--org ORG`: Organization
- `--type TYPE`: Project type
- `--path PATH`: Root path
- `--tags TAG1,TAG2`: Tags
- `--clone-mode MODE`: Clone mode
- `--init`: Initialize workspace
- Remaining argument: Project ID/name

### Step 3: Validate Organization Exists

1. Read `~/.config/ephor/workspaces.json`
2. Check if specified organization exists in `organizations` array
3. If not, ask if user wants to create it

### Step 4: Update Registry

1. Read current `~/.config/ephor/workspaces.json`
2. Create new project entry:
```json
{
  "id": "project-id",
  "organization": "org-name",
  "type": "monorepo",
  "display_name": "Project Name",
  "root": "$HOME/code/project",
  "main_branch": "main",
  "clone_mode": "full-clone",
  "tags": ["tag1", "tag2"],
  "branches": []
}
```
3. Add to `projects` array
4. Write updated registry

### Step 5: Validate Registry

Run validation to ensure registry is correct:
```bash
ephor validate
```

If validation fails, show error and ask user how to fix it.

### Step 6: Initialize Workspace (Optional)

If user requested initialization:

**For monorepo:**
1. Create root directory if it doesn't exist
2. Check if it's already a git repo
3. If not, ask if user wants to initialize git
4. Generate AGENTS.md:
```bash
ephor ensure-agents --workspace project-id
```

`ephor ensure-agents` writes `AGENTS.md` and nothing else. It never creates,
moves or rewrites a project's own tool configuration — where that lives is the
project's to decide. Where the project uses the toolchain, the layout is:

- `grund.toml` at the repository root, so a glance says it is a grounded tree
- `fissile.toml` under `.agent-grounds/`
- `.agents/` reserved for agent instructions, which a sandboxed runtime may
  mount read-only

`.agents/` is the deprecated former name for the first two: both still work,
and `ephor doctor` names a checkout still on the old name without failing it.

**For product-workspace:**
1. Create root directory if it doesn't exist
2. Show instructions for cloning repos:
```
To initialize this workspace, clone the required repositories:

cd /path/to/workspace
git clone <repo-url> ce
git clone <repo-url> ee

Then run:
ephor ensure-agents --workspace project-id
```

### Step 7: Summary

Show summary of what was done:
```
✅ Added project 'project-name' to registry
   Organization: org-name
   Type: monorepo
   Root: /path/to/project
   Main branch: main

✅ Validated registry (13 workspaces)

✅ Generated AGENTS.md (if initialized)

Next steps:
- Add branches: Edit ~/.config/ephor/workspaces.json
- Update workspace: ephor --workspace project-id update
- List all projects: ephor list
```

## Examples

### Example 1: Add Simple Monorepo

User runs: `/track-project my-lib`

Skill asks:
- Organization? → "personal"
- Type? → "monorepo"
- Root path? → "$HOME/code/my-lib"
- Main branch? → "main"
- Initialize? → "yes"

Result:
- Adds project to registry
- Creates directory
- Generates AGENTS.md

### Example 2: Add Widget Workspace

User runs: `/track-project --org acme --type product-workspace acme-experimental`

Skill asks:
- Root path? → "$USER_CODE/g/experimental"
- Main branch? → "master"
- Initialize? → "no"

Result:
- Adds project to registry
- Shows clone instructions
- No initialization (user will clone repos manually)

### Example 3: Add with All Options

User runs: `/track-project --org acme --type monorepo --path ~/code/tools --tags acme,tools --clone-mode worktree --init tools`

Result:
- Uses all provided arguments
- Creates project with worktree mode
- Initializes immediately

## Error Handling

**If organization doesn't exist:**
- Ask user if they want to create it
- If yes, gather org info (name, description)
- Add to organizations array

**If project ID already exists:**
- Show error
- Ask for different ID

**If path already exists and is not empty:**
- Warn user
- Ask if they want to continue

**If validation fails:**
- Show validation error
- Offer to fix or rollback changes

## Best Practices

1. **Always validate** after making changes
2. **Use environment variables** for paths ($HOME, $USER_CODE)
3. **Suggest sensible defaults** based on project type
4. **Don't initialize** for product-workspace unless repos exist
5. **Show clear next steps** after completion

## Related Files

- Registry: `~/.config/ephor/workspaces.json`
- Schema: `$EPHOR_HOME/assets/workspaces.schema.json`
- CLI: `ephor` (installed via `cargo install --path $EPHOR_HOME`)
- Docs: `$EPHOR_HOME/README.md`, `$EPHOR_HOME/docs/registry.md`
- Examples: `ai/skills/track-project/EXAMPLES.md` - Quick reference and common scenarios
