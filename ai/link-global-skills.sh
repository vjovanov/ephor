#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: link-global-skills.sh [--force] [--dry-run]

Link this repository's canonical skills from ai/skills/ into the global AI
skill directories for Claude, Codex, Antigravity, and Pi:

  ~/.claude/skills
  ~/.codex/skills
  ~/.antigravity/skills
  ~/.pi/skills

Options:
  --force    Replace conflicting existing paths.
  --dry-run  Show what would change without modifying anything.
  -h, --help Show this help text.
EOF
}

force=0
dry_run=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --force)
            force=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'Unknown argument: %s\n\n' "$1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
skills_dir="$repo_root/ai/skills"

if [[ ! -d "$skills_dir" ]]; then
    printf 'Skills directory not found: %s\n' "$skills_dir" >&2
    exit 1
fi

run() {
    if (( dry_run )); then
        printf 'DRY-RUN:'
        printf ' %q' "$@"
        printf '\n'
        return 0
    fi

    "$@"
}

link_path() {
    local agent_name="$1"
    local global_dir="$2"
    local target="$3"
    local entry_name="$4"
    local link_path

    link_path="$global_dir/$entry_name"

    run mkdir -p "$global_dir"

    if [[ -L "$link_path" ]]; then
        if [[ "$(readlink "$link_path")" == "$target" ]]; then
            printf '[ok] %s: %s already points to %s\n' "$agent_name" "$link_path" "$target"
            return 0
        fi

        if (( force )); then
            printf '[replace] %s: %s -> %s\n' "$agent_name" "$link_path" "$target"
            run rm -f "$link_path"
            run ln -s "$target" "$link_path"
            return 0
        fi

        printf '[skip] %s: %s points elsewhere (%s)\n' "$agent_name" "$link_path" "$(readlink "$link_path")" >&2
        return 1
    fi

    if [[ -e "$link_path" ]]; then
        if (( force )); then
            printf '[replace] %s: %s -> %s\n' "$agent_name" "$link_path" "$target"
            run rm -rf "$link_path"
            run ln -s "$target" "$link_path"
            return 0
        fi

        printf '[skip] %s: %s already exists and is not a symlink\n' "$agent_name" "$link_path" >&2
        return 1
    fi

    printf '[link] %s: %s -> %s\n' "$agent_name" "$link_path" "$target"
    run ln -s "$target" "$link_path"
}

status=0

for skill_path in "$skills_dir"/*; do
    [[ -d "$skill_path" ]] || continue
    skill_name="$(basename "$skill_path")"
    link_path "Claude" "$HOME/.claude/skills" "$skill_path" "$skill_name" || status=1
    link_path "Codex" "$HOME/.codex/skills" "$skill_path" "$skill_name" || status=1
    link_path "Antigravity" "$HOME/.antigravity/skills" "$skill_path" "$skill_name" || status=1
    link_path "Pi" "$HOME/.pi/skills" "$skill_path" "$skill_name" || status=1
done

exit "$status"
