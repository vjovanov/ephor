mod common;

use std::fs;

use predicates::prelude::*;
use serde_json::json;

use common::*;

#[test]
fn validate_fails_for_missing_active_root() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let registry_path = tmp.path().join("workspaces.json");
    let graal_root = tmp.path().join("g");
    for branch in ["master", "release/2.3", "release/2.5"] {
        for repo in ["app", "plugins", "docs-site"] {
            fs::create_dir_all(graal_root.join(branch).join(repo)).unwrap();
        }
    }

    write_registry(
        &registry_path,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                required_project(&graal_root),
                {
                    "id": "missing-project",
                    "type": "monorepo",
                    "display_name": "missing",
                    "root": tmp.path().join("does-not-exist").to_string_lossy(),
                    "main_branch": "main",
                    "branches": [ { "id": "missing-branch", "branch": "main", "active": true } ]
                }
            ]
        }),
    );

    ephor_cmd()
        .args(["--registry", registry_path.to_str().unwrap(), "validate"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Managed workspace 'missing-branch' root does not exist",
        ));
}

#[test]
fn validate_fails_when_required_branch_is_missing() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let registry_path = tmp.path().join("workspaces.json");

    write_registry(
        &registry_path,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                {
                    "id": "widget",
                    "type": "product-workspace",
                    "display_name": "Widget",
                    "root": tmp.path().join("g").to_string_lossy(),
                    "main_branch": "master",
                    "required_branch_ids": ["widget-2.3", "widget-2.5"],
                    "branch_root_template": "{project_root}/{branch}",
                    "release_branches": [
                        { "id": "widget-main", "branch": "master", "active": true },
                        { "id": "widget-2.3", "branch": "release/2.3", "active": true }
                    ],
                    "branches": []
                }
            ]
        }),
    );

    ephor_cmd()
        .args(["--registry", registry_path.to_str().unwrap(), "validate"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Project 'widget' is missing required branches: widget-2.5",
        ));
}

#[test]
fn list_prints_project_table() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let registry_path = tmp.path().join("workspaces.json");

    write_registry(
        &registry_path,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                {
                    "id": "demo",
                    "organization": "personal",
                    "type": "monorepo",
                    "display_name": "Demo",
                    "root": tmp.path().join("demo").to_string_lossy(),
                    "main_branch": "main",
                    "tags": ["demo-tag"],
                    "branches": [ { "id": "demo-main", "branch": "main", "active": true } ]
                }
            ],
            "organizations": [ { "id": "personal", "name": "Personal" } ]
        }),
    );

    ephor_cmd()
        .args(["--registry", registry_path.to_str().unwrap(), "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Project"))
        .stdout(predicate::str::contains("demo"))
        .stdout(predicate::str::contains("1b"));

    // Tag filter that matches nothing.
    ephor_cmd()
        .args([
            "--registry",
            registry_path.to_str().unwrap(),
            "--tag",
            "nope",
            "list",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No projects found."));
}

#[test]
fn unknown_workspace_id_is_reported() {
    let tmp = tempdir();
    let template = write_template(tmp.path());
    let registry_path = tmp.path().join("workspaces.json");
    write_registry(
        &registry_path,
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": []
        }),
    );

    ephor_cmd()
        .args([
            "--registry",
            registry_path.to_str().unwrap(),
            "--workspace",
            "nope",
            "validate",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "Unknown projects or workspaces: nope",
        ));
}
