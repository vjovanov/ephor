// Every integration-test binary compiles this module separately, so a helper
// that only some of them use reads as dead code in the rest. CI builds with
// `RUSTFLAGS=-D warnings`, which would turn that into a failure.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const TEMPLATE: &str =
    "# AGENTS.md\n\n{summary}\n\n{structure_intro}\n\n{structure_bullets}{validation_section}\n";

/// A temporary world whose path is the one the operating system will hand back.
///
/// macOS hands out `/var/folders/…` temporary directories that are really
/// `/private/var/…`, and a summoned shell prints the second spelling as its
/// `$PWD`. A test that compares what a stub recorded against the path it built
/// the world at then fails on macOS and nowhere else — which is the least
/// useful kind of red, because it says nothing about the behaviour under test.
/// Resolving the base directory once, here, means both sides are the same
/// spelling for every test that asks. Windows is left alone: resolving there
/// produces a `\\?\` verbatim path, which would be a new spelling problem
/// rather than the end of this one.
pub fn tempdir() -> tempfile::TempDir {
    let mut base = std::env::temp_dir();
    if !cfg!(windows) {
        if let Ok(resolved) = base.canonicalize() {
            base = resolved;
        }
    }
    tempfile::Builder::new()
        .tempdir_in(base)
        .expect("a temporary directory")
}

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

/// Compare a feed cache against `tests/integration/golden/<name>`, or rewrite
/// it when `UPDATE_GOLDEN` is set. These goldens are the characterization net
/// for §RM-001-forge-interface: moving a provider behind the forge interface
/// must not change any observable field.
pub fn assert_golden(name: &str, cache_path: &Path) {
    let mut cache: Value = serde_json::from_str(&fs::read_to_string(cache_path).unwrap()).unwrap();
    strip_timestamps(&mut cache);
    let actual = serde_json::to_string_pretty(&cache).unwrap() + "\n";

    let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/integration/golden")
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
    // The exec bit is a Unix thing, and so is the type that sets it: importing
    // `PermissionsExt` unconditionally is what stopped every test binary in
    // this tree from compiling on Windows. Same shape as the seam's own tests.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).expect("stat script").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).expect("chmod script");
    }
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

// ---------------------------------------------------------------------------
// The world `ephor work` is exercised in: a fake forge, a registry and a
// status configuration around one project, and the fake runners a sweep
// launches. Shared because `work_test.rs` and `work_capacity_test.rs` are two
// halves of one subject and build the same world (§FS-012-file-size).
// ---------------------------------------------------------------------------

/// A fake `gh` serving one pull request of the user's own with a failing
/// check, and one review comment awaiting an answer. The role searches arrive
/// as one aliased GraphQL request (§FS-001-forge-interface.8), and the head
/// branch and review decision ride in with it.
pub const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
conn() { printf '{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}' "$1"; }
none=$(conn '')
case "$args" in
  *"search(query:"*"is:pr"*)
    pr42='{"number": 42, "title": "Retry window", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z", "state": "OPEN", "headRefName": "you/ABC-42-work", "reviewDecision": "CHANGES_REQUESTED", "repository": {"nameWithOwner": "acme/widget"}}'
    printf '{"data":{"r0":%s,"r1":%s,"r2":%s,"r3":%s,"r4":%s}}' "$(conn "$pr42")" "$none" "$none" "$none" "$none"
    ;;
  *"pr checks"*)
    printf '[{"name": "gate", "state": "FAILURE", "link": "https://ci/1"}, {"name": "style", "state": "SUCCESS", "link": "https://ci/2"}]'
    ;;
  *"pr list"*)
    printf '[{"number": 42, "title": "Retry window", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z"}]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;
pub fn env(tmp: &Path) -> Vec<(String, String)> {
    vec![
        (
            "XDG_STATE_HOME".to_string(),
            tmp.join("state").to_string_lossy().into_owned(),
        ),
        (
            "EPHOR_STATUS_CONFIG".to_string(),
            tmp.join("status.json").to_string_lossy().into_owned(),
        ),
        (
            "EPHOR_REGISTRY".to_string(),
            tmp.join("workspaces.json").to_string_lossy().into_owned(),
        ),
    ]
}
pub fn ephor(tmp: &Path) -> assert_cmd::Command {
    let mut cmd = ephor_cmd();
    let fake_bin = tmp.join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH);
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            fake_bin.to_string_lossy(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    for (key, value) in env(tmp) {
        cmd.env(key, value);
    }
    cmd
}
pub fn fixture(tmp: &Path, work: Value) {
    let project_root = tmp.join("demo");
    fs::create_dir_all(&project_root).unwrap();
    fixture_with(tmp, &project_root, "main", work);
}
/// The same fixture rooted at a checkout that already exists, on the main
/// branch that checkout was grown from — for the recipes whose opening move is
/// measured in git rather than reported by a forge.
pub fn fixture_on(tmp: &Path, project_root: &Path, main_branch: &str) {
    fixture_with(tmp, project_root, main_branch, Value::Null);
}
pub fn fixture_with(tmp: &Path, project_root: &Path, main_branch: &str, work: Value) {
    let template = write_template(tmp);
    let mut types = base_project_types(&template);
    // The shared fixture's monorepo type leaves `default_branch` as the
    // `{branch}` template, which no test expands. A project whose staleness is
    // actually measured needs the branch its repository is replayed against.
    types[0]["repos"][0]["default_branch"] = json!(main_branch);

    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": types,
            "hook_sets": [],
            "projects": [{
                "id": "demo",
                "type": "monorepo",
                "display_name": "Demo",
                "root": project_root.to_string_lossy(),
                "main_branch": main_branch,
                "branches": [
                    { "id": "demo-ticket", "branch": "you/ABC-42-work", "active": true, "ticket": "ABC-42" }
                ]
            }]
        }),
    );
    let mut config = json!({
        "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
        "projects": {
            "demo": {
                "providers": [{ "provider": "github-prs", "repos": ["acme/widget"] }]
            }
        }
    });
    if !work.is_null() {
        config["work"] = work;
    }
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
}
pub fn detaching_runner(tmp: &Path, log: &Path) {
    fs::create_dir_all(tmp.join("fakebin")).unwrap();
    make_executable(
        &tmp.join("fakebin/rhei"),
        &format!(
            "#!/usr/bin/env bash\n\
             case \"$*\" in\n\
             *--help*) printf 'Options:\\n      --headless  detach it\\n'; exit 0 ;;\n\
             *--headless*) printf '%s\\n' \"$*\" >> {log}; printf '{{\"id\":\"3f9a2c\",\"pid\":9,\"status\":\"running\",\"exit_code\":null}}\\n'; exit 0 ;;\n\
             *) printf 'other %s\\n' \"$*\" >> {log}; exit 0 ;;\n\
             esac\n",
            log = log.to_string_lossy(),
        ),
    );
}
/// How many runs the fake launcher was asked to start.
pub fn starts(log: &Path) -> usize {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("--headless"))
        .count()
}
