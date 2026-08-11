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
    // Issues are their own category (§FS-003-feed-categories.1), and an
    // implementation that does not report a role is reporting the user's own.
    assert_eq!(issue["kind"], "issue");
    assert_eq!(issue["role"], "author");
    assert_eq!(issue["title"], "ABC-42 Retry window is too narrow");
    assert_eq!(issue["state"], "in progress");
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

/// A forge that answers correctly, but slower than the shared default ceiling.
const SLOW_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":false,"gate":false,"issues":false}'
    ;;
  pull-requests)
    sleep 3
    jq -n '[ { id: "app/303", repo: "app", number: "303", title: "Slow but real",
               url: "https://forge.example/pr/303", branch: "you/slow",
               updated_at: "2026-07-30T12:00:00Z", role: "author",
               state: "open", cited: false, threads: [] } ]'
    ;;
  *) exit 2 ;;
esac
"#;

/// Install the slow forge and point a one-provider `demo` project at it.
/// `block_timeout` is the provider block's own `timeout_seconds`, if any.
fn slow_forge_fixture(tmp: &Path, block_timeout: Option<u64>) -> String {
    write_fixture(tmp);
    let mut provider = json!({ "provider": "slowforge", "user": "dev" });
    if let Some(seconds) = block_timeout {
        provider["timeout_seconds"] = json!(seconds);
    }
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            // One second: shorter than the forge, so only the block's own
            // ceiling can let it finish.
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 1 },
            "projects": { "demo": { "providers": [provider] } }
        }))
        .unwrap(),
    )
    .unwrap();

    let fake_bin = tmp.join("slowbin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("ephor-forge-slowforge"), SLOW_FORGE);
    format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn refresh_slow_forge(tmp: &Path, path: &str) -> Value {
    let mut cmd = ephor_cmd();
    cmd.env("PATH", path);
    for (key, value) in extension_env(tmp) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).output().unwrap();
    serde_json::from_str(&fs::read_to_string(tmp.join("state/ephor/feed/demo.json")).unwrap())
        .unwrap()
}

/// A provider block raises its own ceiling past `provider_timeout_seconds`.
/// A forge behind a VPN is slower than a local `gh` call, and sizing the
/// shared default for it would delay every other provider's failure.
#[test]
fn a_provider_block_raises_its_own_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let path = slow_forge_fixture(tmp.path(), Some(30));
    let slot = &refresh_slow_forge(tmp.path(), &path)["providers"]["slowforge"];

    assert_eq!(slot["ok"], true, "{slot:#?}");
    let items = slot["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 1, "{items:#?}");
    assert_eq!(items[0]["id"], "slowforge:app/303");
}

/// The companion: without a block of its own, the same forge is held to the
/// default and times out. This is what proves the test above measures the
/// override rather than a ceiling that was generous all along.
#[test]
fn without_an_override_the_default_timeout_still_applies() {
    let tmp = tempfile::tempdir().unwrap();
    let path = slow_forge_fixture(tmp.path(), None);
    let slot = &refresh_slow_forge(tmp.path(), &path)["providers"]["slowforge"];

    assert_eq!(slot["ok"], false, "{slot:#?}");
    let error = slot["error"].as_str().unwrap_or_default();
    assert!(error.contains("timed out"), "{error}");
}

/// An extension that was never installed must say so by name. "missing tool
/// or secret" sends the reader looking for a credential, when what is missing
/// is an executable whose name appears nowhere in the configuration — the
/// provider block names the *forge*, and ephor derives the command from it.
#[test]
fn an_uninstalled_extension_names_the_executable_it_wanted() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());
    // An empty directory: nothing named `ephor-forge-demoforge` anywhere.
    let empty_bin = tmp.path().join("emptybin");
    fs::create_dir_all(&empty_bin).unwrap();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", empty_bin.to_string_lossy().into_owned());
    for (key, value) in extension_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).output().unwrap();

    let cache: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let error = cache["providers"]["demoforge"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(
        error.contains("ephor-forge-demoforge") && error.contains("PATH"),
        "diagnostic must name the executable and where it looked: {error}"
    );
}

/// Install a forge that fails a given way, and return its refreshed cache
/// slot plus the exit code and stderr of the refresh that produced it.
fn refresh_with_forge(script: &str) -> (Value, i32, String) {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());
    let fake_bin = tmp.path().join("failbin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("ephor-forge-demoforge"), script);
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
    let output = cmd.args(["refresh", "demo"]).output().unwrap();
    let cache: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    (
        cache["providers"]["demoforge"].clone(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A destination that cannot be reached is its own condition, not a generic
/// warning: nothing about the setup is wrong, the items on screen are last
/// known values, and the fix is a network rather than a config edit.
#[test]
fn an_unreachable_destination_is_reported_as_unreachable() {
    let (slot, _code, stderr) = refresh_with_forge(
        "#!/usr/bin/env bash\n\
         echo 'java.net.UnknownHostException: bitbucket.example.com' >&2\n\
         exit 1\n",
    );

    assert_eq!(slot["ok"], false, "{slot:#?}");
    assert_eq!(slot["unreachable"], true, "{slot:#?}");
    assert!(
        stderr.contains("unreachable"),
        "the refresh must say so on stderr: {stderr}"
    );
}

/// The converse: a failure the reader has to fix must not be filed under
/// "the network is down", or they will wait for it to heal on its own.
#[test]
fn a_broken_extension_is_not_reported_as_unreachable() {
    let (slot, _code, stderr) = refresh_with_forge(
        "#!/usr/bin/env bash\necho 'bad configuration: unknown project key' >&2\nexit 1\n",
    );

    assert_eq!(slot["ok"], false, "{slot:#?}");
    // Absent or false — a healthy-shaped slot does not carry the marker.
    assert!(!slot["unreachable"].as_bool().unwrap_or(false), "{slot:#?}");
    assert!(!stderr.contains("unreachable"), "{stderr}");
}

/// Losing every provider is exit 3, as it always was.
#[test]
fn losing_every_provider_still_exits_three() {
    let (_slot, code, stderr) =
        refresh_with_forge("#!/usr/bin/env bash\necho 'exploded' >&2\nexit 1\n");

    assert_eq!(code, 3, "{stderr}");
    assert!(
        stderr.contains("error:") && stderr.contains("demoforge"),
        "the failure must name the provider: {stderr}"
    );
}

/// The case that used to pass silently: one provider dies, another survives,
/// and the refresh reports success. That is how a forge stays uninstalled for
/// months — the timer running this sees exit 0 every time, and the section of
/// the feed it should have filled just looks like a quiet week.
#[test]
fn a_partly_lost_refresh_fails_explicitly() {
    let tmp = tempfile::tempdir().unwrap();
    write_fixture(tmp.path());
    let fake_bin = tmp.path().join("mixedbin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("ephor-forge-goodforge"), FAKE_FORGE);
    make_executable(
        &fake_bin.join("ephor-forge-deadforge"),
        "#!/usr/bin/env bash\necho 'exploded' >&2\nexit 1\n",
    );
    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10 },
            "projects": { "demo": { "providers": [
                { "provider": "goodforge", "user": "dev", "repos": ["app"] },
                { "provider": "deadforge", "user": "dev", "repos": ["app"] }
            ] } }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = ephor_cmd();
    cmd.env(
        "PATH",
        format!(
            "{}:{}",
            fake_bin.to_string_lossy(),
            std::env::var("PATH").unwrap_or_default()
        ),
    );
    for (key, value) in extension_env(tmp.path()) {
        cmd.env(key, value);
    }
    let output = cmd.args(["refresh", "demo"]).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(4),
        "a refresh that lost a provider must not report success: {stderr}"
    );
    assert!(
        stderr.contains("deadforge"),
        "the lost provider must be named: {stderr}"
    );

    // The survivor still delivered: an explicit failure must not cost the
    // providers that worked.
    let cache: Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cache["providers"]["goodforge"]["ok"], true);
    assert!(!cache["providers"]["goodforge"]["items"]
        .as_array()
        .unwrap()
        .is_empty());
}

/// The `capabilities` probe is the forge's first call, so it is where an
/// unreachable host shows up. Its error must be reported as itself — it used
/// to be replaced with "declared no capabilities", which describes a working
/// extension behind a downed VPN as a malformed one.
#[test]
fn a_failing_capabilities_probe_reports_its_own_error() {
    let (slot, _code, _stderr) = refresh_with_forge(
        "#!/usr/bin/env bash\n\
         if [ \"$1\" = capabilities ]; then\n\
           echo 'connection refused by gateway' >&2\n\
           exit 1\n\
         fi\n\
         printf '[]'\n",
    );

    let error = slot["error"].as_str().unwrap_or_default();
    assert!(
        error.contains("connection refused"),
        "the probe's real error must survive: {error}"
    );
    assert_eq!(slot["unreachable"], true, "{slot:#?}");
}
