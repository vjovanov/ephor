//! Unit tests for the interactive interface (§FS-011-command-line), moved out
//! of `mod.rs` so that file stays inside the source budget §FS-012-file-size.1
//! sets. Attached by `#[cfg(test)] #[path = "mod_tests.rs"] mod tests;` there,
//! on the precedent `src/work/mod_tests.rs` set, so this is the body of
//! `feed::tui::tests` and reads its parent through `use super::*`.

use std::collections::BTreeMap;

use super::*;

/// What a menu entry's summons is told it is about, asked of the one
/// implementation both surfaces use (§AR-009-surfaces.1).
fn dossier_of(menu: &ActionMenu, workspace: &Path) -> Vec<(String, String)> {
    let about = match menu.subject.item() {
        Some(item) => crate::api::act::About::Item(Box::new(item.clone())),
        None => crate::api::act::About::Branch {
            project: menu.subject.project().to_string(),
            branch: menu
                .branch
                .as_ref()
                .map(|branch| branch.branch.clone())
                .unwrap_or_default(),
        },
    };
    crate::api::act::dossier_of(&about, &menu.root, workspace, menu.branch.as_ref(), None)
}

use crate::branches::Placement;
use crate::capabilities::Rung;
use crate::feed::cache::Seen;
use crate::feed::model::ItemKind;
use crate::forest::{Staleness, Standing, Upstream};
use serde_json::json;

pub(super) fn ctx_with_branch(root: &Path, template: Option<&str>) -> Ctx {
    let branch = BranchInfo {
        branch: "you/ABC-42-retry-window".to_string(),
        ticket: Some("ABC-42".to_string()),
        active: true,
        is_release: false,
        declared: true,
    };
    let placement = Placement {
        project: "widget".to_string(),
        root: root.to_path_buf(),
        template: template.map(String::from),
        branches: vec![branch],
        main_branch: Some("master".to_string()),
        ..Placement::default()
    };
    Ctx {
        feeds: Vec::new(),
        seen: Seen::new(),
        projects: vec!["widget".to_string()],
        orgs: Vec::new(),
        project_org: BTreeMap::new(),
        placements: BTreeMap::from([("widget".to_string(), placement)]),
        behind: BTreeMap::new(),
        standing: BTreeMap::new(),
        on_branch: BTreeMap::new(),
        linked: BTreeMap::new(),
        stats: BTreeMap::new(),
        capabilities: BTreeMap::new(),
        resurfacing: BTreeMap::new(),
        unattributed: Vec::new(),
        actions: Vec::new(),
        project_actions: BTreeMap::new(),
        provider_blocks: BTreeMap::new(),
        checkouts: BTreeMap::new(),
        recent_days: 7,
        unread_only: true,
        ..Ctx::default()
    }
}

/// Give the fixture project a declared forest.
fn declare(ctx: &mut Ctx, repos: &[&str]) {
    let placement = ctx
        .placements
        .get_mut("widget")
        .expect("the fixture project");
    placement.repos = repos
        .iter()
        .map(|name| crate::forest::Declaration::at(*name))
        .collect();
}

fn ticket_item() -> Item {
    Item {
        id: "github-prs:acme/widget#42".to_string(),
        project: "widget".to_string(),
        source: "github-prs".to_string(),
        kind: ItemKind::Pr,
        role: None,
        title: "[ABC-42] Fix condition errors".to_string(),
        url: None,
        state: None,
        needs_response: false,
        updated_at: Utc::now(),
        raw: json!({}),
    }
}

/// An issue: no branch, because an issue has none until somebody cuts one.
fn issue_item() -> Item {
    Item {
        id: "github-issues:acme/widget#95".to_string(),
        kind: ItemKind::Issue,
        source: "github-issues".to_string(),
        title: "Durations read as seconds".to_string(),
        ..ticket_item()
    }
}

/// The root a surface asks about is the root the dispatch will use
/// (§FS-005-dispatch.14). An entry that says which branch its work belongs
/// on is dispatched inside the workspace that template names, so the hand
/// shown on its row and the roster its picker offers are read there — not
/// at the project root, which for a branch-addressable project holds no
/// change at all (§FS-005-dispatch.25).
#[test]
fn the_root_a_surface_asks_about_is_the_one_the_entry_would_be_dispatched_into() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
    let issue = issue_item();

    // Nothing said which branch: the matter is on none, so the answer is
    // the project's own root — which is what the dispatch refuses over
    // where the work edits the change.
    assert_eq!(
        ctx.work_root(&issue, None),
        Some(tmp.path().join("panta")),
        "the matter's own"
    );
    // And with the entry's template, the root inside the workspace that
    // template names — where its dispatch writes the plan.
    assert_eq!(
        ctx.work_root(&issue, Some("fix/issue-{number}")),
        Some(tmp.path().join("fix/issue-95/panta"))
    );
    // A matter with a branch of its own is never displaced by a template.
    let pr = ticket_item();
    assert_eq!(
        ctx.work_root(&pr, Some("fix/issue-{number}")),
        ctx.work_root(&pr, None)
    );
}

/// One project's cached feed, holding one matter the forge put on `branch`.
fn feed_on(project: &str, key: &str, branch: &str) -> ProjectFeed {
    let matter = crate::matter::Matter {
        key: crate::matter::SubjectKey::stated(key),
        kind: ItemKind::Pr,
        placement: crate::matter::Placement::on(project),
        source: "github-prs".to_string(),
        title: "Retry window".to_string(),
        role: None,
        url: None,
        state: None,
        needs_response: false,
        updated_at: Utc::now(),
        links: Vec::new(),
        discussions: Vec::new(),
        events: Vec::new(),
        fingerprint: Default::default(),
        raw: json!({ "branch": branch }),
    };
    ProjectFeed {
        project: project.to_string(),
        providers: BTreeMap::from([(
            "github-prs".to_string(),
            crate::feed::cache::ProviderSlot {
                ok: true,
                matters: vec![matter],
                ..Default::default()
            },
        )]),
        ..ProjectFeed::default()
    }
}

/// A second project beside the fixture's, with a branch and a feed of its
/// own, so a pass scoped to one has something to leave alone.
fn with_second_project(ctx: &mut Ctx) {
    let placement = Placement {
        project: "gadget".to_string(),
        branches: vec![BranchInfo {
            branch: "you/XYZ-7-widen".to_string(),
            ticket: Some("XYZ-7".to_string()),
            active: true,
            is_release: false,
            declared: true,
        }],
        ..ctx.placements["widget"].clone()
    };
    ctx.projects.push("gadget".to_string());
    ctx.placements.insert("gadget".to_string(), placement);
    ctx.feeds = vec![
        feed_on(
            "widget",
            "github-prs:acme/widget#42",
            "you/ABC-42-retry-window",
        ),
        feed_on("gadget", "github-prs:acme/gadget#7", "you/XYZ-7-widen"),
    ];
}

/// A refresh lands one project at a time, and the placement pass it runs
/// per landing answers for that project alone: the rest of the site keeps
/// the rows it had, and what the scoped pass leaves behind is what the
/// whole-site pass would have left there — one implementation, so the
/// mid-scan answer and the end-of-run answer cannot disagree
/// (§FS-001-forge-interface.7, §FS-008-attribution.2).
#[test]
fn a_landing_places_its_own_project_and_leaves_the_rest_standing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ctx = ctx_with_branch(tmp.path(), None);
    with_second_project(&mut ctx);
    ctx.recompute_placements();

    let widget = ctx.branches("widget")[0].clone();
    let gadget = ctx.branches("gadget")[0].clone();
    assert_eq!(ctx.branch_linked("widget", &widget), 1);
    assert_eq!(ctx.branch_linked("gadget", &gadget), 1);

    // Widget's feed lands again, this time with the item on no branch the
    // project knows. Only widget is re-placed.
    ctx.feeds[0] = feed_on("widget", "github-prs:acme/widget#42", "you/ABC-99-other");
    ctx.recompute_placements_for("widget");
    assert_eq!(ctx.branch_linked("widget", &widget), 0);
    assert_eq!(ctx.branch_linked("gadget", &gadget), 1);
    // The row that left the branch left the map with it — a stale entry
    // would keep filing it under a branch it is no longer on.
    assert!(ctx
        .on_branch
        .keys()
        .all(|(project, _)| project.as_str() != "widget"));

    // And the two scopes agree about the whole site.
    let scoped = (ctx.on_branch.clone(), ctx.linked.clone());
    ctx.recompute_placements();
    assert_eq!(scoped, (ctx.on_branch.clone(), ctx.linked.clone()));
}

#[test]
fn a_sources_own_action_leads_the_menu() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ctx = ctx_with_branch(tmp.path(), None);
    ctx.actions = vec![serde_json::from_value(json!({
        "icon": "🧪", "description": "run the gate", "command": "just gate"
    }))
    .unwrap()];
    ctx.provider_blocks = BTreeMap::from([(
        "widget".to_string(),
        vec![json!({ "provider": "github-ci", "repos": ["acme/widget"] })],
    )]);

    let mut ci = ticket_item();
    ci.id = "github-ci:acme/widget#42".to_string();
    ci.source = "github-ci".to_string();
    ci.kind = ItemKind::Pr;
    ci.state = None;
    // The gate rides on the pull request now, and the source's own action
    // is offered off the gate rather than off a state word.
    ci.raw = json!({
        "repo": "acme/widget",
        "gate": { "repos": [{
            "repo": "acme/widget", "passed": 1, "failed": 2, "running": 0
        }] }
    });

    // The configured action keeps its place and the source's own goes
    // ahead of it (§FS-004-quick-actions.3) — where `gh` is installed for
    // it to be offered at all.
    let menu = ctx.actions_with(&ci, &[], &[]);
    // The failures entry and both restarts (§FS-004-quick-actions.9),
    // where `gh` is installed for any of them to be offered.
    let quick = if crate::feed::provider::command_exists("gh") {
        3
    } else {
        0
    };
    assert_eq!(menu.len(), quick + 1);
    assert_eq!(menu.last().unwrap().description, "run the gate");
    if quick > 0 {
        assert_eq!(menu[0].description, "see the CI failures");
    }
}

/// Provenance orders the menu and a repeated id wins in place: what ephor
/// recognized, then what the project offers of itself, then the person's
/// own (§FS-006-project-interface.9). The project's offers arrive under
/// the trust the row extends to them (§FS-006-project-interface.2).
#[test]
fn the_menu_is_shipped_then_the_projects_then_the_persons() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(crate::manifest::FILE),
        r#"{"actions": [
             {"id": "bench", "description": "the project's benchmark",
              "command": "./bench.sh", "when": {"kinds": ["pr"]}},
             {"id": "nightly", "description": "only on a green gate",
              "command": "./nightly.sh", "when": {"gate": "green"}},
             {"id": "rebase", "description": "the project's own rebase",
              "command": "./rebase.sh"}
           ]}"#,
    )
    .unwrap();
    let mut ctx = ctx_with_branch(tmp.path(), None);
    ctx.actions = vec![serde_json::from_value(json!({
        "id": "bench", "icon": "🧪", "description": "my benchmark", "command": "just bench"
    }))
    .unwrap()];

    let menu = ctx.actions_with(&ticket_item(), &[], &[]);
    let described: Vec<&str> = menu
        .iter()
        .map(|action| action.description.as_str())
        .collect();
    // The item has no gate, so the offer asking for a green one is not
    // there at all; the person's `bench` replaced the project's, in the
    // place the project's held.
    assert_eq!(
        described,
        ["my benchmark", "the project's own rebase"],
        "{described:?}"
    );
    assert_eq!(menu[0].command, "just bench");

    // A row that trusts the checkout for descriptions only runs none of
    // what it offers.
    ctx.placements
        .get_mut("widget")
        .expect("the fixture project")
        .trust = crate::manifest::Trust::Descriptions;
    let menu = ctx.actions_with(&ticket_item(), &[], &[]);
    assert_eq!(menu.len(), 1, "only the person's own is left");
    assert_eq!(menu[0].description, "my benchmark");
}

#[test]
fn checkout_resolves_existing_branch_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace_dir = root.join("you/ABC-42-retry-window");
    std::fs::create_dir_all(&workspace_dir).unwrap();

    let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    let placed = ctx.checkout(&ticket_item()).unwrap();
    assert_eq!(placed.workspace, workspace_dir);
    assert_eq!(placed.ticket.as_deref(), Some("ABC-42"));
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

/// A fixture writes its refs the moment the test runs, so a distance
/// measured against one of them is dated today
/// (§FS-004-quick-actions.6). The repositories `repo_behind` builds have
/// no remote at all, so their base is a local branch nothing fetched and
/// their distances carry no day.
fn today() -> String {
    chrono::Local::now().format("%b %-d").to_string()
}

/// How far the item's checkout trails the project's main branch, out of
/// the one fold the offers read.
fn behind(ctx: &Ctx, item: &Item) -> Option<u64> {
    ctx.item_trailing(item)
        .and_then(|trailing| trailing.behind)
        .map(|trail| trail.behind)
}

/// A repo whose `feature` branch is `commits` commits behind `master`.
fn repo_behind(dir: &Path, commits: usize) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "master"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(dir, &["branch", "feature"]);
    for index in 0..commits {
        git(
            dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("ahead {index}"),
            ],
        );
    }
    git(dir, &["checkout", "-q", "feature"]);
}

#[test]
fn item_checkout_state_uses_recorded_branch_without_registry_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

    // A PR whose branch has no registry entry, resolved via raw.branch.
    let mut pr = ticket_item();
    pr.title = "Unrelated title".to_string();
    pr.raw = json!({ "branch": "someone/feature" });
    assert_eq!(ctx.item_checked_out(&pr), Some(false));
    std::fs::create_dir_all(root.join("someone/feature")).unwrap();
    assert_eq!(ctx.item_checked_out(&pr), Some(true));
    assert_eq!(
        ctx.checkout(&pr).unwrap().workspace,
        root.join("someone/feature")
    );

    // No branch information at all: state is unknown.
    pr.raw = json!({});
    assert_eq!(ctx.item_checked_out(&pr), None);
    assert!(matches!(
        ctx.checkout(&pr).unwrap().state,
        WorkspaceState::Unmatched
    ));
}

#[test]
fn behind_sums_across_workspace_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_behind(&workspace.join("ce"), 2);
    repo_behind(&workspace.join("ee"), 3);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce", "ee"]);
    ctx.recompute_behind();
    let staleness = ctx
        .branch_behind("widget", "you/ABC-42-retry-window")
        .expect("both repositories were measured");
    assert_eq!(staleness.total(), Some(5));
    // The sum is reported, and which repository it came from survives it
    // (§AR-004-forest.1).
    assert_eq!(
        staleness.summary().as_deref(),
        Some("5 behind (ce 2, ee 3)")
    );
}

/// The standing rides beside the behind count, from the same fold: two
/// distances, two facts — one against the project's main branch, one
/// against the branch's own published copy, and the branch is read off
/// each repository's `HEAD`, never the workspace directory's name
/// (§DA-003-upstream-is-the-published-copy).
#[test]
fn the_standing_is_measured_beside_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    let repo = workspace.join("ce");
    repo_behind(&repo, 3);
    // The branch was pushed, then its copy grew two commits this
    // checkout has not pulled — no tracking config, the worktree shape.
    for step in 0..2 {
        git(
            &repo,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("pushed {step}"),
            ],
        );
    }
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/feature", "HEAD"],
    );
    git(&repo, &["reset", "-q", "--hard", "HEAD~2"]);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce"]);
    ctx.recompute_behind();
    assert_eq!(
        ctx.branch_behind("widget", "you/ABC-42-retry-window")
            .and_then(Staleness::total),
        Some(3)
    );
    let standing = ctx
        .branch_standing("widget", "you/ABC-42-retry-window")
        .expect("the copy was read");
    assert_eq!(standing.behind_upstream(), Some(2));
    assert_eq!(standing.repos[0].branch.as_deref(), Some("feature"));
    assert_eq!(
        standing.repos[0].upstream,
        Upstream::Published {
            remote: "origin".to_string(),
            branch: "feature".to_string(),
        }
    );
}

/// The rebase is in the menu because of what is on disk, and only then
/// (§FS-004-quick-actions.6).
#[test]
fn the_rebase_is_offered_on_a_checkout_that_trails_main() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_behind(&workspace.join("ce"), 2);
    repo_behind(&workspace.join("ee"), 3);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce", "ee"]);
    let pr = ticket_item();
    assert_eq!(behind(&ctx, &pr), Some(5));
    let menu = ctx.actions_with(&pr, &[], &[]);
    assert_eq!(menu[0].description, "rebase onto master (5 behind)");
    assert!(menu[0].command.contains("rebase --project"));
    assert!(menu[0].requires_checkout);

    // Level with master: still offered, because the reading that says
    // level is only as fresh as the last fetch and the replay is what
    // would refresh it — and the entry says *level* rather than a count
    // (§FS-004-quick-actions.6).
    for repo in ["ce", "ee"] {
        git(&workspace.join(repo), &["checkout", "-q", "master"]);
    }
    assert_eq!(behind(&ctx, &pr), Some(0));
    let level = ctx.actions_with(&pr, &[], &[]);
    assert_eq!(level.len(), 1, "{level:?}");
    assert_eq!(level[0].id, "rebase");
    assert_eq!(level[0].description, "rebase onto master (level)");
}

#[test]
fn the_rebase_is_not_offered_where_there_is_nothing_to_measure() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));

    // The branch workspace was never checked out.
    assert_eq!(behind(&ctx, &ticket_item()), None);
    assert!(ctx.actions_with(&ticket_item(), &[], &[]).is_empty());

    // An item that resolves to no branch at all has nowhere to rebase,
    // whatever kind it is (§FS-004-quick-actions.2).
    let mut nowhere = ticket_item();
    nowhere.title = "Nothing about any branch".to_string();
    assert_eq!(behind(&ctx, &nowhere), None);
    assert!(ctx.actions_with(&nowhere, &[], &[]).is_empty());
}

/// The offer follows the branch on disk, not the kind of the row that
/// mentions it: an issue and a message about the same change are offered
/// exactly what the pull request is (§FS-004-quick-actions.6).
#[test]
fn any_item_that_resolves_to_a_workspace_is_offered_the_rebase() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_behind(&workspace.join("ce"), 4);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce"]);
    let offered = "rebase onto master (4 behind)";
    for kind in [
        ItemKind::Pr,
        ItemKind::Issue,
        ItemKind::Task,
        ItemKind::Message,
        ItemKind::Ci,
        ItemKind::Status,
    ] {
        let mut item = ticket_item();
        item.kind = kind;
        let menu = ctx.actions_with(&item, &[], &[]);
        assert_eq!(menu.len(), 1, "{kind:?}: {menu:?}");
        assert_eq!(menu[0].description, offered, "{kind:?}");
        // And the entry says nothing about kinds any more, so nothing
        // downstream can narrow it back to pull requests.
        assert!(menu[0].kinds.is_empty(), "{kind:?}");
    }
}

/// The two offers are gated apart: replaying onto the published copy
/// resolves its ref inside each repository, so a project that declares no
/// main branch is still offered it — and is offered nothing to replay onto
/// a base nothing names (§FS-004-quick-actions.6).
#[test]
fn a_project_with_no_main_branch_is_still_offered_the_copy() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let repo = root.join("you/ABC-42-retry-window/ce");
    repo_behind(&repo, 3);
    published_ahead(&repo, "feature", 2);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce"]);
    ctx.placements
        .get_mut("widget")
        .expect("the fixture project")
        .main_branch = None;

    let menu = ctx.actions_with(&ticket_item(), &[], &[]);
    assert_eq!(menu.len(), 1, "{menu:?}");
    assert_eq!(menu[0].id, "rebase-upstream");
    assert_eq!(
        menu[0].description,
        format!("rebase onto origin/feature (2 behind as of {})", today())
    );

    // The row is gated the same way, so what it shows and what the menu
    // offers cannot disagree: the copy's distance, and no distance to a
    // main branch the project never named.
    ctx.recompute_behind();
    assert!(ctx
        .branch_behind("widget", "you/ABC-42-retry-window")
        .is_none());
    assert_eq!(
        ctx.branch_standing("widget", "you/ABC-42-retry-window")
            .and_then(Standing::behind_upstream),
        Some(2)
    );
}

/// The branch row carries the same offers, built by the same code: this is
/// where a reader looking at a stale branch is standing
/// (§FS-004-quick-actions.6).
#[test]
fn a_branch_row_carries_the_same_offers_as_the_items_on_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_behind(&workspace.join("ce"), 2);
    published_ahead(&workspace.join("ce"), "feature", 1);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce"]);
    let offered = ctx.branch_actions("widget", "you/ABC-42-retry-window");
    assert_eq!(offered.len(), 2, "{offered:?}");
    assert_eq!(offered[0].description, "rebase onto master (2 behind)");
    assert_eq!(
        offered[1].description,
        format!("rebase onto origin/feature (1 behind as of {})", today())
    );

    // The same entries the item's menu carries — one implementation, so a
    // reader cannot be told two different things about one checkout.
    let menu = ctx.actions_with(&ticket_item(), &[], &[]);
    let described = |actions: &[ActionConfig]| -> Vec<(String, String)> {
        actions
            .iter()
            .map(|action| (action.id.clone(), action.command.clone()))
            .collect()
    };
    assert_eq!(described(&offered), described(&menu));

    // A branch whose workspace is not on disk is a checkout question
    // (§FS-004-quick-actions.7), so the rebase is withheld rather than
    // offered and left to fail.
    assert!(ctx
        .branch_actions("widget", "you/never-checked-out")
        .is_empty());
}

/// Publish the branch this repository is on and move that copy `commits`
/// ahead of the checkout — somebody else pushed to it, and no tracking
/// config was ever written (§DA-003-upstream-is-the-published-copy).
fn published_ahead(dir: &Path, branch: &str, commits: usize) {
    for index in 0..commits {
        git(
            dir,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("pushed {index}"),
            ],
        );
    }
    git(
        dir,
        &[
            "update-ref",
            &format!("refs/remotes/origin/{branch}"),
            "HEAD",
        ],
    );
    if commits > 0 {
        git(dir, &["reset", "-q", "--hard", &format!("HEAD~{commits}")]);
    }
}

/// A repository parked on the base itself and tracking it, whose copy is
/// `commits` ahead: the workspace repository a change does not touch. Its
/// published copy *is* its base, so both distances are the same distance.
fn repo_on_the_base(dir: &Path, commits: usize) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "master"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", "base"]);
    published_ahead(dir, "master", commits);
    git(dir, &["remote", "add", "origin", "."]);
    git(
        dir,
        &["branch", "--set-upstream-to=origin/master", "master"],
    );
}

/// The second offer: onto the branch's own published copy, naming the ref
/// so the two entries differ in the word that matters
/// (§FS-004-quick-actions.8).
#[test]
fn the_rebase_onto_the_published_copy_is_offered_and_names_the_ref() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let repo = root.join("you/ABC-42-retry-window/ce");
    // Level with main, so only the published copy has anything to replay.
    repo_behind(&repo, 0);
    published_ahead(&repo, "feature", 2);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce"]);
    let pr = ticket_item();
    let menu = ctx.actions_with(&pr, &[], &[]);
    assert_eq!(menu.len(), 2, "{menu:?}");
    assert_eq!(menu[1].id, "rebase-upstream");
    assert_eq!(
        menu[1].description,
        format!("rebase onto origin/feature (2 behind as of {})", today())
    );
    assert!(menu[1].command.contains("rebase --upstream --project"));
    assert!(menu[1].requires_checkout);

    // Level with the copy: still offered, and labelled *level* — the
    // distance to a copy is measured against what was last fetched too
    // (§FS-004-quick-actions.8).
    git(
        &repo,
        &["update-ref", "refs/remotes/origin/feature", "HEAD"],
    );
    let level = ctx.actions_with(&pr, &[], &[]);
    assert_eq!(level.len(), 2, "{level:?}");
    assert_eq!(
        level[1].description,
        format!("rebase onto origin/feature (level as of {})", today())
    );

    // A branch published nowhere has no copy to name and no reading a
    // fetch would correct, so this entry alone goes.
    git(&repo, &["update-ref", "-d", "refs/remotes/origin/feature"]);
    let unpushed = ctx.actions_with(&pr, &[], &[]);
    assert_eq!(unpushed.len(), 1, "{unpushed:?}");
    assert_eq!(unpushed[0].id, "rebase");
}

/// A forest where the repositories disagree — one on the change's branch,
/// one parked on the base — is offered both, because a forest is not one
/// branch (§FS-004-quick-actions.8). The copy entry counts, and names,
/// only the repository that trails a copy of its own: the parked one's
/// distance is the first entry's, not this one's twice.
#[test]
fn both_rebases_are_offered_where_the_forest_disagrees() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_behind(&workspace.join("ce"), 0);
    published_ahead(&workspace.join("ce"), "feature", 2);
    repo_on_the_base(&workspace.join("ee"), 1);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce", "ee"]);
    let menu = ctx.actions_with(&ticket_item(), &[], &[]);
    assert_eq!(menu.len(), 2);
    assert_eq!(menu[0].description, "rebase onto master (1 behind)");
    // `ee`'s copy is its base, so it neither counts here nor keeps the
    // entry from naming the one ref the counted repositories share.
    assert_eq!(menu[1].id, "rebase-upstream");
    assert_eq!(
        menu[1].description,
        format!("rebase onto origin/feature (2 behind as of {})", today())
    );
}

/// And where every repository's published copy *is* its base, the copy
/// entry has nothing of its own to count: only the first is offered
/// (§FS-004-quick-actions.8).
#[test]
fn the_rebase_onto_the_copy_is_not_offered_where_the_copy_is_the_base() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_on_the_base(&workspace.join("ce"), 1);
    repo_on_the_base(&workspace.join("ee"), 1);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce", "ee"]);
    // The distance is real, and the base count carries it; the copy sum
    // leaves it out entirely, so no gate anywhere reads one distance
    // under two names.
    let trailing = ctx
        .item_trailing(&ticket_item())
        .expect("the checkout was measured");
    assert_eq!(trailing.behind.map(|trail| trail.behind), Some(2));
    assert_eq!(trailing.behind_upstream, None);
    let menu = ctx.actions_with(&ticket_item(), &[], &[]);
    assert_eq!(menu.len(), 1);
    assert_eq!(menu[0].id, "rebase");
    assert_eq!(
        menu[0].description,
        format!("rebase onto master (2 behind as of {})", today())
    );
}

/// A red gate on my own change, on a checkout that trails: the commands
/// and the work stand in one menu (§FS-005-dispatch.1), each carrying its
/// own icon, and the replay appears once — the recipe named `rebase` is
/// what that entry hands its conflict to, not a second row saying the same
/// thing.
#[test]
fn the_menu_carries_the_work_that_can_be_handed_over() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let workspace = root.join("you/ABC-42-retry-window");
    repo_behind(&workspace.join("ce"), 2);

    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    declare(&mut ctx, &["ce"]);
    let mut mine = ticket_item();
    mine.role = Some(crate::feed::model::ItemRole::Author);
    mine.raw = json!({ "gate": { "repos": [
        { "repo": "ce", "passed": 1, "failed": 2, "running": 0 }
    ] } });

    let recipes = crate::work::recipe::shipped();
    let menu = ctx.actions_with(&mine, &recipes, &[]);
    let described: Vec<(&str, &str, bool)> = menu
        .iter()
        .map(|entry| {
            (
                entry.icon.as_str(),
                entry.description.as_str(),
                entry.agent.is_some(),
            )
        })
        .collect();
    assert_eq!(
        described,
        [
            ("⤴", "rebase onto master (2 behind)", false),
            ("🛠", "fix the red gate", true),
        ],
        "{described:?}"
    );
    // The work rides on the entry whole, so what is dispatched from the
    // menu is the recipe itself (§FS-005-dispatch.4).
    let work = menu[1].agent.as_ref().expect("the recipe rides along");
    assert_eq!(work.id, "fix-gate");
    assert!(work.brief.starts_with("The gate on {title} is red."));
    // And the replay is one entry, the deterministic one.
    assert_eq!(menu.iter().filter(|entry| entry.id == "rebase").count(), 1);
}

/// Offered only where it would work (§FS-004-quick-actions.2): work that
/// edits the change waits on the change being here, work that reads one
/// does not, and nothing is asked about an item that is finished
/// (§FS-005-dispatch.6).
#[test]
fn work_is_offered_where_it_would_work_and_nowhere_else() {
    let tmp = tempfile::tempdir().unwrap();
    // Nothing checked out: the branch workspace the template names is not
    // on disk.
    let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
    let recipes = crate::work::recipe::shipped();
    let ids = |ctx: &Ctx, item: &Item| -> Vec<String> {
        ctx.actions_with(item, &recipes, &[])
            .into_iter()
            .filter(|entry| entry.agent.is_some())
            .map(|entry| entry.id)
            .collect()
    };

    // Fixing a gate edits the change, so it is the checkout's question
    // first (§FS-004-quick-actions.7).
    let mut mine = ticket_item();
    mine.role = Some(crate::feed::model::ItemRole::Author);
    mine.raw = json!({ "gate": { "repos": [
        { "repo": "ce", "passed": 1, "failed": 2, "running": 0 }
    ] } });
    assert!(ids(&ctx, &mine).is_empty());

    // Reviewing one reads it, and fetches what it needs: offered with
    // nothing on disk at all.
    let mut theirs = ticket_item();
    theirs.role = Some(crate::feed::model::ItemRole::Reviewer);
    assert_eq!(ids(&ctx, &theirs), ["review"]);

    // Merged: there is nothing to ask for about it any more.
    let mut done = theirs.clone();
    done.state = Some("merged".to_string());
    assert!(ids(&ctx, &done).is_empty());
}

/// With no runner bound the work is still offered — a ticket is written
/// whether or not anything can run it — and where the entry would say who
/// gets it, it says instead that nobody can be asked, in the *workable*
/// rung's own words (§FS-005-dispatch.14).
#[test]
fn with_no_runner_bound_the_work_is_still_offered_and_says_nobody_can_be_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
    let mut theirs = ticket_item();
    theirs.role = Some(crate::feed::model::ItemRole::Reviewer);
    let offered = ctx.actions_with(&theirs, &crate::work::recipe::shipped(), &[]);
    assert_eq!(
        offered.iter().filter(|entry| entry.agent.is_some()).count(),
        1
    );

    // The rung's own sentence about a runner that is not there.
    let unbound = crate::work::runtime::refusal(&crate::work::recipe::WorkConfig {
        runner: Some("no-such-runner-here".to_string()),
        ..crate::work::recipe::WorkConfig::default()
    })
    .expect("a runner that is not on PATH is refused");
    assert!(unbound.contains("no-such-runner-here"), "{unbound}");

    use crate::work::runtime::roster::{Choice, Hand};
    let nobody = crate::api::session::who_gets_it(&Choice::Unasked { note: None }, Some(&unbound));
    assert_eq!(nobody.says, unbound);
    // Said, not refused: the ticket is written all the same.
    assert!(nobody.refusal.is_none());

    // With a runner there and nobody named, the runtime picks unasked.
    let unasked = crate::api::session::who_gets_it(&Choice::Unasked { note: None }, None);
    assert_eq!(unasked.says, "whoever the runtime picks");

    // A chosen hand names itself, and carries why it cannot be asked right
    // now rather than vanishing (§FS-005-dispatch.14).
    let chosen = crate::api::session::who_gets_it(
        &Choice::Chosen {
            hand: Hand {
                id: "luna".to_string(),
                agent: Some("claude-code".to_string()),
                model: None,
                provider: None,
                efforts: vec!["high".to_string()],
                available: Some("'claude-code' is not on PATH".to_string()),
            },
            effort: Some("high".to_string()),
            whence: "the site's default hand".to_string(),
            pool: Some("claude-code".to_string()),
            said: None,
            note: None,
        },
        None,
    );
    assert_eq!(
        chosen.says,
        "luna at high (unavailable: 'claude-code' is not on PATH)"
    );
    assert!(chosen.refusal.is_none());

    // And a choice that cannot stand is the whole reason, and refuses.
    let refused =
        crate::api::session::who_gets_it(&Choice::Refused("permits only sonnet".to_string()), None);
    assert_eq!(refused.refusal.as_deref(), Some("permits only sonnet"));
}

#[test]
fn behind_skips_unchecked_branches_and_non_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // Workspace missing entirely: no entry.
    let mut ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    ctx.recompute_behind();
    assert!(ctx.behind.is_empty());

    // Workspace exists but is not a git repo: no entry either.
    std::fs::create_dir_all(root.join("you/ABC-42-retry-window")).unwrap();
    ctx.recompute_behind();
    assert!(ctx.behind.is_empty());
}

/// The table is what the surfaces read, and it is honest about time: a
/// checkout that appears buys the rungs that were waiting on it
/// (§AR-005-capabilities.1, §AR-005-capabilities.3).
#[test]
fn the_capability_table_is_resolved_per_project_and_again_when_the_world_moves() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("widget");
    let mut ctx = ctx_with_branch(&root, Some("{project_root}/{branch}"));
    ctx.recompute_capabilities();

    // Nothing on disk: placed fails, and what cannot be looked in says so.
    let can = ctx.can("widget");
    assert!(!can.holds(Rung::Placed));
    assert!(!can.holds(Rung::Checkable));
    assert!(!can.holds(Rung::Tasks));
    assert!(can.holds(Rung::BranchAddressable));
    assert!(can
        .refusal(&[Rung::Placed])
        .unwrap()
        .contains("is not on disk"));

    // The project arrives, with a check verb and a task store in it.
    std::fs::create_dir_all(root.join("panta")).unwrap();
    std::fs::write(root.join("check.sh"), "#!/bin/sh\n").unwrap();
    ctx.recompute_capabilities();
    let can = ctx.can("widget");
    assert!(can.holds(Rung::Placed));
    assert!(can.holds(Rung::Checkable));
    assert!(can.holds(Rung::Tasks));

    // A project the registry says nothing about holds nothing, and the
    // table answers rather than being absent.
    assert!(ctx.can("ghost").held().is_empty());
}

/// The checkout offered on a branch row can actually run. The entry runs
/// `ephor checkout`, which needs to be told a branch or a matter it can
/// read one off (§FS-004-quick-actions.7); a branch row has no matter
/// (§FS-004-quick-actions.6), so the dossier says the branch and says the
/// item id empty rather than leaving a stale inherited one to bind the
/// command to somebody else's change. An offer refused on the keystroke is
/// worse than no offer (§FS-004-quick-actions.2).
#[test]
fn a_branch_rows_checkout_is_told_the_branch_and_no_matter() {
    let tmp = tempfile::tempdir().unwrap();
    let ctx = ctx_with_branch(tmp.path(), Some("{project_root}/{branch}"));
    let branch = ctx.branches("widget")[0].clone();
    let target = tmp.path().join(&branch.branch);
    let entry = crate::api::offers::checkout_action(&target);
    // Both are named, so the one command serves an item row and a branch
    // row alike.
    assert!(entry.command.contains("--item \"$EPHOR_ITEM_ID\""));
    assert!(entry.command.contains("--branch \"$EPHOR_BRANCH\""));

    let menu = ActionMenu::new(
        actions::Subject::Branch {
            project: "widget".to_string(),
            branch: branch.branch.clone(),
        },
        tmp.path().to_path_buf(),
        tmp.path().to_path_buf(),
        Some(branch.clone()),
        WorkspaceState::Missing(target),
        None,
        &ctx.can("widget"),
        Vec::new(),
    );
    let carried = dossier_of(&menu, tmp.path());
    let value = |key: &str| {
        carried
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(value("EPHOR_BRANCH"), Some(branch.branch.as_str()));
    assert_eq!(value("EPHOR_TICKET"), Some("ABC-42"));
    assert_eq!(value("EPHOR_PROJECT"), Some("widget"));
    // Said, and said empty: an unset variable is whatever the shell that
    // launched ephor held.
    assert_eq!(value("EPHOR_ITEM_ID"), Some(""));

    // An item row is unchanged: its own id, and its own branch.
    let item_menu = ActionMenu::new(
        actions::Subject::Item(Box::new(ticket_item())),
        tmp.path().to_path_buf(),
        tmp.path().to_path_buf(),
        Some(branch.clone()),
        WorkspaceState::Ready,
        None,
        &ctx.can("widget"),
        Vec::new(),
    );
    let carried = dossier_of(&item_menu, tmp.path());
    let value = |key: &str| {
        carried
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(value("EPHOR_ITEM_ID"), Some(ticket_item().id.as_str()));
    assert_eq!(value("EPHOR_BRANCH"), Some(branch.branch.as_str()));
}

#[test]
fn checkout_falls_back_to_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    // Branch matched but its workspace directory does not exist.
    let ctx = ctx_with_branch(root, Some("{project_root}/{branch}"));
    let placed = ctx.checkout(&ticket_item()).unwrap();
    assert_eq!(placed.workspace, root);
    assert!(placed.branch.is_some());

    // No branch template at all (plain single-checkout project).
    let ctx = ctx_with_branch(root, None);
    assert_eq!(ctx.checkout(&ticket_item()).unwrap().workspace, root);
}
