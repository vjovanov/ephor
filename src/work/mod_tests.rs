//! Unit tests for the dispatch module (§FS-005-dispatch), moved out of
//! `mod.rs` so that file stays inside the source budget §FS-012-file-size.2
//! sets. Attached by `#[cfg(test)] #[path = "mod_tests.rs"] mod tests;` there,
//! so this is the body of `work::tests` and reads its parent through
//! `use super::*`.

use super::*;
use crate::branches::BranchInfo;
use std::fs;
use std::path::Path;

fn placement(project: &str, root: &Path, template: Option<&str>) -> Placement {
    Placement {
        project: project.to_string(),
        root: root.to_path_buf(),
        template: template.map(String::from),
        branches: Vec::new(),
        main_branch: None,
        repos: Vec::new(),
        aliases: Vec::new(),
        territory: Vec::new(),
        trust: Default::default(),
    }
}

fn branch(name: &str) -> BranchInfo {
    BranchInfo {
        branch: name.to_string(),
        ticket: None,
        active: false,
        is_release: false,
        declared: true,
    }
}

fn plant(dir: &Path, plan: &str, title: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(plan), format!("# Rhei: {title}\n")).unwrap();
}

/// The work roots are enumerated from the configured places
/// (§FS-005-dispatch.15): the project's own checkout and each branch
/// workspace on disk — the work root is per branch workspace, and each
/// is its own execution root. A declared branch with no workspace yet is
/// skipped, not guessed at; a plan the ledger knows keeps its matter and
/// its title, and one found beside it arrives item-less.
#[test]
fn the_roots_are_the_configured_places_and_the_ledger_keeps_its_matter() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("widget");
    let mut widget = placement("widget", &root, Some("{project_root}/branches/{branch}"));
    widget.branches = vec![branch("you/ABC-1"), branch("you/ABC-2-unmade")];
    // The project's own tickets, and a branch workspace holding a
    // dispatched plan beside a hand-written one.
    plant(&root.join("panta"), "housekeeping.rhei.md", "Housekeeping");
    let workspace = root.join("branches/you/ABC-1");
    plant(
        &workspace.join("panta"),
        "forge-widget-7.rhei.md",
        "ledger's",
    );
    plant(&workspace.join("panta"), "audit.rhei.md", "Audit the paths");

    let mut ledger = Ledger {
        version: 1,
        entries: BTreeMap::new(),
        starts: BTreeMap::new(),
    };
    ledger.entries.insert(
        "forge:widget/7".to_string(),
        Entry {
            project: "widget".to_string(),
            title: "Widen the retry window".to_string(),
            url: None,
            root: workspace.join("panta"),
            checkout: workspace.clone(),
            branch: None,
            plan_id: "forge-widget-7".to_string(),
            plan: workspace.join("panta/forge-widget-7.rhei.md"),
            dispatches: Vec::new(),
        },
    );

    let groups = enumerate_roots(
        &WorkConfig::default(),
        &BTreeMap::new(),
        std::slice::from_ref(&widget),
        &ledger,
    );
    assert_eq!(groups.len(), 2, "{:?}", roots_of(&groups));
    let by_root = |dir: &Path| {
        let dir = fs::canonicalize(dir).unwrap();
        groups
            .iter()
            .find(|group| group.root == dir)
            .unwrap_or_else(|| panic!("{} should be a root", dir.display()))
    };
    let own = by_root(&root.join("panta"));
    assert_eq!(own.plans.len(), 1);
    assert_eq!(own.plans[0].plan_id, "housekeeping");
    assert_eq!(own.plans[0].item, None);

    let dispatched = by_root(&workspace.join("panta"));
    assert_eq!(dispatched.plans.len(), 2);
    // The ledger's plan first, with its matter and its title; the
    // hand-written one beside it, item-less.
    assert_eq!(dispatched.plans[0].item.as_deref(), Some("forge:widget/7"));
    assert_eq!(dispatched.plans[0].title, "Widen the retry window");
    assert_eq!(dispatched.plans[1].plan_id, "audit");
    assert_eq!(dispatched.plans[1].item, None);
    assert_eq!(dispatched.plans[1].title, "");
}

fn roots_of(groups: &[runtime::watch::RootPlans]) -> Vec<&Path> {
    groups.iter().map(|group| group.root.as_path()).collect()
}

/// A stand-in runtime with the transition verb the cancel asks for
/// (§FS-005-dispatch.16): it moves the named ticket's state line and
/// records the result the way the shipped binding does — the plan's own
/// state line, and `runtime/results/<plan>.<ticket>.md`. Anything else
/// it is asked, it refuses, in a sentence of its own.
const STAND_IN_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="${1:-}"; shift || true
[ "$verb" = transition ] || { echo "  × stand-in: no verb '$verb'" >&2; exit 2; }
plan="$1"; shift
task=""; from=""; to=""; result=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --task) task="$2"; shift 2 ;;
    --from) from="$2"; shift 2 ;;
    --to) to="$2"; shift 2 ;;
    --result) result="$2"; shift 2 ;;
    *) shift ;;
  esac
done
grep -q "^### Task $task:" "$plan" || { echo "  × stand-in: no task '$task'" >&2; exit 1; }
if [ "$from" = "fix" ]; then
  echo "  × Task $task cannot leave state fix." >&2
  echo "  │ Missing required output artifact: report" >&2
  exit 1
fi
awk -v task="$task" -v to="$to" '
  /^### Task / { current = $3; sub(":", "", current) }
  /^\*\*State:\*\*/ && current == task { print "**State:** " to; next }
  { print }
' "$plan" > "$plan.tmp" && mv "$plan.tmp" "$plan"
stem="$(basename "$plan" .rhei.md)"
mkdir -p "$(dirname "$plan")/runtime/results"
printf '## Result\n\n%s\n' "$result" >> "$(dirname "$plan")/runtime/results/$stem.$task.md"
echo "Task $stem.$task transitioned: '$from' → '$to'"
"#;

/// A work root under the shipped machine, holding one plan with three
/// tickets — one done, two open, the third ordered after the second — and
/// the config binding the stand-in runtime as the runner.
fn root_with_plan(tmp: &Path) -> (WorkConfig, PathBuf, PathBuf) {
    let root = tmp.join("panta");
    WorkRoot::ensure(&root, plan::SHIPPED_STATES).unwrap();
    let plan_path = root.join("forge-demo-17.rhei.md");
    fs::write(
        &plan_path,
        concat!(
            "# Rhei: demo\n**States:** ephor-work\n\n## Tasks\n\n",
            "### Task fix-gate-1: one\n**State:** done\n\nbody\n\n",
            "### Task fix-gate-2: two\n**State:** review\n**Prior:** Task fix-gate-1\n\nbody\n\n",
            "### Task fix-gate-3: three\n**State:** review\n**Prior:** Task fix-gate-2\n\nbody\n\n",
            "### Task ask-1: four\n**State:** fix\n\nbody\n",
        ),
    )
    .unwrap();
    let runner = tmp.join("stand-in-runtime");
    fs::write(&runner, STAND_IN_RUNTIME).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // The runner is named by path and looked up as one word, so unlike the
    // other stand-ins it has to be executed rather than read. Settle it
    // before any test runs it: `exec` on a file this process just wrote
    // fails with `ETXTBSY` while a child another thread forked still holds
    // it open, and the `exec` that trips is the shell's, where nothing can
    // wait it out.
    crate::seams::summons::settle_executable(&runner);
    let config = WorkConfig {
        runner: Some(runner.to_string_lossy().into_owned()),
        ..WorkConfig::default()
    };
    (config, root, plan_path)
}

fn entry_for(root: &Path, plan_path: &Path) -> Entry {
    Entry {
        project: "demo".to_string(),
        title: "demo".to_string(),
        url: None,
        root: root.to_path_buf(),
        checkout: root.parent().unwrap().to_path_buf(),
        branch: None,
        plan_id: "forge-demo-17".to_string(),
        plan: plan_path.to_path_buf(),
        dispatches: Vec::new(),
    }
}

/// A cancel is the runtime's move: the stand-in moves the state line and
/// records the reason, ephor reads both back — the ticket taken back,
/// with its reason as its line — and names what was ordered after it.
/// A reason left blank is recorded as exactly that; a dry run moves
/// nothing; and a second cancel of the same ticket is refused
/// (§FS-005-dispatch.16).
#[test]
fn a_cancel_asks_the_runtime_names_what_it_leaves_waiting_and_is_read_back() {
    let tmp = tempfile::tempdir().unwrap();
    let (config, root, plan_path) = root_with_plan(tmp.path());

    let dry = cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-2",
        "",
        true,
    )
    .expect("a dry run answers");
    assert_eq!(dry.left_waiting, vec!["fix-gate-3"]);
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("**State:** review\n**Prior:** Task fix-gate-1"));

    let done = cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-2",
        "asked twice",
        false,
    )
    .expect("the stand-in agrees");
    assert_eq!(done.from, "review");
    assert_eq!(done.left_waiting, vec!["fix-gate-3"]);
    assert!(
        done.describe().contains("fix-gate-3 is ordered after it"),
        "{}",
        done.describe()
    );
    let text = fs::read_to_string(&plan_path).unwrap();
    assert!(
        text.contains("### Task fix-gate-2: two\n**State:** cancelled"),
        "{text}"
    );
    assert!(
        text.contains("### Task fix-gate-3: three\n**State:** review"),
        "{text}"
    );

    // Read back: taken back, with the reason as its line; the badge says
    // taken back where that is the last word (§FS-005-dispatch.16).
    let status = status_of_entry(&config, &entry_for(&root, &plan_path), None);
    let two = status
        .tickets
        .iter()
        .find(|t| t.id == "fix-gate-2")
        .unwrap();
    assert!(two.cancelled && two.finished);
    assert_eq!(two.verdict.as_deref(), Some("asked twice"));
    assert_eq!(status.open_tickets(), 2);

    // Blank is recorded as blank, and said so on the row.
    cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-3",
        "   ",
        false,
    )
    .unwrap();
    let status = status_of_entry(&config, &entry_for(&root, &plan_path), None);
    let three = status
        .tickets
        .iter()
        .find(|t| t.id == "fix-gate-3")
        .unwrap();
    assert_eq!(three.verdict.as_deref(), Some(CANCELLED_UNSAID));

    // Already taken back: nothing to do, said rather than asked again.
    let again = cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-2",
        "",
        false,
    )
    .expect_err("already cancelled");
    assert!(again.to_string().contains("already cancelled"), "{again}");

    // The next ticket ephor writes follows the last one not taken back.
    let plan = Plan::read(&plan_path).unwrap().unwrap();
    assert_eq!(plan.last_ticket().map(|t| t.id), Some("ask-1".to_string()));
}

/// Everything ephor can see for itself is refused before the runtime is
/// asked, in one sentence each; and what the runtime refuses comes back
/// in its own words (§FS-005-dispatch.16, §DA-005-cancel-is-the-runtimes-move).
#[test]
fn a_cancel_refuses_on_what_the_artifacts_say_and_relays_what_the_runtime_says() {
    let tmp = tempfile::tempdir().unwrap();
    let (config, root, plan_path) = root_with_plan(tmp.path());
    let refuse = |config: &WorkConfig, ticket: &str| {
        cancel_ticket(
            config,
            &root,
            "forge-demo-17",
            &plan_path,
            ticket,
            "",
            false,
        )
        .expect_err("refused")
        .to_string()
    };

    // No runner: the workable rung's own sentence, and the plan untouched.
    let unbound = WorkConfig {
        runner: Some("no-such-runtime-anywhere".to_string()),
        ..WorkConfig::default()
    };
    assert!(refuse(&unbound, "fix-gate-2").contains("is not on PATH"));

    // No such ticket, and one already over.
    assert!(refuse(&config, "fix-gate-9").contains("holds no ticket 'fix-gate-9'"));
    assert!(refuse(&config, "fix-gate-1").contains("already over"));

    // The runtime's own refusal, relayed: the stand-in will not leave `fix`.
    let said = refuse(&config, "ask-1");
    assert!(
        said.contains(
            "refused: Task ask-1 cannot leave state fix. Missing required output artifact: report"
        ),
        "{said}"
    );
    assert!(fs::read_to_string(&plan_path)
        .unwrap()
        .contains("### Task ask-1: four\n**State:** fix"));

    // A machine with no abandonment state: refused by name, with what to add.
    let bare = tmp.path().join("bare");
    fs::create_dir_all(&bare).unwrap();
    fs::write(
        bare.join("states.yaml"),
        "name: bare\nstates:\n  fix:\n    agent: x\n  done:\n    final: true\n",
    )
    .unwrap();
    let bare_plan = bare.join("p.rhei.md");
    fs::write(
        &bare_plan,
        "# Rhei: p\n**States:** bare\n\n## Tasks\n\n### Task a-1: a\n**State:** fix\n\nbody\n",
    )
    .unwrap();
    let said = cancel_ticket(&config, &bare, "p", &bare_plan, "a-1", "", false)
        .expect_err("nowhere to put it")
        .to_string();
    assert!(said.contains("the machine 'bare' in"), "{said}");
    assert!(
        said.contains("declares no final 'cancelled' state"),
        "{said}"
    );
    assert!(said.contains("ephor work states"), "{said}");
}

/// A ticket a live run holds is the run's to finish (§FS-005-dispatch.16):
/// with the root's lock held and the journal naming the ticket where the
/// plan has it, the cancel refuses and names the run; with the lock free
/// the same journal line is a dead run's, and the cancel goes ahead.
#[cfg(unix)]
#[test]
fn a_ticket_a_live_run_holds_is_not_cancelled_from_under_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (config, root, plan_path) = root_with_plan(tmp.path());
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::create_dir_all(root.join("runtime/logs")).unwrap();
    let lock_path = root.join(".rhei/run.lock");
    fs::write(&lock_path, "").unwrap();
    let log = root.join("runtime/logs/task-fix-gate-2-review.log");
    fs::write(&log, "working").unwrap();
    fs::write(
        root.join("runtime/transitions.log"),
        "2026-08-15T10:00:00Z  fix-gate-2  start@review  runtime/logs/task-fix-gate-2-review.log\n",
    )
    .unwrap();

    // The lock held, as a live run holds it.
    let held = fs::File::open(&lock_path).unwrap();
    held.lock().unwrap();
    let said = cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-2",
        "",
        false,
    )
    .expect_err("a live run holds it")
    .to_string();
    assert!(said.contains("held by a live run in 'review'"), "{said}");
    // Another ticket in the same root, not held, is fair.
    cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-3",
        "",
        false,
    )
    .expect("queued, not held");
    held.unlock().unwrap();
    drop(held);

    // The lock free: the run died, the journal line is history, and the
    // reader may take the ticket back.
    cancel_ticket(
        &config,
        &root,
        "forge-demo-17",
        &plan_path,
        "fix-gate-2",
        "the run died",
        false,
    )
    .expect("nobody holds it now");
}

/// The walk is bounded by what can resolve without an item: a work-root
/// template that ignores the workspace is listed once however many
/// places render to it, and one naming a placeholder only an item can
/// fill is skipped rather than guessed (§FS-005-dispatch.15.1).
#[test]
fn a_shared_root_is_listed_once_and_an_item_template_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("widget");
    let mut widget = placement("widget", &root, Some("{project_root}/branches/{branch}"));
    widget.branches = vec![branch("a"), branch("b")];
    fs::create_dir_all(root.join("branches/a")).unwrap();
    fs::create_dir_all(root.join("branches/b")).unwrap();
    plant(&root.join("work"), "shared.rhei.md", "Shared");

    let mut projects = BTreeMap::new();
    projects.insert(
        "widget".to_string(),
        ProjectWorkConfig {
            root: Some("{root}/work".to_string()),
            ..ProjectWorkConfig::default()
        },
    );
    let ledger = Ledger {
        version: 1,
        entries: BTreeMap::new(),
        starts: BTreeMap::new(),
    };
    let groups = enumerate_roots(
        &WorkConfig::default(),
        &projects,
        std::slice::from_ref(&widget),
        &ledger,
    );
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].root, fs::canonicalize(root.join("work")).unwrap());
    assert_eq!(groups[0].plans.len(), 1, "one listing, one plan, no dupes");

    // A placeholder only an item can fill cannot resolve here.
    projects.insert(
        "widget".to_string(),
        ProjectWorkConfig {
            root: Some("{root}/work/{ticket}".to_string()),
            ..ProjectWorkConfig::default()
        },
    );
    let groups = enumerate_roots(
        &WorkConfig::default(),
        &projects,
        std::slice::from_ref(&widget),
        &ledger,
    );
    assert!(groups.is_empty(), "{:?}", roots_of(&groups));
}

/// A branch workspace that is a symlink back to the checkout renders the
/// same work root under two names, and one directory is one operation —
/// the runtime's lock is on the directory, so two rows here would show
/// one run twice (§FS-005-dispatch.15). Groups collapse on the
/// directory, never on the spelling.
#[cfg(unix)]
#[test]
fn an_aliased_workspace_is_one_root_not_two() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("widget");
    let mut widget = placement("widget", &root, Some("{project_root}/branches/{branch}"));
    widget.branches = vec![branch("main")];
    plant(&root.join("panta"), "housekeeping.rhei.md", "Housekeeping");
    fs::create_dir_all(root.join("branches")).unwrap();
    std::os::unix::fs::symlink(&root, root.join("branches/main")).unwrap();

    let ledger = Ledger {
        version: 1,
        entries: BTreeMap::new(),
        starts: BTreeMap::new(),
    };
    let groups = enumerate_roots(
        &WorkConfig::default(),
        &BTreeMap::new(),
        std::slice::from_ref(&widget),
        &ledger,
    );
    assert_eq!(groups.len(), 1, "{:?}", roots_of(&groups));
    assert_eq!(groups[0].plans.len(), 1);
}

fn ticket(id: &str, state: &str) -> TicketStatus {
    TicketStatus {
        running: false,
        queued: false,
        id: id.to_string(),
        recipe: "fix-gate".to_string(),
        title: "fix the red gate".to_string(),
        state: Some(state.to_string()),
        finished: false,
        cancelled: false,
        waiting: false,
        assignee: None,
        pinned: None,
        verdict: None,
        asked: None,
    }
}

fn work_status(tickets: Vec<TicketStatus>) -> WorkStatus {
    WorkStatus {
        quiet: None,
        project: "widget".to_string(),
        root: PathBuf::from("/w/widget/panta"),
        plan_id: "forge-widget-42".to_string(),
        checkout: PathBuf::from("/w/widget"),
        plan: PathBuf::from("/w/widget/panta/forge-widget-42.rhei.md"),
        missing: false,
        tickets,
        workflows: 0,
        changes: Vec::new(),
        advance: None,
    }
}

/// The work stands on a row per open ticket, and the one the runtime
/// parked stands first: it is the one part nobody else will move
/// (§FS-005-dispatch.9, §FS-005-dispatch.23). Each row names the ticket it
/// is, so cancelling on it takes back that one and not the plan's newest
/// (§FS-005-dispatch.16).
#[test]
fn every_open_ticket_gets_a_row_and_the_parked_one_leads() {
    let parked = TicketStatus {
        waiting: true,
        ..ticket("fix-gate-2", "ask")
    };
    let status = work_status(vec![ticket("fix-gate-1", "collect"), parked]);
    let lines = status.lines(60);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0].tone, Tone::Waiting);
    assert!(lines[0].said.contains("waiting on you"), "{lines:?}");
    assert_eq!(lines[0].ticket.as_deref(), Some("fix-gate-2"));
    assert_eq!(lines[1].tone, Tone::Going);
    assert_eq!(lines[1].said, "fix-gate · collect");
    assert_eq!(lines[1].ticket.as_deref(), Some("fix-gate-1"));
}

/// What is over is one row and not many: the whole record is the work
/// screen's, and a tree that grew a row per finished ticket would bury the
/// matters between them (§FS-005-dispatch.18, §FS-005-dispatch.23). It
/// carries no ticket, because there is nothing there to take back.
#[test]
fn a_plan_with_nothing_open_stands_on_one_row_for_what_it_decided() {
    let finished = |id: &str, verdict: &str| TicketStatus {
        finished: true,
        verdict: Some(verdict.to_string()),
        state: Some("done".to_string()),
        ..ticket(id, "done")
    };
    let status = work_status(vec![
        finished("fix-gate-1", "an older one"),
        finished("fix-gate-2", "the gate is green"),
    ]);
    let lines = status.lines(60);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0].tone, Tone::Over);
    assert_eq!(lines[0].said, "fix-gate · the gate is green");
    assert_eq!(lines[0].ticket, None);

    // Taken back is a different kind of over, and the row says which.
    let taken_back = TicketStatus {
        finished: true,
        cancelled: true,
        ..ticket("fix-gate-1", "cancelled")
    };
    let cancelled = work_status(vec![taken_back]).lines(60);
    assert_eq!(cancelled[0].said, "fix-gate · cancelled");
    assert_eq!(cancelled[0].marker, "⊘");
}

/// An item that moved under its work says so on a row of its own: it is a
/// fact about the work and not about the matter (§FS-005-dispatch.5,
/// §FS-005-dispatch.23).
#[test]
fn an_item_that_moved_under_its_work_says_so_on_a_row_of_its_own() {
    let mut status = work_status(vec![ticket("fix-gate-1", "collect")]);
    status.changes = vec!["1 new message".to_string()];
    let lines = status.lines(60);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[1].tone, Tone::Stale);
    assert!(lines[1].said.contains("1 new message"), "{lines:?}");
    assert_eq!(lines[1].ticket, None);
}
// ---- the sweep behind autorun (§FS-005-dispatch.24) ----

/// A work root holding a plan, a machine, and whatever tickets are asked
/// for. `fix` runs, `needs-human` gates, `done` is over — the shipped
/// shape, narrowed to what the sweep has to tell apart.
fn due_root(root: &Path, tickets: &str) -> runtime::watch::RootPlans {
    fs::create_dir_all(root).unwrap();
    fs::write(
        root.join("states.yaml"),
        concat!(
            "name: m\n",
            "states:\n",
            "  collect:\n    agent: x\n",
            "  fix:\n    agent: x\n",
            "  needs-human:\n    gating: true\n",
            "  done:\n    final: true\n",
        ),
    )
    .unwrap();
    fs::write(
        root.join("widget-42.rhei.md"),
        format!("# Rhei: t\n**States:** m\n\n## Tasks\n\n{tickets}"),
    )
    .unwrap();
    runtime::watch::RootPlans {
        root: root.to_path_buf(),
        plans: vec![runtime::watch::PlanRef {
            project: "widget".to_string(),
            plan_id: "widget-42".to_string(),
            path: root.join("widget-42.rhei.md"),
            item: Some("forge:widget/42".to_string()),
            title: "Widen the retry window".to_string(),
        }],
    }
}

fn ticket_at(id: &str, state: &str) -> String {
    format!("### Task {id}: do it\n**State:** {state}\n\nwork\n\n")
}

fn asking(recipes: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    BTreeMap::from([(
        "widget".to_string(),
        recipes.iter().map(|id| id.to_string()).collect(),
    )])
}

/// The same, for the workflow entries that asked to run themselves
/// (§FS-005-dispatch.28).
fn laying(entries: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    asking(entries)
}

/// A runner every machine has, so nothing here is refused for want of one.
fn work_config() -> WorkConfig {
    WorkConfig {
        runner: Some("sh".to_string()),
        ..WorkConfig::default()
    }
}

fn empty_ledger() -> Ledger {
    Ledger {
        version: 1,
        entries: BTreeMap::new(),
        starts: BTreeMap::new(),
    }
}

fn candidate(id: &str, project: &str) -> Due {
    Due {
        project: project.to_string(),
        projects: vec![project.to_string()],
        root: PathBuf::from(format!("/{id}")),
        checkout: PathBuf::from("/"),
        plans: vec![id.to_string()],
        tickets: vec![format!("{id}.fix-1")],
        item: Some(id.to_string()),
        items: vec![id.to_string()],
    }
}

#[test]
fn zero_pauses_autorun_and_omission_is_unlimited() {
    let unlimited = Capacity::new(None, BTreeMap::new(), LiveRuns::default());
    assert!(unlimited.refusal(&["widget".to_string()]).is_none());

    let paused = Capacity::new(Some(0), BTreeMap::new(), LiveRuns::default());
    assert!(paused
        .refusal(&["widget".to_string()])
        .is_some_and(|why| why.contains("global work.max_concurrent 0")));
}

#[test]
fn a_project_ceiling_is_additional_to_the_aggregate_ceiling() {
    let live = LiveRuns {
        global: 1,
        projects: BTreeMap::from([("widget".to_string(), 1)]),
    };
    let capacity = Capacity::new(Some(3), BTreeMap::from([("widget".to_string(), 1)]), live);
    assert!(capacity
        .refusal(&["widget".to_string()])
        .is_some_and(|why| why.contains("projects.widget.work.max_concurrent 1")));
    assert!(capacity.refusal(&["another".to_string()]).is_none());
}

#[test]
fn failed_and_immediately_finished_starts_leave_the_slot_available() {
    let mut capacity = Capacity::new(Some(1), BTreeMap::new(), LiveRuns::default());
    // A failed start never calls `started`; an immediately finished or
    // already-dead start records `false`. Neither consumes the slot.
    let projects = ["widget".to_string()];
    assert!(capacity.refusal(&projects).is_none());
    capacity.started(&projects, false);
    assert!(capacity.refusal(&projects).is_none());
    capacity.started(&projects, true);
    assert!(capacity.refusal(&projects).is_some());
}

#[test]
fn existing_live_roots_consume_global_and_project_capacity() {
    let tmp = tempfile::tempdir().unwrap();
    let mut widget = due_root(
        &tmp.path().join("widget/panta"),
        &ticket_at("fix-gate-1", "collect"),
    );
    widget.plans[0].project = "widget".to_string();
    let mut other = due_root(
        &tmp.path().join("other/panta"),
        &ticket_at("fix-gate-1", "collect"),
    );
    other.plans[0].project = "other".to_string();
    fs::create_dir_all(widget.root.join(".rhei")).unwrap();
    fs::write(widget.root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(widget.root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();

    let live = LiveRuns::read(&work_config(), &[widget, other]);
    assert_eq!(live.global, 1);
    assert_eq!(live.projects.get("widget"), Some(&1));
    assert_eq!(live.projects.get("other"), None);
    drop(holder);
}

#[test]
fn due_roots_follow_the_ranking_and_keep_prior_order_after_it() {
    let due = vec![
        candidate("first", "widget"),
        candidate("second", "widget"),
        candidate("third", "widget"),
    ];
    let ranked = rank_due(due, &["third".to_string()]);
    assert_eq!(
        ranked
            .iter()
            .map(|root| root.item.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["third", "first", "second"]
    );
}

#[test]
fn review_repro_shared_root_ranks_by_the_due_item() {
    let tmp = tempfile::tempdir().unwrap();
    let shared_root = tmp.path().join("a-shared/panta");
    let mut shared = due_root(&shared_root, &ticket_at("review-1", "fix"));
    shared.plans[0].item = Some("unranked".to_string());
    let high_plan = shared_root.join("high.rhei.md");
    fs::write(
        &high_plan,
        format!(
            "# Rhei: high\n**States:** m\n\n## Tasks\n\n{}",
            ticket_at("fix-gate-1", "fix")
        ),
    )
    .unwrap();
    shared.plans.push(runtime::watch::PlanRef {
        project: "widget".to_string(),
        plan_id: "high".to_string(),
        path: high_plan,
        item: Some("highest".to_string()),
        title: "Highest ranked".to_string(),
    });
    let mut other = due_root(
        &tmp.path().join("b-other/panta"),
        &ticket_at("fix-gate-1", "fix"),
    );
    other.plans[0].item = Some("second".to_string());

    let due = due_among(
        &work_config(),
        &[shared, other],
        &asking(&["fix-gate"]),
        &laying(&[]),
        &empty_ledger(),
        Utc::now(),
    );
    let ranked = rank_due(due, &["highest".to_string(), "second".to_string()]);
    assert_eq!(ranked[0].root, shared_root);
    assert_eq!(ranked[0].item.as_deref(), Some("highest"));
    assert_eq!(ranked[0].tickets, vec!["high.fix-gate-1".to_string()]);
}

/// The plain case: an open ticket from a recipe that asked to run itself,
/// on a root nothing is running on, is due — and it says which ticket
/// made it so (§FS-005-dispatch.24).
#[test]
fn an_open_ticket_from_an_autorun_recipe_makes_its_root_due() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    let due = due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&["fix-gate"]),
        &laying(&[]),
        &empty_ledger(),
        Utc::now(),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].root, root);
    assert_eq!(due[0].plans, vec!["widget-42".to_string()]);
    assert_eq!(due[0].tickets, vec!["widget-42.fix-gate-1".to_string()]);
    // The checkout is the directory the work root sits in — where the
    // runtime is run from (§FS-005-dispatch.3).
    assert_eq!(due[0].checkout, tmp.path());
}

/// Silence means the key: a recipe that never asked to run itself is
/// started by the reader, as everything always was
/// (§FS-005-dispatch.24).
#[test]
fn a_recipe_that_did_not_ask_is_never_due() {
    let tmp = tempfile::tempdir().unwrap();
    let group = due_root(&tmp.path().join("panta"), &ticket_at("review-1", "fix"));
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&["fix-gate"]),
        &laying(&[]),
        &empty_ledger(),
        Utc::now(),
    )
    .is_empty());
}

/// What a run would not advance is not what makes a root due: work that
/// is over, work parked on a question for a person, and work somebody
/// has claimed (§FS-005-dispatch.24, §FS-005-dispatch.15).
#[test]
fn finished_parked_and_claimed_tickets_make_nothing_due() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(
        &root,
        &format!(
            "{}{}{}",
            ticket_at("fix-gate-1", "done"),
            ticket_at("fix-gate-2", "needs-human"),
            "### Task fix-gate-3: do it\n**State:** fix\n**Assignee:** luna\n\nwork\n\n",
        ),
    );
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&["fix-gate"]),
        &laying(&[]),
        &empty_ledger(),
        Utc::now(),
    )
    .is_empty());
}

/// A root a run already holds gets nothing: the runtime schedules one run
/// per root, and the live run reaches a ticket written beneath it
/// (§FS-005-dispatch.24).
#[test]
fn a_root_a_run_already_holds_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    fs::create_dir_all(root.join(".rhei")).unwrap();
    fs::write(root.join(".rhei/run.lock"), "").unwrap();
    let holder = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    holder.lock().unwrap();

    assert!(
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &laying(&[]),
            &empty_ledger(),
            Utc::now(),
        )
        .is_empty(),
        "a second run there would only wait for the first"
    );
    // And once that run lets go, the root is due again. Closing the
    // holder is not always the instant the kernel releases the lock —
    // the release rides on the last reference to the open file going
    // away, which can be deferred by a millisecond under load — so this
    // waits for the world to agree rather than assuming it already does.
    // Everything ephor does here reads the lock as it is at the moment it
    // asks (§FS-005-dispatch.15), which is exactly what is being checked.
    drop(holder);
    let freed = std::time::Instant::now();
    while runtime::watch::live(&work_config(), &root) {
        assert!(
            freed.elapsed() < std::time::Duration::from_secs(5),
            "the run let go and the lock never came free"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &laying(&[]),
            &empty_ledger(),
            Utc::now(),
        )
        .len(),
        1
    );
}

/// Finality and gating are the machine's words. With no machine to say
/// them, nothing can be judged runnable, and the sweep starts nothing
/// rather than guessing (§FS-005-dispatch.15).
#[test]
fn a_root_with_no_machine_starts_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    fs::remove_file(root.join("states.yaml")).unwrap();
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&["fix-gate"]),
        &laying(&[]),
        &empty_ledger(),
        Utc::now(),
    )
    .is_empty());
}

/// A root whose start failed rests, and is tried again once it has
/// (§FS-005-dispatch.24) — otherwise a runner that refuses turns every
/// sweep into another spawn.
#[test]
fn a_root_whose_start_failed_rests_before_it_is_tried_again() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    let now = Utc::now();
    let mut ledger = empty_ledger();
    ledger.starts.insert(
        root.to_string_lossy().into_owned(),
        ledger::Start {
            at: now,
            failures: 1,
            says: "the runner refused".to_string(),
        },
    );
    let sweep = |ledger: &Ledger, at| {
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &laying(&[]),
            ledger,
            at,
        )
    };
    assert!(sweep(&ledger, now).is_empty(), "it has just failed");
    assert_eq!(
        sweep(&ledger, now + chrono::Duration::minutes(6)).len(),
        1,
        "and it is tried again once the interval is out"
    );
    // Two failures in a row and it waits longer than one did.
    ledger
        .starts
        .get_mut(&root.to_string_lossy().into_owned())
        .unwrap()
        .failures = 3;
    assert!(
        sweep(&ledger, now + chrono::Duration::minutes(6)).is_empty(),
        "the interval grows with each consecutive failure"
    );
}

/// A ticket a hand appended is due exactly as a dispatched one: the
/// recipe is a fact about the ticket, read off its id where no dispatch
/// recorded one (§FS-005-dispatch.24).
#[test]
fn a_ticket_nobody_dispatched_is_due_by_the_recipe_its_id_names() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-7", "fix"));
    let due = due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&["fix-gate"]),
        &laying(&[]),
        &empty_ledger(),
        Utc::now(),
    );
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].tickets, vec!["widget-42.fix-gate-7".to_string()]);
}

/// An id that is not `<recipe>-<n>` names no recipe, and nothing is
/// guessed from it.
#[test]
fn a_ticket_id_that_names_no_recipe_makes_nothing_due() {
    assert_eq!(recipe_of_ticket("fix-gate-1"), Some("fix-gate"));
    assert_eq!(recipe_of_ticket("fix-gate-1-2"), Some("fix-gate-1"));
    assert_eq!(recipe_of_ticket("housekeeping"), None);
    assert_eq!(recipe_of_ticket("-1"), None);
    assert_eq!(recipe_of_ticket("fix-gate-"), None);
}

/// Work about a branch belongs in that branch's working tree. A root
/// whose checkout has since moved to another branch holds different
/// code, and a start with nobody watching refuses exactly where dispatch
/// does (§FS-005-dispatch.24, §FS-005-dispatch.3).
#[test]
fn a_checkout_standing_on_another_branch_is_not_run_in() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = tmp.path().join("widget");
    let root = checkout.join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    fs::create_dir_all(checkout.join(".git")).unwrap();
    fs::write(checkout.join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();

    let mut ledger = empty_ledger();
    ledger.entries.insert(
        "forge:widget/42".to_string(),
        Entry {
            project: "widget".to_string(),
            title: "t".to_string(),
            url: None,
            root: root.clone(),
            checkout: checkout.clone(),
            branch: Some("you/retry-window".to_string()),
            plan_id: "widget-42".to_string(),
            plan: root.join("widget-42.rhei.md"),
            dispatches: Vec::new(),
        },
    );
    let sweep = |ledger: &Ledger| {
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &laying(&[]),
            ledger,
            Utc::now(),
        )
    };
    assert!(
        sweep(&ledger).is_empty(),
        "the tree is standing on another branch"
    );
    // Back on the branch the work is about, and it runs.
    fs::write(
        checkout.join(".git/HEAD"),
        "ref: refs/heads/you/retry-window\n",
    )
    .unwrap();
    assert_eq!(sweep(&ledger).len(), 1);
}

/// A branch nobody recorded refuses nothing: an entry written before the
/// branch was kept, or work that matched no branch at all, is run where
/// it always was (§FS-005-dispatch.24).
#[test]
fn a_branch_nobody_recorded_refuses_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let checkout = tmp.path().join("widget");
    let root = checkout.join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    fs::create_dir_all(checkout.join(".git")).unwrap();
    fs::write(checkout.join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();
    let mut ledger = empty_ledger();
    ledger.entries.insert(
        "forge:widget/42".to_string(),
        Entry {
            project: "widget".to_string(),
            title: "t".to_string(),
            url: None,
            root: root.clone(),
            checkout: checkout.clone(),
            branch: None,
            plan_id: "widget-42".to_string(),
            plan: root.join("widget-42.rhei.md"),
            dispatches: Vec::new(),
        },
    );
    assert_eq!(
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-gate"]),
            &laying(&[]),
            &ledger,
            Utc::now(),
        )
        .len(),
        1
    );
}

/// The ledger says which recipe ephor wrote a ticket from, and that beats
/// the id's own shape: a ticket dispatched from a recipe that does not
/// autorun is not made due by an id that happens to look like one that
/// does (§FS-005-dispatch.24).
#[test]
fn the_ledgers_recipe_answers_for_a_ticket_ephor_dispatched() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = due_root(&root, &ticket_at("fix-gate-1", "collect"));
    let mut ledger = empty_ledger();
    ledger.entries.insert(
        "forge:widget/42".to_string(),
        Entry {
            project: "widget".to_string(),
            title: "t".to_string(),
            url: None,
            root: root.clone(),
            checkout: tmp.path().to_path_buf(),
            branch: None,
            plan_id: "widget-42".to_string(),
            plan: root.join("widget-42.rhei.md"),
            dispatches: vec![ledger::Dispatch {
                ticket: "fix-gate-1".to_string(),
                // Written from a recipe that asks nobody to run it, under
                // an id another recipe's tickets would carry.
                recipe: "review".to_string(),
                at: Utc::now(),
                plan: None,
                snapshot: Default::default(),
            }],
        },
    );
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&["fix-gate"]),
        &laying(&[]),
        &ledger,
        Utc::now(),
    )
    .is_empty());
}

/// A work root holding a plan a workflow laid down: the directory
/// workspace the runtime rendered — an index that names no task, a
/// machine of its own, and the tasks in files beside it — plus the
/// ledger's record of which entry laid it (§FS-005-dispatch.28).
///
/// The plan's own machine deliberately disagrees with the root's: `open`
/// runs and `fix` is over here, where the root's `m` has neither — so a
/// judgment made against the wrong one cannot pass by accident.
fn laid_root(root: &Path, entry: &str, tasks: &[&str]) -> runtime::watch::RootPlans {
    let group = due_root(root, &ticket_at("fix-gate-1", "done"));
    let plan_id = format!("forge-widget-42-{entry}");
    let dir = root.join(&plan_id);
    fs::create_dir_all(dir.join("tasks")).unwrap();
    fs::write(dir.join("index.rhei.md"), "# Rhei: fix it\n**States:** w\n").unwrap();
    fs::write(
        dir.join("states.yaml"),
        concat!(
            "name: w\n",
            "states:\n",
            "  open:\n    agent: x\n",
            "  done:\n    agent: x\n",
            "  asking:\n    gating: true\n",
            "  fix:\n    final: true\n",
        ),
    )
    .unwrap();
    for (n, task) in tasks.iter().enumerate() {
        fs::write(dir.join("tasks").join(format!("{n:02}-task.md")), task).unwrap();
    }
    runtime::watch::RootPlans {
        root: group.root,
        plans: vec![runtime::watch::PlanRef {
            project: "widget".to_string(),
            plan_id,
            path: dir.join("index.rhei.md"),
            item: Some("forge:widget/42".to_string()),
            title: "Widen the retry window".to_string(),
        }],
    }
}

/// The ledger as it stands after one entry laid one workflow down
/// (§FS-005-dispatch.19).
fn laid_ledger(root: &Path, entry: &str) -> Ledger {
    let mut ledger = empty_ledger();
    ledger.entries.insert(
        "forge:widget/42".to_string(),
        Entry {
            project: "widget".to_string(),
            title: "t".to_string(),
            url: None,
            root: root.to_path_buf(),
            checkout: root.parent().unwrap().to_path_buf(),
            branch: None,
            plan_id: "widget-42".to_string(),
            plan: root.join("widget-42.rhei.md"),
            dispatches: vec![ledger::Dispatch {
                ticket: String::new(),
                recipe: entry.to_string(),
                at: Utc::now(),
                plan: Some(format!("forge-widget-42-{entry}")),
                snapshot: Default::default(),
            }],
        },
    );
    ledger
}

/// A plan a workflow laid down is due exactly as a ticket is, and by the
/// same silence: the entry that laid it is what asked, its tasks live in
/// files beside its index, and only an entry that said `autorun` makes
/// the root due (§FS-005-dispatch.28).
#[test]
fn a_laid_workflows_tasks_make_its_root_due_when_the_entry_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = laid_root(&root, "fix-issue", &[&ticket_at("ticket", "open")]);
    let ledger = laid_ledger(&root, "fix-issue");
    let sweep = |workflows: BTreeMap<String, BTreeSet<String>>| {
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&[]),
            &workflows,
            &ledger,
            Utc::now(),
        )
    };
    let due = sweep(laying(&["fix-issue"]));
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].root, root);
    assert_eq!(due[0].plans, vec!["forge-widget-42-fix-issue".to_string()]);
    assert_eq!(
        due[0].tickets,
        vec!["forge-widget-42-fix-issue.ticket".to_string()]
    );
    assert_eq!(due[0].item.as_deref(), Some("forge:widget/42"));

    // Silence means the key: an entry that said nothing lays a plan
    // nobody but the reader starts.
    assert!(sweep(laying(&[])).is_empty());
    // And the entry that laid it is the one that has to have asked —
    // another entry asking says nothing about this plan.
    assert!(sweep(laying(&["review-change"])).is_empty());
}

/// A task's state means whatever the machine in force for its own store
/// says it means (§FS-006-project-interface.7): the laid plan's own
/// `states.yaml` answers for its tasks, never the work root's
/// (§FS-005-dispatch.28).
#[test]
fn a_laid_workflows_tasks_are_judged_by_the_plans_own_machine() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    // `done` is final in the root's machine and ordinary work in the
    // plan's own, so judging it by the root's would start nothing.
    let group = laid_root(&root, "fix-issue", &[&ticket_at("ticket", "done")]);
    let ledger = laid_ledger(&root, "fix-issue");
    let sweep = || {
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&[]),
            &laying(&["fix-issue"]),
            &ledger,
            Utc::now(),
        )
    };
    assert_eq!(sweep().len(), 1, "the plan's own machine says it is open");

    // And the other way: `fix` runs under the root's machine and is over
    // under the plan's, so nothing is due.
    fs::write(
        root.join("forge-widget-42-fix-issue/tasks/00-task.md"),
        ticket_at("ticket", "fix"),
    )
    .unwrap();
    assert!(sweep().is_empty(), "the plan's own machine says it is over");
}

/// What a run would not advance is not what makes a laid plan's root due
/// either: work that is over, work parked on a question, and work
/// somebody has claimed (§FS-005-dispatch.28, §FS-005-dispatch.24).
#[test]
fn finished_parked_and_claimed_tasks_of_a_laid_workflow_make_nothing_due() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = laid_root(
        &root,
        "fix-issue",
        &[
            &ticket_at("over", "fix"),
            &ticket_at("waiting", "asking"),
            "### Task claimed: do it\n**State:** open\n**Assignee:** luna\n\nwork\n\n",
        ],
    );
    let ledger = laid_ledger(&root, "fix-issue");
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&[]),
        &laying(&["fix-issue"]),
        &ledger,
        Utc::now(),
    )
    .is_empty());
}

/// The tasks are in the files beside the index, and nowhere else: an
/// index alone names no task, so a plan whose tasks cannot be read makes
/// nothing due rather than guessing (§FS-005-dispatch.28).
#[test]
fn a_laid_workflow_with_no_task_files_makes_nothing_due() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = laid_root(&root, "fix-issue", &[]);
    let ledger = laid_ledger(&root, "fix-issue");
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&[]),
        &laying(&["fix-issue"]),
        &ledger,
        Utc::now(),
    )
    .is_empty());
}

/// Finality and gating for a laid plan's tasks are its own machine's
/// words, and with none to be read there nothing is judged runnable
/// (§FS-005-dispatch.28, §FS-005-dispatch.15): the root's machine answers
/// for other work and must not stand in for it.
#[test]
fn a_laid_workflow_whose_machine_cannot_be_read_starts_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let group = laid_root(&root, "fix-issue", &[&ticket_at("ticket", "open")]);
    // A states document that names no machine: the root's own would call
    // `open` runnable, and that answer is not this plan's.
    fs::write(
        root.join("forge-widget-42-fix-issue/states.yaml"),
        "states:\n  open:\n",
    )
    .unwrap();
    assert!(due_among(
        &work_config(),
        std::slice::from_ref(&group),
        &asking(&[]),
        &laying(&["fix-issue"]),
        &laid_ledger(&root, "fix-issue"),
        Utc::now(),
    )
    .is_empty());
}

/// A plan the record says nothing about asked for nothing
/// (§FS-005-dispatch.28): one a reader laid by hand, or one enumeration
/// simply found in the root, is the reader's to start — and that holds
/// however its tasks are named. `fix-issue-1` is the shape ephor's own
/// ticket ids have, but the tasks of a store of its own are the runtime's
/// and the spelling says nothing about who asked.
#[test]
fn a_plan_nothing_in_the_ledger_laid_is_nobodys_to_start() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    let sweep = |group| {
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&["fix-issue"]),
            &laying(&["fix-issue"]),
            &empty_ledger(),
            Utc::now(),
        )
    };
    assert!(sweep(laid_root(
        &root,
        "fix-issue",
        &[&ticket_at("ticket", "open")]
    ))
    .is_empty());

    // And with an id the recipe rule would otherwise read a recipe out of.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    assert_eq!(recipe_of_ticket("fix-issue-1"), Some("fix-issue"));
    assert!(sweep(laid_root(
        &root,
        "fix-issue",
        &[&ticket_at("fix-issue-1", "open")]
    ))
    .is_empty());
}

/// A laid plan that declares no machine of its own runs under the root's,
/// which is what the runtime resolves such a plan against — not under a
/// default nobody chose (§FS-005-dispatch.28,
/// §FS-006-project-interface.7). Both directions: what the root's machine
/// calls over or parked makes nothing due, and what it calls work does.
#[test]
fn a_laid_workflow_that_declares_no_machine_is_judged_by_the_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("panta");
    // `done` is final and `needs-human` gating in the root's machine, and
    // neither is a state the runtime's default declares at all.
    let group = laid_root(
        &root,
        "fix-issue",
        &[
            &ticket_at("over", "done"),
            &ticket_at("parked", "needs-human"),
        ],
    );
    fs::remove_file(root.join("forge-widget-42-fix-issue/states.yaml")).unwrap();
    let ledger = laid_ledger(&root, "fix-issue");
    let sweep = || {
        due_among(
            &work_config(),
            std::slice::from_ref(&group),
            &asking(&[]),
            &laying(&["fix-issue"]),
            &ledger,
            Utc::now(),
        )
    };
    assert!(sweep().is_empty(), "the root's machine says both are over");

    // And the way round that does start a run: `fix` is work under the
    // root's machine, and the plan borrowing it is what makes it due.
    fs::write(
        root.join("forge-widget-42-fix-issue/tasks/01-task.md"),
        ticket_at("parked", "fix"),
    )
    .unwrap();
    assert_eq!(sweep().len(), 1, "the root's machine says it is open");
}

/// A root a workflow laid into flows through the same sweep as one a
/// recipe wrote: the ranking orders the two together, and capacity
/// counts them the same way (§FS-005-dispatch.28, §FS-005-dispatch.26).
#[test]
fn a_workflow_root_ranks_and_is_capped_beside_a_recipe_root() {
    let tmp = tempfile::tempdir().unwrap();
    let workflow_root = tmp.path().join("a-workflow/panta");
    let workflow = laid_root(&workflow_root, "fix-issue", &[&ticket_at("ticket", "open")]);
    let mut recipe = due_root(
        &tmp.path().join("b-recipe/panta"),
        &ticket_at("fix-gate-1", "collect"),
    );
    recipe.plans[0].item = Some("forge:widget/7".to_string());

    let due = due_among(
        &work_config(),
        &[workflow, recipe],
        &asking(&["fix-gate"]),
        &laying(&["fix-issue"]),
        &laid_ledger(&workflow_root, "fix-issue"),
        Utc::now(),
    );
    assert_eq!(due.len(), 2, "both roots are due");
    let ranked = rank_due(due, &["forge:widget/7".to_string()]);
    assert_eq!(ranked[0].item.as_deref(), Some("forge:widget/7"));
    assert_eq!(ranked[1].item.as_deref(), Some("forge:widget/42"));
    // And the ceiling reaches the workflow root exactly as it reaches the
    // other one: one aggregate slot, taken by whichever ranks first.
    let mut capacity = Capacity::new(Some(1), BTreeMap::new(), LiveRuns::default());
    assert!(capacity.refusal(&ranked[0].projects).is_none());
    capacity.started(&ranked[0].projects, true);
    assert!(capacity.refusal(&ranked[1].projects).is_some());
}

/// The back-off's own arithmetic: it doubles, and it stops growing
/// (§FS-005-dispatch.24).
#[test]
fn the_back_off_doubles_and_is_capped() {
    let at = Utc::now();
    let rest = |failures| {
        ledger::Start {
            at,
            failures,
            says: String::new(),
        }
        .ready_at()
            - at
    };
    assert_eq!(rest(1), chrono::Duration::minutes(5));
    assert_eq!(rest(2), chrono::Duration::minutes(10));
    assert_eq!(rest(3), chrono::Duration::minutes(20));
    // However long it has been failing, it is always tried again.
    assert_eq!(rest(99), chrono::Duration::hours(2));
}
/// Open and being worked on right now are different facts, and the row
/// says which (§FS-005-dispatch.23): a ticket a live run holds is marked
/// and toned apart from one merely open, and one on a live root the run
/// has not reached says it will get its turn.
#[test]
fn a_ticket_a_run_has_in_hand_is_marked_apart_from_one_merely_open() {
    let idle = work_status(vec![ticket("fix-gate-1", "fix")]).lines(60);
    assert_eq!(idle[0].tone, Tone::Going);
    assert_eq!(idle[0].marker, "⚙");
    assert_eq!(idle[0].said, "fix-gate · fix");

    let mut held = ticket("fix-gate-1", "fix");
    held.running = true;
    let live = work_status(vec![held]).lines(60);
    assert_eq!(live[0].tone, Tone::Running);
    assert_eq!(live[0].marker, "▶");
    assert_eq!(live[0].said, "fix-gate · fix");

    let mut waiting_its_turn = ticket("fix-gate-2", "fix");
    waiting_its_turn.queued = true;
    let queued = work_status(vec![waiting_its_turn]).lines(60);
    assert_eq!(queued[0].tone, Tone::Going);
    assert_eq!(queued[0].said, "fix-gate · fix · queued");
}

/// A live run that has gone silent wears the badge the board gives it, on
/// the row the reader is already looking at — and only while something is
/// actually running there (§FS-005-dispatch.23, §FS-005-dispatch.15).
#[test]
fn a_quiet_run_says_so_on_the_row_it_is_running() {
    let mut held = ticket("fix-gate-1", "fix");
    held.running = true;
    let mut status = work_status(vec![held]);
    status.quiet = Some(12);
    assert_eq!(status.lines(60)[0].said, "fix-gate · fix · quiet 12m");

    // Nothing running: the badge is not a thing an idle row wears.
    let mut idle = work_status(vec![ticket("fix-gate-1", "fix")]);
    idle.quiet = Some(12);
    assert_eq!(idle.lines(60)[0].said, "fix-gate · fix");
}

/// A parked ticket is still the reader's business first, whatever a run
/// on the root is doing (§FS-005-dispatch.9): it leads, and it is never
/// dressed as running.
#[test]
fn a_parked_ticket_leads_even_while_a_run_is_live_on_its_root() {
    let mut parked = ticket("answer-1", "needs-human");
    parked.waiting = true;
    parked.queued = true;
    let mut held = ticket("fix-gate-1", "fix");
    held.running = true;
    let lines = work_status(vec![held, parked]).lines(60);
    assert_eq!(lines[0].tone, Tone::Waiting);
    assert!(lines[0].said.contains("waiting on you"), "{lines:?}");
    assert_eq!(lines[1].tone, Tone::Running);
}
