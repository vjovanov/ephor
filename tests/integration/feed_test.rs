mod common;

use std::fs;
use std::path::Path;

use predicates::prelude::*;
use serde_json::json;

use common::*;

/// A fake `gh` that serves canned JSON for the calls github-prs/ci make.
///
/// The role searches arrive as **one** GraphQL request carrying an aliased
/// search per role (§FS-001-forge-interface.8), so the answer is one object
/// with every alias in it: `r0` authored, then in-a-thread, cited, review
/// requested, assigned. `pr view` is a hard failure here — the head branch and
/// the review decision ride in with the search now, and asking again per pull
/// request is the cost §FS-001-forge-interface.8.3 exists to refuse.
const FAKE_GH: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
conn() { printf '{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}' "$1"; }
none=$(conn '')
case "$args" in
  *"search(query:"*"is:pr"*)
    pr42='{"number": 42, "title": "Fix condition errors", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z", "state": "OPEN", "headRefName": "you/ABC-42-work", "reviewDecision": "CHANGES_REQUESTED", "repository": {"nameWithOwner": "acme/widget"}}'
    printf '{"data":{"r0":%s,"r1":%s,"r2":%s,"r3":%s,"r4":%s}}' "$(conn "$pr42")" "$none" "$none" "$none" "$none"
    ;;
  *"search(query:"*"is:issue"*)
    # One closed issue on a repository nobody configured, with no comments;
    # the involves question returns it too (an author is involved) plus one
    # that is someone else's, with a comment awaiting a reply.
    mine='{"number": 7951, "title": "OSC 8 hyperlinks disabled", "url": "https://github.com/other/pi/issues/7951", "updatedAt": "2026-08-01T09:00:00Z", "state": "CLOSED", "repository": {"nameWithOwner": "other/pi"}, "comments": {"totalCount": 0}}'
    theirs='{"number": 12, "title": "Retry window", "url": "https://github.com/other/lib/issues/12", "updatedAt": "2026-08-01T11:00:00Z", "state": "OPEN", "repository": {"nameWithOwner": "other/lib"}, "comments": {"totalCount": 1}}'
    printf '{"data":{"q0":%s,"q1":%s}}' "$(conn "$mine")" "$(conn "$mine,$theirs")"
    ;;
  *"pr view"*)
    echo "the head branch and review decision came with the search" >&2
    exit 1
    ;;
  *"pr list"*)
    printf '[{"number": 42, "title": "Fix condition errors", "url": "https://github.com/acme/widget/pull/42", "updatedAt": "2026-08-01T10:00:00Z"}]'
    ;;
  *"pr checks"*)
    printf '[{"name": "gate", "state": "FAILURE", "link": "https://ci/1"}, {"name": "style", "state": "SUCCESS", "link": "https://ci/2"}]'
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
    let tmp = tempdir();
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
        // Two rows, not three: the CI report is of the pull request it is
        // about, so its gate lands on that row rather than beside it
        // (§FS-007-matters.5).
        .stdout(predicate::str::contains("demo: 2 items"));

    let cache_file = tmp.path().join("state/ephor/feed/demo.json");
    let cache: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&cache_file).unwrap()).unwrap();
    assert_eq!(cache["project"], "demo");
    assert_eq!(cache["providers"]["github-prs"]["ok"], true);
    assert_eq!(
        cache["providers"]["github-prs"]["matters"][0]["key"],
        "github-prs:acme/widget#42"
    );
    // The PR records its head branch (drives checkout state in the TUI).
    assert_eq!(
        cache["providers"]["github-prs"]["matters"][0]["raw"]["branch"],
        "you/ABC-42-work"
    );
    // …and its gate: one passing and one failing check on the PR's repo.
    assert_eq!(
        cache["providers"]["github-prs"]["matters"][0]["raw"]["gate"],
        json!({ "repos": [{ "repo": "acme/widget", "passed": 1, "failed": 1, "running": 0 }] })
    );
    assert_eq!(
        cache["providers"]["github-prs"]["matters"][0]["needs_response"],
        true
    );
    // The CI source reports the pull request, carrying its gate — one
    // subject, one row (§FS-003-feed-categories.5).
    let ci = &cache["providers"]["github-ci"]["matters"][0];
    assert_eq!(ci["key"], "github-ci:acme/widget#42");
    assert_eq!(ci["kind"], "pr");
    assert_eq!(ci["needs_response"], true);
    assert_eq!(ci["events"][0]["kind"], "gate");
    assert_eq!(
        cache["providers"]["custom-status"]["matters"][0]["title"],
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
    cmd.args(["mark-read", "--all"]).assert().success();

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
    let tmp = tempdir();
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
    let tmp = tempdir();
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
    let items = cache["providers"]["github-issues"]["matters"]
        .as_array()
        .unwrap();
    let mine = items
        .iter()
        .find(|item| item["key"] == "github-issues:other/pi#7951")
        .expect("the issue the user opened");
    assert_eq!(mine["kind"], "issue");
    assert_eq!(mine["role"], "author");
    assert_eq!(mine["state"], "closed");
    // Closed with nobody waiting on the user — it is news, not a task.
    assert_eq!(mine["needs_response"], false);

    let theirs = items
        .iter()
        .find(|item| item["key"] == "github-issues:other/lib#12")
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
    // Fetched is not shown: the closed issue asks nothing of anybody, so it
    // has no loose end and never reaches the feed (§FS-003-feed-categories.2).
    // The open one, which is waiting on an answer, does.
    cmd.args(["feed"])
        .assert()
        .success()
        .stdout(predicate::str::contains("other/pi#7951").not())
        .stdout(predicate::str::contains("other/lib#12"));

    // Filtering by the new kind reaches the one the feed holds.
    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["feed", "--kind", "issue"])
        .assert()
        .success()
        .stdout(predicate::str::contains("other/lib#12"));
}

/// A fake `gh` for a source that follows a label: only the label search is
/// answered, and either role search is a hard failure — the label question is
/// the whole of what this fixture asks. With every question in one request,
/// "asked no role question" is now a claim about the request's contents.
const FAKE_GH_LABEL: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
conn() { printf '{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}' "$1"; }
case "$args" in
  *"search(query:"*"author:@me"*|*"search(query:"*"involves:@me"*)
    echo "a role search was made" >&2
    exit 1
    ;;
  *"search(query:"*"label:priority state:open"*)
    ours='{"number": 101, "title": "Metadata for Netty 4.2", "url": "https://github.com/oracle/graalvm-reachability-metadata/issues/101", "updatedAt": "2026-08-01T09:00:00Z", "state": "OPEN", "repository": {"nameWithOwner": "oracle/graalvm-reachability-metadata"}, "comments": {"totalCount": 0}, "author": {"login": "tester"}}'
    theirs='{"number": 102, "title": "Reflection config drifts", "url": "https://github.com/oracle/graalvm-reachability-metadata/issues/102", "updatedAt": "2026-08-01T11:00:00Z", "state": "OPEN", "repository": {"nameWithOwner": "oracle/graalvm-reachability-metadata"}, "comments": {"totalCount": 1}, "author": {"login": "stranger"}}'
    printf '{"data":{"q0":%s}}' "$(conn "$ours,$theirs")"
    ;;
  *graphql*issue*comments*)
    printf '{"data": {"repository": {"issue": {"comments": {"nodes": [{"id": "IC_9", "author": {"login": "stranger"}, "body": "still reproducing?", "createdAt": "2026-08-01T11:00:00Z", "reactions": {"nodes": []}}]}}}}}'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// §FS-001-forge-interface.1: a source told to follow a label reports the open
/// issues carrying it whoever is in them, each under the role its author says
/// the reader holds — and asks no role question at all.
#[test]
fn a_followed_label_brings_issues_nobody_is_in_under_the_role_of_their_author() {
    let tmp = tempdir();
    write_feed_fixture(tmp.path());
    let fake_bin = tmp.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH_LABEL);
    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );

    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": {
                "ttl_seconds": 600,
                "provider_timeout_seconds": 10,
                "github_user": "tester",
                "recent_days": 3650
            },
            "projects": {
                "demo": {
                    "providers": [
                        {
                            "provider": "github-issues",
                            "authored": false,
                            "participating": false,
                            "labels": ["priority"]
                        }
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
    // Success at all is the proof that no role search was made: the fake
    // fails either of them, and a provider that fails loses the refresh.
    cmd.args(["refresh", "demo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo: 2 items"));

    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let items = cache["providers"]["github-issues"]["matters"]
        .as_array()
        .unwrap();

    let mine = items
        .iter()
        .find(|item| item["key"] == "github-issues:oracle/graalvm-reachability-metadata#101")
        .expect("the labelled issue the user opened");
    assert_eq!(mine["kind"], "issue");
    assert_eq!(mine["role"], "author");
    assert_eq!(mine["state"], "open");

    // Nobody the role searches would have found is in this one, and it still
    // arrives — under the participating role.
    let theirs = items
        .iter()
        .find(|item| item["key"] == "github-issues:oracle/graalvm-reachability-metadata#102")
        .expect("the labelled issue the user has never touched");
    assert_eq!(theirs["role"], "reviewer");
    assert_eq!(theirs["state"], "open");
    assert_eq!(
        theirs["raw"]["threads"][0]["messages"][0]["text"],
        "still reproducing?"
    );
}

/// §FS-003-feed-categories.3: with the recency window shut, finished work
/// leaves the feed the moment it finishes; unfinished work is untouched.
#[test]
fn a_zero_recency_window_drops_finished_work_from_the_feed() {
    let tmp = tempdir();
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
    let tmp = tempdir();
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
        cache["providers"]["github-prs"]["matters"][0]["key"],
        "github-prs:acme/widget#42"
    );
    // custom-status does not depend on gh and stays fresh.
    assert_eq!(cache["providers"]["custom-status"]["ok"], true);
}

#[test]
fn status_check_exits_4_on_unread_needs_response() {
    let tmp = tempdir();
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
    cmd.args(["mark-read", "--all"]).assert().success();

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
/// conversation records the exchange. The search and the conversation are both
/// GraphQL now, so they are told apart by what the query asks for.
const FAKE_GH_REVIEW: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
conn() { printf '{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}' "$1"; }
none=$(conn '')
case "$args" in
  *"search(query:"*"is:pr"*)
    pr77='{"number": 77, "title": "Add layered workflow", "url": "https://github.com/acme/widget/pull/77", "updatedAt": "2026-08-02T10:00:00Z", "state": "OPEN", "repository": {"nameWithOwner": "acme/widget"}}'
    # Nothing authored; found as a commenter and as a citation.
    printf '{"data":{"r0":%s,"r1":%s,"r2":%s,"r3":%s,"r4":%s}}' "$none" "$(conn "$pr77")" "$(conn "$pr77")" "$none" "$none"
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
    let tmp = tempdir();
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
    let item = &cache["providers"]["github-prs"]["matters"][0];
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
    let tmp = tempdir();
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
    let item = &cache["providers"]["github-prs"]["matters"][0];
    assert_eq!(item["needs_response"], false);
    assert_eq!(item["raw"]["threads"][0]["messages"][1]["author"], "tester");
}

/// A fake `gh` for the completeness net: the role searches find one pull
/// request the user merely commented on, while GitHub's notification list
/// knows two things they never could — that a *team* was named on that same
/// pull request, and that a review was asked of the user in a repository
/// nobody configured.
const FAKE_GH_NOTICES: &str = r#"#!/usr/bin/env bash
set -euo pipefail
args="$*"
conn() { printf '{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[%s]}' "$1"; }
none=$(conn '')
case "$args" in
  *"api notifications"*)
    printf '[{"id": "9001", "unread": true, "reason": "team_mention", "updated_at": "2026-08-02T12:00:00Z", "repository": {"full_name": "acme/widget"}, "subject": {"title": "Add layered workflow", "type": "PullRequest", "url": "https://api.github.com/repos/acme/widget/pulls/77"}}, {"id": "9002", "unread": true, "reason": "review_requested", "updated_at": "2026-08-02T13:00:00Z", "repository": {"full_name": "other/lib"}, "subject": {"title": "Bump the timeout", "type": "PullRequest", "url": "https://api.github.com/repos/other/lib/pulls/5"}}, {"id": "9003", "unread": true, "reason": "security_alert", "updated_at": "2026-08-02T14:00:00Z", "repository": {"full_name": "acme/widget"}, "subject": {"title": "CVE-2026-1 in serde_yaml", "type": "RepositoryVulnerabilityAlert", "url": null}}, {"id": "9004", "unread": true, "reason": "subscribed", "updated_at": "2026-08-02T15:00:00Z", "repository": {"full_name": "acme/widget"}, "subject": {"title": "v2.1.0", "type": "Release", "url": "https://api.github.com/repos/acme/widget/releases/tag/v2.1.0"}}]'
    ;;
  *"search(query:"*"is:pr"*)
    pr77='{"number": 77, "title": "Add layered workflow", "url": "https://github.com/acme/widget/pull/77", "updatedAt": "2026-08-02T10:00:00Z", "state": "OPEN", "repository": {"nameWithOwner": "acme/widget"}}'
    printf '{"data":{"r0":%s,"r1":%s,"r2":%s,"r3":%s,"r4":%s}}' "$none" "$(conn "$pr77")" "$none" "$none" "$none"
    ;;
  *"api user"*)
    printf 'tester'
    ;;
  *"api graphql"*)
    printf '{"data":{"repository":{"pullRequest":{"comments":{"nodes":[{"author":{"login":"reviewer"},"body":"looks close","createdAt":"2026-08-02T09:00:00Z","reactions":{"nodes":[]}}]}}}}}'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// §FS-001-forge-interface.1 and §FS-003-feed-categories.5 together: the notice
/// list is what makes the feed exhaustive, and merging is what stops that
/// costing the reader a duplicate for every pull request they already had.
#[test]
fn the_notice_list_catches_what_the_role_searches_cannot() {
    let tmp = tempdir();
    write_feed_fixture(tmp.path());
    let fake_bin = tmp.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH_NOTICES);
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
                        { "provider": "github-prs", "repos": ["acme/widget"], "reviews": true },
                        { "provider": "github-notifications" }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut refresh = ephor_cmd();
    refresh.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        refresh.env(key, value);
    }
    refresh.args(["refresh", "demo"]).assert().success();

    let mut feed = ephor_cmd();
    feed.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        feed.env(key, value);
    }
    let out = feed.args(["feed", "--json"]).assert().success();
    let items: Vec<serde_json::Value> =
        serde_json::from_slice(&out.get_output().stdout).expect("feed --json");

    let by_id = |id: &str| {
        items
            .iter()
            .find(|item| item["id"] == id)
            .unwrap_or_else(|| panic!("{id} is missing from {items:#?}"))
    };

    // Both sources found #77, and the reader sees one row for it.
    assert_eq!(
        items
            .iter()
            .filter(|item| item["id"].as_str().is_some_and(|id| id.ends_with("#77")))
            .count(),
        1
    );
    let shared = by_id("github-prs:acme/widget#77");
    // The fuller report is the row it kept …
    assert!(shared["raw"]["threads"].is_array());
    // … carrying the reason only the notice list knew, and with it the fact
    // that somebody is waiting — which the conversation alone never said.
    assert_eq!(
        shared["raw"]["reasons"],
        json!(["in-thread", "team_mention"])
    );
    assert_eq!(shared["needs_response"], true);

    // A review asked for in a repository nobody configured is still work.
    let elsewhere = by_id("github-notifications:other/lib#5");
    assert_eq!(elsewhere["kind"], "pr");
    assert_eq!(elsewhere["role"], "reviewer");
    assert_eq!(elsewhere["needs_response"], true);
    assert_eq!(elsewhere["url"], "https://github.com/other/lib/pull/5");

    // An advisory is a kind ephor has no capability for at all — no search it
    // could run would ever return one — so it arrives as a message
    // (§FS-003-feed-categories.1) and still asks for an answer.
    let advisory = by_id("github-notifications:9003");
    assert_eq!(advisory["kind"], "message");
    assert_eq!(advisory["state"], "security_alert");
    assert_eq!(advisory["needs_response"], true);
    // With no number to place it by, it falls back to its repository rather
    // than to a row that cannot be opened.
    assert_eq!(advisory["url"], "https://github.com/acme/widget");

    // Being kept informed is not being asked: `subscribed` is outside the
    // default `reasons`, so the release notification never becomes a row.
    assert!(!items
        .iter()
        .any(|item| item["id"] == "github-notifications:9004"));
}

#[test]
fn status_without_config_reports_error() {
    let tmp = tempdir();
    let mut cmd = ephor_cmd();
    cmd.env("EPHOR_STATUS_CONFIG", tmp.path().join("missing.json"));
    cmd.args(["status"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Cannot read feed config"));
}

/// custom-status is a summons like every other command ephor runs
/// (§AR-002-summons, §FS-006-project-interface.3): it is told about the project
/// in the one `EPHOR_*` vocabulary, it may answer in the published envelope by
/// writing `$EPHOR_ANSWER` (§FS-006-project-interface.4), and its exit code is
/// read the one way — `75` is parked, which for a status source is nothing to
/// report rather than a failure.
#[test]
fn custom_status_runs_as_a_summons_and_may_answer_in_the_envelope() {
    let tmp = tempdir();
    write_feed_fixture(tmp.path());
    let project_root = tmp.path().join("demo");
    let reporter = project_root.join("report.sh");
    make_executable(
        &reporter,
        r#"#!/usr/bin/env bash
set -euo pipefail
cat > "$EPHOR_ANSWER" <<JSON
{ "v": 1,
  "matters": [
    { "key": "release:$EPHOR_PROJECT", "kind": "status", "title": "cut from $EPHOR_WORKSPACE",
      "state": "warn", "url": "https://ci/release" }
  ],
  "needs_response": true }
JSON
"#,
    );

    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "projects": {
                "demo": {
                    "providers": [
                        { "provider": "custom-status", "command": reporter.to_string_lossy(),
                          "format": "answer" }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = ephor_cmd();
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();

    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let item = &cache["providers"]["custom-status"]["matters"][0];
    assert_eq!(item["key"], "release:demo");
    assert_eq!(item["state"], "warn");
    assert_eq!(item["needs_response"], true);
    // The dossier reached the command: it wrote back where it ran.
    assert_eq!(
        item["title"],
        format!("cut from {}", project_root.to_string_lossy())
    );

    // A parked source has nothing to report just now, which is not a failure.
    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "projects": {
                "demo": {
                    "providers": [
                        { "provider": "custom-status", "command": "exit 75" }
                    ]
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = ephor_cmd();
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh", "demo"]).assert().success();
    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(cache["providers"]["custom-status"]["ok"], true);
    assert_eq!(
        cache["providers"]["custom-status"]["matters"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

/// A source that asks nothing about any one project is fetched once for the
/// whole site, and the engine says where each of its findings belongs
/// (§AR-008-pipeline.1, §AR-003-attribution). A mention on a repository in a
/// project's *territory* — in no forest at all — lands on that project
/// (§FS-008-attribution.1), and what nothing claims is kept where it can be
/// looked at (§FS-008-attribution.4).
#[test]
fn a_shared_source_is_placed_by_the_engine_rather_than_by_itself() {
    let tmp = tempdir();
    write_feed_fixture(tmp.path());
    let fake_bin = tmp.path().join("fakebin");
    fs::create_dir_all(&fake_bin).unwrap();
    make_executable(&fake_bin.join("gh"), FAKE_GH_NOTICES);
    let path = format!(
        "{}:{}",
        fake_bin.to_string_lossy(),
        std::env::var("PATH").unwrap_or_default()
    );

    // The project claims one repository beyond its forest.
    let registry: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tmp.path().join("workspaces.json")).unwrap())
            .unwrap();
    let mut registry = registry;
    registry["projects"][0]["territory"] = json!(["other"]);
    fs::write(
        tmp.path().join("workspaces.json"),
        serde_json::to_string_pretty(&registry).unwrap(),
    )
    .unwrap();

    // The notification stream is a site-level source now, named nowhere near
    // a project.
    fs::write(
        tmp.path().join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 600, "provider_timeout_seconds": 10, "github_user": "tester" },
            "sources": [{ "provider": "github-notifications" }],
            "projects": { "demo": { "providers": [] } }
        }))
        .unwrap(),
    )
    .unwrap();

    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    cmd.args(["refresh"]).assert().success();

    // A notice about a repository in the project's territory is the project's,
    // though no source ever said so.
    let cache: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("state/ephor/feed/demo.json")).unwrap(),
    )
    .unwrap();
    let placed = cache["providers"]["github-notifications"]["matters"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        placed.iter().any(|matter| matter["key"]
            .as_str()
            .is_some_and(|key| key.contains("other/lib"))),
        "the territory repository should have landed on demo: {placed:#?}"
    );
    for matter in &placed {
        assert_eq!(matter["placement"]["on"]["project"], "demo");
    }

    // And what nothing claimed is kept where it can be looked at, rather than
    // dropped on the floor.
    let bucket = tmp.path().join("state/ephor/feed/unattributed.json");
    assert!(bucket.is_file(), "the bucket is part of the store");
    let mut cmd = ephor_cmd();
    cmd.env("PATH", &path);
    for (key, value) in feed_env(tmp.path()) {
        cmd.env(key, value);
    }
    let out = cmd
        .args(["feed", "--unattributed", "--json"])
        .assert()
        .success();
    let items: Vec<serde_json::Value> =
        serde_json::from_slice(&out.get_output().stdout).expect("feed --unattributed --json");
    assert!(
        items
            .iter()
            .all(|item| item["project"].as_str().unwrap_or("").is_empty()),
        "nothing in the bucket belongs to a project: {items:#?}"
    );
}
