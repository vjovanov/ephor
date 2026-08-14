mod common;

use std::fs;

use predicates::prelude::*;
use serde_json::json;

use common::*;

#[test]
fn ensure_agents_supports_ad_hoc_workspace() {
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

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(workspace_root.join("app")).unwrap();
    fs::create_dir_all(workspace_root.join("plugins")).unwrap();

    ephor_cmd()
        .args([
            "--registry",
            registry_path.to_str().unwrap(),
            "ensure-agents",
            "--type",
            "product-workspace",
            "--root",
            workspace_root.to_str().unwrap(),
            "--display-name",
            "Widget workspace",
            "--var",
            "branch=you/ABC-42-sample",
        ])
        .assert()
        .success();

    let agents = fs::read_to_string(workspace_root.join("AGENTS.md")).unwrap();
    assert!(agents.contains("* @./ticket-number.md"), "{agents}");
    assert!(agents.contains("* @./ticket-content.md"), "{agents}");
    assert!(agents.contains("See `app/AGENTS.md`."), "{agents}");
    assert!(agents.contains("docs-site/"), "{agents}");
    assert!(agents.contains("just test"), "{agents}");
}

#[test]
fn ensure_agents_without_ticket_uses_branch_summary() {
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

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root).unwrap();

    ephor_cmd()
        .args([
            "--registry",
            registry_path.to_str().unwrap(),
            "ensure-agents",
            "--type",
            "product-workspace",
            "--root",
            workspace_root.to_str().unwrap(),
            "--var",
            "branch=feature/no-ticket",
        ])
        .assert()
        .success();

    let agents = fs::read_to_string(workspace_root.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("This workspace is for branch `feature/no-ticket`."),
        "{agents}"
    );
}

#[test]
fn ensure_agents_requires_root_for_ad_hoc_type() {
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
            "ensure-agents",
            "--type",
            "product-workspace",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "--root is required when using ensure-agents --type.",
        ));
}
