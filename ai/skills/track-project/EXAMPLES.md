# Track Project - Examples

Quick reference for common project tracking scenarios.

## Quick Start

### Add a Simple Monorepo

```bash
/track-project my-library
```

**Prompts you for:**
- Organization (e.g., "personal")
- Type (select "monorepo")
- Root path (e.g., "$HOME/code/my-library")
- Main branch (e.g., "main")
- Initialize? (yes/no)

**Result:**
- Adds project to `~/.config/ephor/workspaces.json`
- Creates directory (if init)
- Generates AGENTS.md (if init)

---

### Add Widget-Related Project

```bash
/track-project --org acme --type monorepo acme-native-plugin
```

**Prompts you for:**
- Root path
- Main branch
- Tags (suggest: "acme,repo")

**Uses defaults:**
- Organization: acme
- Type: monorepo
- Clone mode: full-clone

---

### Add Existing Project (Don't Initialize)

```bash
/track-project --org acme --path ~/code/existing-project --type monorepo existing-project
```

**Effect:**
- Adds to registry
- Does NOT create directories (already exists)
- Does NOT initialize git
- Generates AGENTS.md only

---

## Common Scenarios

### Personal Side Project

```bash
/track-project my-app
```

**Typical values:**
- Organization: `personal` (create if doesn't exist)
- Type: `monorepo`
- Root: `$HOME/code/my-app`
- Main branch: `main`
- Clone mode: `full-clone`
- Tags: `personal`
- Initialize: `yes`

---

### Work Repository

```bash
/track-project --org acme --tags acme,repo work-tools
```

**Typical values:**
- Organization: `acme`
- Type: `monorepo`
- Root: `$USER_CODE/work-tools`
- Main branch: `master`
- Clone mode: `full-clone`
- Tags: `acme,repo`

---

### Multi-Repo Workspace

```bash
/track-project --org acme --type product-workspace experimental-workspace
```

**Important:**
- Type must be `product-workspace`
- Will show clone instructions instead of initializing
- Expects `ce/` and `ee/` subdirectories

**Instructions shown:**
```
To initialize this workspace, clone the required repositories:

cd $USER_CODE/experimental-workspace/master
git clone git@github.com:acme/widget.git ce
git clone git@github.com:acme/widget-enterprise.git ee

Then run:
ephor ensure-agents --workspace experimental-workspace-master
```

---

### With Worktree Mode

```bash
/track-project --org acme --clone-mode worktree --path ~/code/multi-branch multi-branch-project
```

**When to use worktree mode:**
- You'll have many active branches
- Disk space is limited
- You want to share `.git` across branches

**Note:** Initialization will create the main worktree structure.

---

## Command Line Options

| Option | Description | Example |
|--------|-------------|---------|
| `--org ORG` | Organization | `--org acme` |
| `--type TYPE` | Project type | `--type monorepo` |
| `--path PATH` | Root directory | `--path ~/code/myapp` |
| `--tags TAG1,TAG2` | Project tags | `--tags acme,repo` |
| `--clone-mode MODE` | Clone mode | `--clone-mode worktree` |
| `--init` | Force initialization | `--init` |
| `--no-init` | Skip initialization | `--no-init` |

---

## Registry Entry Generated

For a typical project, the skill generates:

```json
{
  "id": "my-project",
  "organization": "personal",
  "type": "monorepo",
  "display_name": "My Project",
  "root": "$HOME/code/my-project",
  "main_branch": "main",
  "clone_mode": "full-clone",
  "tags": ["personal"],
  "branches": []
}
```

---

## After Adding a Project

### Verify it was added:
```bash
ephor list
```

### Validate registry:
```bash
ephor validate
```

### Add branches later:
Edit `~/.config/ephor/workspaces.json` and add to `branches` array:
```json
{
  "branches": [
    {
      "id": "my-project-feature-x",
      "branch": "feature/new-ui",
      "active": true,
      "display_name": "My Project - New UI"
    }
  ]
}
```

### Update the workspace:
```bash
ephor --workspace my-project update
```

---

## Error Recovery

### If validation fails:

The skill will show the error and offer options:
1. Fix automatically (if possible)
2. Edit manually
3. Rollback changes

### If directory already exists:

The skill will:
1. Detect existing directory
2. Check if it's a git repo
3. Ask whether to:
   - Use existing directory
   - Abort
   - Choose different path

### If project ID conflicts:

The skill will:
1. Show the conflict
2. Suggest alternative ID
3. Ask user to choose different ID

---

## Tips

1. **Use environment variables** for paths:
   - `$HOME/code/...` for personal projects
   - `$USER_CODE/...` for work projects

2. **Follow naming conventions**:
   - Project IDs: lowercase-with-hyphens
   - Display names: Proper Case With Spaces

3. **Tag appropriately**:
   - Use `acme` for Widget-related
   - Use `repo` for single repositories
   - Use `monorepo` for monorepos
   - Use `personal` for personal projects

4. **Choose clone mode wisely**:
   - `full-clone`: Simple, isolated, more disk space
   - `worktree`: Efficient, shared .git, for many branches

5. **Initialize selectively**:
   - Initialize for new projects
   - Don't initialize for existing projects
   - Don't initialize product-workspace (clone repos manually)

---

## See Also

- [SKILL.md](SKILL.md) - Full skill documentation
- [../../$EPHOR_HOME/README.md](../../$EPHOR_HOME/README.md) - Project system overview
- [../../$EPHOR_HOME/docs/registry.md](../../$EPHOR_HOME/docs/registry.md) - What the registry's concepts mean
- [../../projects/ORGANIZATIONS-AND-BRANCHING.md](../../projects/ORGANIZATIONS-AND-BRANCHING.md) - Org and branch details
