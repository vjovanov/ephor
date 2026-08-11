//! §FS-001-forge-interface.2, out-of-process transport, end to end.
//!
//! The extension below is a real shell script — no Rust, no compilation — and
//! it is the whole implementation of a forge. If this test passes, `jq` over a
//! vendor CLI is a complete way to add one.

mod common;

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use common::*;

/// A forge extension in bash. It reads the request on stdin, answers each
/// subcommand with JSON on stdout, and decides for itself which messages are
/// the user's — the one identity question policy cannot answer for it.
const FAKE_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
request="$(cat)"
me=$(printf '%s' "$request" | jq -r '.config.user')

case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true,"issues":true}'
    ;;
  pull-requests)
    jq -n --arg me "$me" '[
      {
        id: "app/101", repo: "app", number: "101",
        title: "Widen the retry window",
        url: "https://forge.example/pr/101",
        branch: "you/ABC-42-retry",
        updated_at: "2026-07-30T12:00:00Z",
        role: "author", state: "open:needs_work", cited: false,
        threads: [ { messages: [
          { author: "Other Dev", text: "Please widen it.", when: "2026-07-30T11:00:00Z", mine: false },
          { author: $me,        text: "Done.",            when: "2026-07-30T11:30:00Z", mine: true  }
        ] } ],
        gate: { repos: [ { repo: "app", passed: 5, failed: 1, running: 0 },
                         { repo: "plugins", passed: 2, failed: 0, running: 1 } ] }
      },
      {
        id: "plugins/202", repo: "plugins", number: "202",
        title: "Cache the resolver",
        url: "https://forge.example/pr/202",
        branch: "someone/cache",
        updated_at: "2026-07-29T12:00:00Z",
        role: "reviewer", state: "open:mentioned", cited: true,
        threads: [ { messages: [
          { author: "Other Dev", text: "what do you think?", when: "2026-07-29T11:00:00Z", mine: false }
        ] } ]
      }
    ]'
    ;;
  issues)
    jq -n --argjson tickets "$(printf '%s' "$request" | jq '.tickets')" '[
      $tickets[] | {
        key: ., title: "Retry window is too narrow",
        status: "In Progress",
        url: ("https://tracker.example/browse/" + .),
        updated_at: "2026-07-29T09:12:00Z",
        messages: [ { author: "Other Dev", text: "Reproduced on staging.",
                      when: "2026-07-28T10:00:00Z", mine: false } ]
      } ]'
    ;;
  *)
    echo "unknown subcommand: $1" >&2
    exit 2
    ;;
esac
"#;

fn extension_env(tmp: &Path) -> Vec<(String, String)> {
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

fn write_fixture(tmp: &Path) {
    let template = write_template(tmp);
    let project_root = tmp.join("demo");
    fs::create_dir_all(&project_root).unwrap();

    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [{
                "id": "demo",
                "type": "monorepo",
                "display_name": "Demo",
                "root": project_root.to_string_lossy(),
                "main_branch": "main",
                "branches": [
                    { "id": "demo-ticket", "branch": "you/ABC-42-retry", "active": true, "ticket": "ABC-42" }
                ]
            }]
        }),
    );

    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10 },
            "projects": {
                "demo": {
                    // Names no built-in provider, so ephor resolves
                    // `ephor-forge-demoforge` on PATH. Every other key is
                    // opaque to ephor and handed to the extension.
                    "providers": [
                        { "provider": "demoforge", "user": "dev", "repos": ["app", "plugins"] }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn path_with_extension(tmp: &Path) -> String {
    let fake_bin = tmp.join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("ephor-forge-demoforge"), FAKE_FORGE);
    format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn a_bash_extension_is_a_complete_forge_implementation() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());
    let path = path_with_extension(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in extension_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    let cache: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let items = cache["providers"]["demoforge"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        items.len(),
        3,
        "two pull requests and one issue: {items:#?}"
    );

    let authored = items
        .iter()
        .find(|i| i["id"] == "demoforge:app/101")
        .unwrap();
    // Policy ran over out-of-process data exactly as it would in process: a
    // needs_work verdict stands even though the user had the last word in the
    // thread — answering a comment does not clear a review.
    assert_eq!(authored["role"], "author");
    assert_eq!(authored["needs_response"], true);
    assert_eq!(authored["raw"]["branch"], "you/ABC-42-retry");
    assert_eq!(authored["raw"]["gate"]["repos"][1]["running"], 1);
    assert_eq!(
        authored["raw"]["threads"][0]["messages"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Cited, and the last word is not the user's: unanswered.
    let reviewing = items
        .iter()
        .find(|i| i["id"] == "demoforge:plugins/202")
        .unwrap();
    assert_eq!(reviewing["role"], "reviewer");
    assert_eq!(reviewing["needs_response"], true);

    // The registry's active branch ticket reached the extension and came back
    // as an issue, with the same message shape as a conversation.
    let issue = items
        .iter()
        .find(|i| i["id"] == "demoforge:ABC-42")
        .unwrap();
    assert_eq!(issue["kind"], "message");
    assert_eq!(
        issue["title"],
        "ABC-42 [In Progress] Retry window is too narrow"
    );
    assert_eq!(issue["needs_response"], true);

    // The gate renders on the row like any other forge's.
    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in extension_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed"])
        .assert()
        .success()
        .stdout(predicates::str::contains("✓7 ✗1 ⋯1"));
}

/// An extension that is not installed, or that answers rubbish, must degrade
/// to a provider warning — never take the refresh down.
#[test]
fn a_broken_extension_degrades_to_a_provider_warning() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());
    let fake_bin = tmp.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(
        &fake_bin.join("ephor-forge-demoforge"),
        "#!/usr/bin/env bash\nprintf 'not json'\n",
    );
    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in extension_env(tmp.path()) {
        cmd.env(key, value);
    }
    // Exit 3 = every provider failed; the run itself still completes.
    let output = cmd.args(["refresh", "demo"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("demoforge"), "{stderr}");
}
