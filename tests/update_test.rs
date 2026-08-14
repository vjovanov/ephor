mod common;

use std::fs;

use serde_json::json;

use common::*;

const FAKE_GIT: &str =
    "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$*\" >> \"${FAKE_GIT_LOG:?}\"\n";

#[test]
fn update_defaults_to_release_branches_plus_active_project_branches() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let fake_bin = tmp.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let log_file = tmp.path().join("git.log");
    make_executable(&fake_bin.join("git"), FAKE_GIT);

    let workspace_root = tmp.path().join("g");
    for branch in ["master", "release/2.3", "release/2.5"] {
        for repo in ["app", "plugins", "docs-site"] {
            fs::create_dir_all(workspace_root.join(branch).join(repo).join(".git")).unwrap();
        }
    }
    let active_root = tmp.path().join("active");
    fs::create_dir_all(active_root.join(".git")).unwrap();

    let registry_path = tmp.path().join("workspaces.json");
    write_registry(
        &registry_path,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                required_project(&workspace_root),
                {
                    "id": "demo",
                    "type": "monorepo",
                    "display_name": "demo",
                    "root": active_root.to_string_lossy(),
                    "main_branch": "main",
                    "branches": [
                        { "id": "demo-active", "branch": "main", "active": true },
                        {
                            "id": "demo-inactive",
                            "branch": "feature",
                            "active": false,
                            "vars": { "branch": "feature" },
                            "repo_overrides": { "repo": { "branch": "feature" } }
                        }
                    ]
                }
            ]
        }),
    );

    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );
    ephor_cmd()
        .env("PATH", path)
        .env("FAKE_GIT_LOG", log_file.to_str().unwrap())
        .args(["--registry", registry_path.to_str().unwrap(), "update"])
        .assert()
        .success();

    let log = fs::read_to_string(&log_file).unwrap();
    assert!(log.contains("fetch origin master"), "{log}");
    assert!(log.contains("fetch origin release/2.3"), "{log}");
    assert!(log.contains("fetch origin release/2.5"), "{log}");
    assert!(
        log.contains(&format!("-C {}", active_root.display())),
        "{log}"
    );
    assert!(!log.contains("fetch origin feature"), "{log}");
}

#[test]
fn update_skip_agents_does_not_regenerate_agents_file() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let fake_bin = tmp.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let log_file = tmp.path().join("git.log");
    make_executable(&fake_bin.join("git"), FAKE_GIT);

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(workspace_root.join(".git")).unwrap();
    let agents_path = workspace_root.join("AGENTS.md");
    fs::write(&agents_path, "existing tracked guidance\n").unwrap();

    let registry_path = tmp.path().join("workspaces.json");
    write_registry(
        &registry_path,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                {
                    "id": "demo",
                    "type": "monorepo",
                    "display_name": "demo",
                    "root": workspace_root.to_string_lossy(),
                    "main_branch": "main",
                    "branches": [ { "id": "demo-main", "branch": "main", "active": true } ]
                }
            ]
        }),
    );

    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );
    ephor_cmd()
        .env("PATH", path)
        .env("FAKE_GIT_LOG", log_file.to_str().unwrap())
        .args([
            "--registry",
            registry_path.to_str().unwrap(),
            "update",
            "--skip-agents",
        ])
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&agents_path).unwrap(),
        "existing tracked guidance\n"
    );
}

#[test]
fn update_runs_data_driven_hooks() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let fake_bin = tmp.path().join("bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let git_log = tmp.path().join("git.log");
    make_executable(&fake_bin.join("git"), FAKE_GIT);
    let hook_log = tmp.path().join("hook.log");
    make_executable(
        &fake_bin.join("record-hook"),
        "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s %s\\n' \"$PWD\" \"$*\" >> \"${HOOK_LOG:?}\"\n",
    );

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(workspace_root.join(".git")).unwrap();

    let mut project_types = base_project_types(&template);
    project_types[0]["agents"]["update_hooks"] = json!({ "pre": ["record"], "post": [] });

    let registry_path = tmp.path().join("workspaces.json");
    write_registry(
        &registry_path,
        &json!({
            "project_types": project_types,
            "hook_sets": [
                {
                    "id": "record",
                    "hooks": [
                        { "command": ["record-hook", "branch={branch}", "ws={workspace_id}"], "pass_debug": true }
                    ]
                }
            ],
            "projects": [
                {
                    "id": "demo",
                    "type": "monorepo",
                    "display_name": "demo",
                    "root": workspace_root.to_string_lossy(),
                    "main_branch": "main",
                    "branches": [ { "id": "demo-main", "branch": "main", "active": true } ]
                }
            ]
        }),
    );

    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );
    ephor_cmd()
        .env("PATH", path)
        .env("FAKE_GIT_LOG", git_log.to_str().unwrap())
        .env("HOOK_LOG", hook_log.to_str().unwrap())
        .args([
            "--registry",
            registry_path.to_str().unwrap(),
            "update",
            "--debug",
            "--skip-agents",
        ])
        .assert()
        .success();

    let log = fs::read_to_string(&hook_log).unwrap();
    assert!(log.contains("branch=main ws=demo-main --debug"), "{log}");
    assert!(
        log.starts_with(&workspace_root.to_string_lossy().into_owned()),
        "{log}"
    );
}
