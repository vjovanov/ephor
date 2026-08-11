// Every integration-test binary compiles this module separately, so a helper
// that only some of them use reads as dead code in the rest. CI builds with
// `RUSTFLAGS=-D warnings`, which would turn that into a failure.
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const TEMPLATE: &str =
    "# AGENTS.md\n\n{summary}\n\n{structure_intro}\n\n{structure_bullets}{validation_section}\n";

pub fn ephor_cmd() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("ephor").expect("ephor binary")
}

/// Drop wall-clock fields so a golden compares only derived data.
pub fn strip_timestamps(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("fetched_at");
            // custom-status has no upstream timestamp, so it stamps the moment
            // it ran — the one item field that is not derived from the fixture.
            if map.get("source").and_then(Value::as_str) == Some("custom-status") {
                map.remove("updated_at");
            }
            for (_, nested) in map.iter_mut() {
                strip_timestamps(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_timestamps(item);
            }
        }
        _ => {}
    }
}

/// Compare a feed cache against `tests/golden/<name>`, or rewrite it when
/// `UPDATE_GOLDEN` is set. These goldens are the characterization net for
/// §RM-001-forge-interface: moving a provider behind the forge interface must
/// not change any observable field.
pub fn assert_golden(name: &str, cache_path: &Path) {
    let mut cache: Value = serde_json::from_str(&fs::read_to_string(cache_path).unwrap()).unwrap();
    strip_timestamps(&mut cache);
    let actual = serde_json::to_string_pretty(&cache).unwrap() + "\n";

    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(golden.parent().unwrap()).unwrap();
        fs::write(&golden, &actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&golden).unwrap_or_else(|err| {
        panic!(
            "missing golden {}: {err}\nrun: UPDATE_GOLDEN=1 cargo test",
            golden.display()
        )
    });
    assert_eq!(actual, expected, "{name} drifted from the golden");
}

pub fn write_template(dir: &Path) -> PathBuf {
    let path = dir.join("root_agents.md.tmpl");
    fs::write(&path, TEMPLATE).expect("write template");
    path
}

pub fn write_registry(path: &Path, data: &Value) {
    fs::write(
        path,
        serde_json::to_string_pretty(data).expect("serialize registry"),
    )
    .expect("write registry");
}

pub fn make_executable(path: &Path, contents: &str) {
    fs::write(path, contents).expect("write script");
    let mut perms = fs::metadata(path).expect("stat script").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("chmod script");
}

pub fn base_project_types(template_path: &Path) -> Value {
    let template = template_path.to_string_lossy().into_owned();
    json!([
        {
            "id": "monorepo",
            "layout": "monorepo",
            "repos": [
                {
                    "id": "repo",
                    "path": ".",
                    "role": "Repository root",
                    "required": true,
                    "update_mode": "branch",
                    "default_branch": "{branch}",
                    "agents_description": "The repository root contains the full project."
                }
            ],
            "agents": {
                "template": template,
                "structure_intro": "This project uses a single repository root:",
                "summary_template": "This project root tracks `{display_name}` on branch `{branch}`."
            }
        },
        {
            "id": "product-workspace",
            "layout": "polyrepo",
            "repos": [
                {
                    "id": "app",
                    "path": "app",
                    "role": "Application repository",
                    "required": true,
                    "update_mode": "branch",
                    "default_branch": "{branch}",
                    "agents_description": "`app/` is the main application repository.",
                    "agents_guide": "app/AGENTS.md"
                },
                {
                    "id": "plugins",
                    "path": "plugins",
                    "role": "Plugin repository",
                    "required": true,
                    "update_mode": "branch",
                    "default_branch": "{branch}",
                    "agents_description": "`plugins/` holds the plugins released alongside the application.",
                    "agents_guide": "plugins/AGENTS.md"
                },
                {
                    "id": "docs-site",
                    "path": "docs-site",
                    "role": "Documentation site",
                    "required": true,
                    "update_mode": "branch",
                    "default_branch": "{branch}",
                    "agents_description": "`docs-site/` holds the published documentation."
                }
            ],
            "agents": {
                "template": template,
                "structure_intro": "This workspace contains separate repositories:",
                "summary_with_ticket_template": "Refer to the following configurations for ticket context:\n\n* @./ticket-number.md\n* @./ticket-content.md",
                "summary_without_ticket_template": "This workspace is for branch `{branch}`.",
                "validations": [
                    { "cwd": "app", "command": "just test" }
                ]
            }
        }
    ])
}

pub fn required_project(root: &Path) -> Value {
    json!({
        "id": "widget",
        "type": "product-workspace",
        "display_name": "Widget",
        "root": root.to_string_lossy(),
        "main_branch": "master",
        "required_branch_ids": ["widget-2.3", "widget-2.5"],
        "branch_root_template": "{project_root}/{branch}",
        "tags": ["widget"],
        "release_branches": [
            { "id": "widget-main", "branch": "master", "active": true, "display_name": "Widget main" },
            { "id": "widget-2.3", "branch": "release/2.3", "active": true, "display_name": "Widget 2.3" },
            { "id": "widget-2.5", "branch": "release/2.5", "active": true, "display_name": "Widget 2.5" }
        ],
        "branches": []
    })
}
