mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::json;

use common::*;

/// A fake `gh` that serves canned JSON for the calls github-prs/ci make.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
case "$args" in
  *"search prs"*"--author"*)
    printf '[{"number": 42, "title": "Fix condition errors", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z", "isDraft": false}]'
    ;;
  *"search prs"*"--commenter"*|*"search prs"*"--mentions"*)
    printf '[]'
    ;;
  *"pr view"*reviewDecision*)
    printf '{"reviewDecision": "CHANGES_REQUESTED", "headRefName": "you/ABC-42-work"}'
    ;;
  *"pr list"*)
    printf '[{"number": 42, "title": "Fix condition errors", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z"}]'
    ;;
  *"pr checks"*)
    printf '[{"name": "gate", "state": "FAILURE", "link": "https://ci/1"}, {"name": "style", "state": "SUCCESS", "link": "https://ci/2"}]'
    ;;
  *"search issues"*"--author"*)
    # One closed issue on a repository nobody configured, with no comments.
    printf '[{"number": 7951, "title": "OSC 8 hyperlinks disabled", "url": "https://github.com/other/pi/issues/7951", "updatedAt": "2026-08-01T09:00:00Z", "state": "closed", "repository": {"nameWithOwner": "other/pi"}, "commentsCount": 0}]'
    ;;
  *"search issues"*"--involves"*)
    # Returns the authored one too (author is involved) plus one that is
    # someone else's, with a comment awaiting a reply.
    printf '[{"number": 7951, "title": "OSC 8 hyperlinks disabled", "url": "https://github.com/other/pi/issues/7951", "updatedAt": "2026-08-01T09:00:00Z", "state": "closed", "repository": {"nameWithOwner": "other/pi"}, "commentsCount": 0}, {"number": 12, "title": "Retry window", "url": "https://github.com/other/lib/issues/12", "updatedAt": "2026-08-01T11:00:00Z", "state": "open", "repository": {"nameWithOwner": "other/lib"}, "commentsCount": 1}]'
    ;;
  *graphql*issue*comments*)
    printf '{"data": {"repository": {"issue": {"comments": {"nodes": [{"id": "IC_1", "author": {"login": "someone"}, "body": "any update?", "createdAt": "2026-08-01T11:00:00Z", "reactions": {"nodes": []}}]}}}}}'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

fn feed_env(tmp: &Path) -> Vec<(String, String)> {
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

fn write_feed_fixture(tmp: &Path) {
    let template = write_template(tmp);
    let project_root = tmp.join("demo");
    fs::create_dir_all(&project_root).unwrap();
    fs::write(project_root.join("status.txt"), "all systems go\n").unwrap();

    write_registry(
        &tmp.join("workspaces.json"),
        &json!({
            "project_types": base_project_types(&template),
            "hook_sets": [],
            "projects": [
                {
                    "id": "demo",
                    "type": "monorepo",
                    "display_name": "Demo",
                    "root": project_root.to_string_lossy(),
                    "main_branch": "main",
                    "branches": [
                        { "id": "demo-main", "branch": "main", "active": true },
                        { "id": "demo-ticket", "branch": "you/ABC-42-work", "active": true, "ticket": "ABC-42" }
                    ]
                }
            ]
        }),
    );

    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "projects": {
                "demo": {
                    "providers": [
                        { "provider": "github-prs", "repos": ["acme/widget"] },
                        { "provider": "github-ci", "repos": ["acme/widget"] },
                        { "provider": "custom-status", "command": "cat status.txt" }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

fn path_with_fake_gh(tmp: &Path) -> String {
    let fake_bin = tmp.join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH);
    format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    )
}

#[test]
fn refresh_populates_cache_and_feed_lists_items() {
    let tmp = tempfile::tempdir().unwrap();
    write_feed_fixture(tmp.path());
    let path = path_with_fake_gh(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo: 3 items"));

    let cache_file = tmp.path().join("state/ephor/feed/demo.json");
    let cache: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cache_file).unwrap()).unwrap();
    assert_eq!(cache["project"], "demo");
    assert_eq!(cache["providers"]["github-prs"]["ok"], true);
    assert_eq!(
        cache["providers"]["github-prs"]["items"][0]["id"],
        "github-prs:acme/widget#42"
    );
    // The PR records its head branch (drives checkout state in the TUI).
    assert_eq!(
        cache["providers"]["github-prs"]["items"][0]["raw"]["branch"],
        "you/ABC-42-work"
    );
    // …and its gate: one passing and one failing check on the PR's repo.
    assert_eq!(
        cache["providers"]["github-prs"]["items"][0]["raw"]["gate"],
        json!({ "repos": [{ "repo": "acme/widget", "passed": 1, "failed": 1, "running": 0 }] })
    );
    assert_eq!(
        cache["providers"]["github-prs"]["items"][0]["needs_response"],
        true
    );
    assert_eq!(
        cache["providers"]["github-ci"]["items"][0]["needs_response"],
        true
    );
    assert_eq!(
        cache["providers"]["custom-status"]["items"][0]["title"],
        "all systems go"
    );

    // Feed shows the items; --unread empties after mark-read --all.
    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Fix condition errors"))
        // The PR line carries its gate status.
        .stdout(predicate::str::contains("✓1 ✗1"));

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["--all", "mark-read"]).assert().success();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed", "--unread"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No feed items"));
}

/// Characterization net for §RM-001-forge-interface: the whole GitHub-backed
/// feed, field for field. Moving these providers behind the forge interface
/// must not change any observable value.
#[test]
fn github_providers_produce_the_recorded_feed() {
    let tmp = tempfile::tempdir().unwrap();
    write_feed_fixture(tmp.path());
    let path = path_with_fake_gh(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    assert_golden(
        "github-feed.json",
        &tmp.path().join("state/ephor/feed/demo.json"),
    );
}

/// Write the shared fixture, then replace its feed config with one whose only
/// provider is github-issues, searching the whole forge.
fn write_issues_fixture(tmp: &Path, recent_days: u64) {
    write_feed_fixture(tmp);
    fs::write(
        tmp.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": {
                "ttl_seconds": 600,
                "provider_timeout_seconds": 10,
                "github_user": "tester",
                "recent_days": recent_days
            },
            "projects": {
                "demo": {
                    "providers": [
                        { "provider": "github-issues", "participating": true }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
}

/// §FS-001-forge-interface.1: issues arrive by role, from repositories nobody
/// configured, closed ones included.
#[test]
fn issues_arrive_by_role_from_repositories_nobody_configured() {
    let tmp = tempfile::tempdir().unwrap();
    write_issues_fixture(tmp.path(), 3650);
    let path = path_with_fake_gh(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    // Two issues, not three: the authored one comes back from the involves
    // search as well and is counted once, as the user's own.
    cmd.args(["refresh", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo: 2 items"));

    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let items = cache["providers"]["github-issues"]["items"]
        .as_array()
        .unwrap();
    let mine = items
        .iter()
        .find(|item| item["id"] == "github-issues:other/pi#7951")
        .expect("the issue the user opened");
    assert_eq!(mine["kind"], "issue");
    assert_eq!(mine["role"], "author");
    assert_eq!(mine["state"], "closed");
    // Closed with nobody waiting on the user — it is news, not a task.
    assert_eq!(mine["needs_response"], false);

    let theirs = items
        .iter()
        .find(|item| item["id"] == "github-issues:other/lib#12")
        .expect("the issue the user only takes part in");
    assert_eq!(theirs["role"], "reviewer");
    assert_eq!(theirs["state"], "open");
    assert_eq!(theirs["needs_response"], true);
    assert_eq!(
        theirs["raw"]["threads"][0]["messages"][0]["text"],
        "any update?"
    );

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("other/pi#7951"))
        .stdout(predicate::str::contains("[closed]"))
        .stdout(predicate::str::contains("other/lib#12"));

    // Filtering by the new kind reaches both.
    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed", "--kind", "issue"])
        .assert()
        .success()
        .stdout(predicate::str::contains("other/pi#7951"))
        .stdout(predicate::str::contains("other/lib#12"));
}

/// §FS-003-feed-categories.3: with the recency window shut, finished work
/// leaves the feed the moment it finishes; unfinished work is untouched.
#[test]
fn a_zero_recency_window_drops_finished_work_from_the_feed() {
    let tmp = tempfile::tempdir().unwrap();
    write_issues_fixture(tmp.path(), 0);
    let path = path_with_fake_gh(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("other/pi#7951").not())
        .stdout(predicate::str::contains("other/lib#12"));
}

#[test]
fn failing_provider_keeps_previous_items_as_stale() {
    let tmp = tempfile::tempdir().unwrap();
    write_feed_fixture(tmp.path());
    let path = path_with_fake_gh(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    // Break gh: now github providers fail but their items must survive.
    let fake_bin = tmp.path().join("fakebin");
    make_executable(&fake_bin.join("gh"), "#!/usr/bin/env bash\nexit 1\n");

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    // Keeping the last-good items is not the same as the refresh succeeding:
    // the github providers delivered nothing this time, and a run that says
    // "success" while a source is down is what lets an outage go unnoticed.
    cmd.args(["refresh", "demo"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("github-prs"));

    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cache["providers"]["github-prs"]["ok"], false);
    assert_eq!(cache["providers"]["github-prs"]["stale"], true);
    assert_eq!(
        cache["providers"]["github-prs"]["items"][0]["id"],
        "github-prs:acme/widget#42"
    );
    // custom-status does not depend on gh and stays fresh.
    assert_eq!(cache["providers"]["custom-status"]["ok"], true);
}

#[test]
fn status_check_exits_4_on_unread_needs_response() {
    let tmp = tempfile::tempdir().unwrap();
    write_feed_fixture(tmp.path());
    let path = path_with_fake_gh(tmp.path());

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["status", "demo", "--cached", "--check"])
        .assert()
        .code(4);

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["--all", "mark-read"]).assert().success();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["status", "demo", "--cached", "--check"])
        .assert()
        .code(0);
}

/// A fake `gh` for the reviewing path: the user is cited on PR 77 and the
/// conversation records the exchange.
const FAKE_GH_REVIEW: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
case "$args" in
  *"search prs"*"--author"*)
    printf '[]'
    ;;
  *"search prs"*"--commenter"*|*"search prs"*"--mentions"*)
    printf '[{"number": 77, "title": "Add layered workflow", "url": "https://github.com/acme/widget/pull/77", "updatedAt": "2026-08-02T10:00:00Z", "isDraft": false}]'
    ;;
  *"api user"*)
    printf 'tester'
    ;;
  *"api graphql"*)
    printf '{"data":{"repository":{"pullRequest":{"comments":{"nodes":[{"author":{"login":"reviewer"},"body":"@tester can you confirm?","createdAt":"2026-08-02T09:00:00Z","reactions":{"nodes":[]}}]}}}}}'
    ;;
  *"pr list"*)
    printf '[]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

#[test]
fn reviewer_items_record_threads_and_pending_citation() {
    let tmp = tempfile::tempdir().unwrap();
    write_feed_fixture(tmp.path());
    let fake_bin = tmp.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH_REVIEW);
    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );

    // Enable the reviewing side for this fixture.
    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "projects": {
                "demo": {
                    "providers": [
                        { "provider": "github-prs", "repos": ["acme/widget"], "reviews": true }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let item = &cache["providers"]["github-prs"]["items"][0];
    assert_eq!(item["role"], "reviewer");
    assert_eq!(item["state"], "open:mentioned");
    // Cited with no reply of ours afterwards and no reaction: still pending.
    assert_eq!(item["needs_response"], true);
    let messages = &item["raw"]["threads"][0]["messages"];
    assert_eq!(messages[0]["author"], "reviewer");
    assert_eq!(messages[0]["text"], "@tester can you confirm?");
}

#[test]
fn answered_citation_stops_needing_a_response() {
    let tmp = tempfile::tempdir().unwrap();
    write_feed_fixture(tmp.path());
    let fake_bin = tmp.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    // Same as FAKE_GH_REVIEW, but the user replied after being cited.
    let answered = FAKE_GH_REVIEW.replace(
        r#""reactions":{"nodes":[]}}]}"#,
        r#""reactions":{"nodes":[]}},{"author":{"login":"tester"},"body":"confirmed","createdAt":"2026-08-02T09:30:00Z","reactions":{"nodes":[]}}]}"#,
    );
    make_executable(&fake_bin.join("gh"), &answered);
    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );

    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "projects": {
                "demo": {
                    "providers": [
                        { "provider": "github-prs", "repos": ["acme/widget"], "reviews": true }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let item = &cache["providers"]["github-prs"]["items"][0];
    assert_eq!(item["needs_response"], false);
    assert_eq!(item["raw"]["threads"][0]["messages"][1]["author"], "tester");
}

#[test]
fn status_without_config_reports_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cmd = ephor_cmd();
    cmd.env("EPHOR_STATUS_CONFIG", tmp.path().join("missing.json"));
    cmd.args(["status"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Cannot read feed config"));
}
