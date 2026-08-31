//! E2E-008-operations: every operation is visible in one place, read from the artifacts alone.
//!
//! The scenario is the operations board's whole contract
//! (§FS-005-dispatch.15), proven against fixture work roots shaped like what
//! the runtime leaves behind. Liveness is the lock: a run holds one per
//! execution root, the probe never waits on it, and the operating system
//! releases it when a run dies — so a running row is a held lock, a claim
//! with the lock free is a different row with the runner's own remedy
//! beside it, and a root with neither is not a row at all. Which ticket a
//! live run holds is read from the journal it writes as it works, never
//! from a process table (§FS-005-dispatch.4) — and only believed while the
//! ticket's own state still agrees: the journal survives across runs, and a
//! run that died mid-slot released nothing.
//!
//! Work that stopped for a person keeps its row (§FS-005-dispatch.9): the
//! usual end of a run that parks a ticket is the run exiting — nothing else
//! was schedulable — so the parked ticket must not vanish with the lock; and
//! it keeps it at any depth, because the floor reads subtasks too
//! (§FS-005-dispatch.15). What a dead run was holding mid-slot keeps a row
//! of its own — dropped, never conflated with parked: one is a question
//! about the work, the other a run that wants starting again.
//!
//! The no-runner case is here too, because it is the shape most
//! installations see: with no runtime bound there are no operations — the
//! board says so in the workable rung's own words — while the plans stay
//! readable on disk, the floor that is never removed, and the runner's own
//! listing only ever sharpens what the files said (§FS-005-dispatch.15).
//! Watch-only throughout: nothing in here starts, stops, or touches a run.

use std::fs;
use std::path::{Path, PathBuf};

use ephor::work::recipe::WorkConfig;
use ephor::work::runtime::plan::Plan;
use ephor::work::runtime::watch::{self, Doing, PlanRef, RootPlans};

/// A runner every machine has, so the workable rung holds while the listing
/// itself fails — which is exactly the floor's case: state and assignee come
/// from the plan file.
fn floor_config() -> WorkConfig {
    WorkConfig {
        runner: Some("sh".to_string()),
        ..WorkConfig::default()
    }
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A work root the runtime has been at: a machine with a gating state, one
/// plan with an open ticket, an unclaimed second ticket, and a finished one.
fn work_root(root: &Path, claimed: bool) -> RootPlans {
    write(
        &root.join("states.yaml"),
        concat!(
            "name: m\n",
            "states:\n",
            "  fix:\n    agent: x\n",
            "  needs-human:\n    gating: true\n",
            "  done:\n    final: true\n",
        ),
    );
    let assignee = if claimed { "**Assignee:** luna\n" } else { "" };
    write(
        &root.join("widget-42.rhei.md"),
        &format!(
            "# Rhei: acme/widget#42\n**States:** m\n\n## Tasks\n\n\
             ### Task fix-gate-1: fix the gate\n**State:** fix\n{assignee}\nwork\n\n\
             ### Task answer-1: answer\n**State:** fix\n\nwork\n\n\
             ### Task old-1: shipped\n**State:** done\n\nwork\n"
        ),
    );
    RootPlans {
        root: root.to_path_buf(),
        plans: vec![PlanRef {
            project: "widget".to_string(),
            plan_id: "widget-42".to_string(),
            path: root.join("widget-42.rhei.md"),
            item: Some("forge:widget/42".to_string()),
            title: "Widen the retry window".to_string(),
        }],
    }
}

/// What a run writes as it works: the journal line taking the first ticket
/// up, and the log being appended to.
fn run_artifacts(root: &Path) {
    write(
        &root.join("runtime/transitions.log"),
        "2026-08-14T10:00:00Z  fix-gate-1  start@fix  runtime/logs/task-fix-gate-1-fix.log\n",
    );
    write(
        &root.join("runtime/logs/task-fix-gate-1-fix.log"),
        "working\n",
    );
}

/// Move one ticket to a new state, the way `rhei transition` does: the
/// `**State:**` line and nothing else — no assignee is ever written by a
/// transition, only `rhei next` claims and `rhei complete` releases.
fn transition(root: &Path, ticket: &str, from: &str, to: &str) {
    let plan = root.join("widget-42.rhei.md");
    let text = fs::read_to_string(&plan).unwrap();
    write(
        &plan,
        &text.replacen(
            &format!("Task {ticket}: answer\n**State:** {from}"),
            &format!("Task {ticket}: answer\n**State:** {to}"),
            1,
        ),
    );
}

/// Hold the run lock the way the runtime does, from this process: flock
/// conflicts across descriptors, so the probe's try-lock sees a held lock.
fn hold_lock(root: &Path) -> fs::File {
    write(&root.join(".rhei/run.lock"), "");
    let file = fs::File::open(root.join(".rhei/run.lock")).unwrap();
    file.lock().unwrap();
    file
}

/// A fake runner binary whose `list` answers with the given JSON and which
/// records every root it was asked about — for proving both what the
/// listing sharpens and where it is never asked at all.
fn fake_runner(dir: &Path, listing: &str) -> (WorkConfig, PathBuf) {
    let runner = dir.join("acme-runtime");
    let calls = dir.join("list-calls.log");
    fs::write(
        &runner,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  list) printf '%s' '{listing}'; echo \"$2\" >> {} ;;\n  *) exit 1 ;;\nesac\n",
            calls.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    }
    (
        WorkConfig {
            runner: Some(runner.to_string_lossy().into_owned()),
            ..WorkConfig::default()
        },
        calls,
    )
}

/// A held lock is a running row, with the journaled ticket running and the
/// rest queued; the lock released — which is what the OS does when a run
/// dies, with no file changed — and nothing held mid-slot, the root stops
/// being an operation entirely (§FS-005-dispatch.15).
#[test]
fn a_held_lock_is_a_running_row_and_a_released_one_is_no_row() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, false);
    let holder = hold_lock(root);
    run_artifacts(root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.refusal, None);
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert!(op.live);
    assert_eq!(op.tickets[0].ticket, "fix-gate-1");
    assert_eq!(op.tickets[0].doing, Doing::Running);
    assert_eq!(op.tickets[1].ticket, "answer-1");
    assert_eq!(op.tickets[1].doing, Doing::Queued);
    // Finished work is counted, not listed: it is history, not an operation.
    assert_eq!(op.done, 1);
    // The board can say where a reader would go: the matter behind it.
    assert_eq!(op.item(), Some("forge:widget/42"));

    // The run finishes its slot — the journal gets the release line — and
    // dies: the OS releases the lock, nothing else on disk moves, and
    // silence was never the signal. The root simply stops being a row.
    // "Released" reads as not-live promptly rather than instantaneously: a
    // process forked by a parallel test between the drop and the probe
    // briefly duplicates the descriptor and with it the lock, until its
    // exec closes it — so poll past that window rather than racing it.
    let journal = root.join("runtime/transitions.log");
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str(
        "2026-08-14T10:09:00Z  fix-gate-1  end@fix  runtime/logs/task-fix-gate-1-fix.log  exit=0,duration=9m,outcome=completed\n",
    );
    write(&journal, &text);
    drop(holder);
    let released = (0..200).any(|_| {
        let clear = !watch::live(&floor_config(), root);
        if !clear {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        clear
    });
    assert!(released, "the lock should read released");
    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert!(board.operations.is_empty(), "no lock, no claim, no row");
}

/// The payoff of §FS-005-dispatch.9 at board scale: the runtime parks a
/// ticket for a person, nothing else is schedulable, the run exits and the
/// lock goes free — `rhei transition` wrote no assignee, so nothing but the
/// parked state itself marks this root. The ticket keeps its row, waiting
/// on the reader, and it is listed ahead of anything else its operation is
/// doing.
#[test]
fn work_parked_for_a_person_keeps_its_row_after_the_run_exits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, false);
    transition(root, "answer-1", "fix", "needs-human");
    // The lock file the exited run left behind, held by nobody.
    write(&root.join(".rhei/run.lock"), "");

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert!(!op.live);
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "answer-1");
    assert_eq!(op.tickets[0].doing, Doing::Waiting);

    // The same parked ticket under a still-live run sorts ahead of the
    // running one: it is the one part of the work nobody else will move
    // (§FS-005-dispatch.9).
    let holder = hold_lock(root);
    run_artifacts(root);
    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    assert!(op.live);
    assert_eq!(op.tickets[0].ticket, "answer-1");
    assert_eq!(op.tickets[0].doing, Doing::Waiting);
    assert_eq!(op.tickets[1].ticket, "fix-gate-1");
    assert_eq!(op.tickets[1].doing, Doing::Running);
    drop(holder);
}

/// A run that died mid-slot leaves a trace, not a blank (§FS-005-dispatch.9):
/// the lock is free, the journal's last word on the ticket is an unreleased
/// assignment, and the plan still has the ticket where the journal put it —
/// so the ticket shows **dropped**, a flavour of its own, never worded as
/// the parked question it is not (§FS-005-dispatch.15). And the stale entry
/// is never believed past the world: once the ticket's state moves on, it
/// stops counting — under a later live run it reads queued, not running
/// forever.
#[test]
fn a_crashed_run_leaves_its_held_ticket_dropped_not_erased() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, false);
    write(&root.join(".rhei/run.lock"), "");
    run_artifacts(root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert!(!op.live, "the OS released the dead run's lock");
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "fix-gate-1");
    assert_eq!(op.tickets[0].doing, Doing::Dropped);

    // A later run locks the same root for the other ticket. The dead
    // ticket's state has moved on since — the journal's unreleased line no
    // longer matches it, so it reads as what it is, never as running.
    let plan = root.join("widget-42.rhei.md");
    let text = fs::read_to_string(&plan).unwrap();
    write(
        &plan,
        &text.replacen(
            "Task fix-gate-1: fix the gate\n**State:** fix",
            "Task fix-gate-1: fix the gate\n**State:** done",
            1,
        ),
    );
    let journal = root.join("runtime/transitions.log");
    let mut text = fs::read_to_string(&journal).unwrap();
    text.push_str(
        "2026-08-15T09:00:00Z  answer-1  start@fix  runtime/logs/task-answer-1-fix.log\n",
    );
    write(&journal, &text);
    let holder = hold_lock(root);
    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    assert!(op.live);
    assert_eq!(op.done, 2, "the moved ticket is history, not a held slot");
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "answer-1");
    assert_eq!(op.tickets[0].doing, Doing::Running);
    drop(holder);
}

/// An assignee is a claim, never a liveness signal: with the lock free the
/// ticket is *claimed, not scheduled*, and the remedy beside it is the bound
/// runner's own release command — reported, not acted on
/// (§FS-005-dispatch.15).
#[test]
fn a_claim_with_the_lock_free_is_its_own_row_with_the_remedy() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, true);
    // The lock file a finished run left behind, held by nobody.
    write(&root.join(".rhei/run.lock"), "");

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert!(!op.live);
    // Only the claimed ticket is an operation; open unclaimed work is the
    // work screen's business, and finished work is history.
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(
        op.tickets[0].doing,
        Doing::Claimed {
            assignee: "luna".to_string(),
            free: "sh release widget-42.fix-gate-1".to_string(),
        }
    );
}

/// The rows are found by looking, never by remembering
/// (§FS-005-dispatch.15): a plan ephor never dispatched — hand-written, or a
/// project's own planning tickets — is enumerated off the work root itself,
/// and a run somebody started in another terminal on that root is a running
/// row like any other. Such an operation has no matter behind it by
/// construction, so where `Enter` would go to the matter it opens the plan
/// instead — the row leads to the plan, titled in the plan's own words. And
/// enumeration is a reading: with no runner bound the plans are still found,
/// it is only operations that cannot exist then.
#[test]
fn a_plan_ephor_never_dispatched_is_watched_all_the_same() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    write(
        &root.join("states.yaml"),
        concat!(
            "name: m\n",
            "states:\n",
            "  fix:\n    agent: x\n",
            "  needs-human:\n    gating: true\n",
            "  done:\n    final: true\n",
        ),
    );
    // Hand-written: no ledger entry anywhere names this plan.
    write(
        &root.join("audit.rhei.md"),
        "# Rhei: Audit the retry paths\n**States:** m\n\n## Tasks\n\n\
         ### Task sweep-1: sweep the callers\n**State:** fix\n\nwork\n",
    );

    // Enumeration knows what a plan looks like; the caller gets ids and
    // paths and wraps them item-less — there is no matter to attach.
    let found = ephor::work::runtime::plan::plans_in(root);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].plan_id, "audit");
    let group = RootPlans {
        root: root.to_path_buf(),
        plans: found
            .into_iter()
            .map(|plan| PlanRef {
                project: "widget".to_string(),
                plan_id: plan.plan_id,
                path: plan.path,
                item: None,
                title: String::new(),
            })
            .collect(),
    };

    // A run started in another terminal: ephor never dispatched into this
    // root, and the lock is the whole signal.
    let holder = hold_lock(root);
    write(
        &root.join("runtime/transitions.log"),
        "2026-08-14T10:00:00Z  sweep-1  start@fix  runtime/logs/task-sweep-1-fix.log\n",
    );
    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert!(op.live);
    assert_eq!(op.tickets[0].ticket, "sweep-1");
    assert_eq!(op.tickets[0].doing, Doing::Running);
    // No matter by construction — Enter's fallback is the plan itself, and
    // the row speaks in the plan's own words.
    assert_eq!(op.item(), None);
    assert_eq!(op.plan(), Some(root.join("audit.rhei.md").as_path()));
    assert_eq!(op.tickets[0].title, "Audit the retry paths");
    drop(holder);

    // The run gone and nothing waiting, the root stops being a row — being
    // enumerated makes a plan watched, not listed forever.
    let released = (0..200).any(|_| {
        let clear = !watch::live(&floor_config(), root);
        if !clear {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        clear
    });
    assert!(released, "the lock should read released");
    write(&root.join("runtime/transitions.log"), "");
    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert!(board.operations.is_empty());

    // Parked by that run before it exited: the hand-written plan keeps its
    // row exactly as a dispatched one would (§FS-005-dispatch.9).
    let text = fs::read_to_string(root.join("audit.rhei.md")).unwrap();
    write(
        &root.join("audit.rhei.md"),
        &text.replacen("**State:** fix", "**State:** needs-human", 1),
    );
    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.operations.len(), 1);
    assert_eq!(board.operations[0].tickets[0].doing, Doing::Waiting);

    // With no runner bound enumeration still finds the plan — reading is
    // the floor and never requires the binary — while the board rightly
    // holds no operations (§FS-005-dispatch.15).
    assert_eq!(ephor::work::runtime::plan::plans_in(root).len(), 1);
    let absent = WorkConfig {
        runner: Some("acme-runtime".to_string()),
        ..WorkConfig::default()
    };
    let board = watch::board(&absent, std::slice::from_ref(&group));
    assert!(board.operations.is_empty());
    assert!(board.refusal.is_some());
}

/// With no runtime bound the board is empty and says why in the workable
/// rung's own words — the shape most installations see, and correct rather
/// than broken — while the plan files stay readable exactly as before: the
/// floor is never removed (§FS-005-dispatch.15).
#[test]
fn no_runtime_is_no_rows_and_the_plans_still_read() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, true);
    let holder = hold_lock(root);
    run_artifacts(root);

    let absent = WorkConfig {
        runner: Some("acme-runtime".to_string()),
        ..WorkConfig::default()
    };
    let board = watch::board(&absent, std::slice::from_ref(&group));
    assert!(board.operations.is_empty());
    assert_eq!(
        board.refusal.as_deref(),
        Some("acme-runtime is not on PATH; ephor writes the tickets but the runtime runs them.")
    );

    // The markdown is the floor: with the runner absent, state, claim, and
    // ticket order still read straight off the plan file.
    let plan = Plan::read(&group.plans[0].path).unwrap().unwrap();
    let tickets = plan.tickets();
    assert_eq!(tickets.len(), 3);
    assert_eq!(tickets[0].state.as_deref(), Some("fix"));
    assert_eq!(tickets[0].assignee.as_deref(), Some("luna"));
    assert_eq!(tickets[2].state.as_deref(), Some("done"));
    drop(holder);
}

/// Where the binary itself is there, its own listing sharpens state and
/// assignee for the tickets it names — and only sharpens: the plan read
/// stays underneath it, answering for everything the listing does not
/// (§FS-005-dispatch.15). And the listing is a forked process, so it is
/// asked only about a root that holds an operation: an idle root beside the
/// busy one costs no subprocess at all (§FS-005-dispatch.15.1).
#[test]
fn the_runners_own_listing_sharpens_rows_and_skips_idle_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let claimed_root = tmp.path().join("work");
    let idle_root = tmp.path().join("idle");
    // The floor claims fix-gate-1 for luna; the runner's fresher answer says
    // nadia holds it now. The idle root has no lock and no claim.
    let claimed = work_root(&claimed_root, true);
    let idle = work_root(&idle_root, false);
    let (bound, calls) = fake_runner(
        tmp.path(),
        r#"[{"id":"widget-42.fix-gate-1","state":"fix","assignee":"nadia"}]"#,
    );

    let board = watch::board(&bound, &[claimed, idle]);
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "fix-gate-1");
    match &op.tickets[0].doing {
        Doing::Claimed { assignee, free } => {
            // The listing's word beat the file's — sharpened, not replaced:
            // the second ticket still read straight off the plan.
            assert_eq!(assignee, "nadia");
            assert!(free.ends_with(" release widget-42.fix-gate-1"), "{free}");
        }
        other => panic!("the listing's claim should show: {other:?}"),
    }
    let asked = fs::read_to_string(&calls).unwrap_or_default();
    assert!(
        asked.contains(&claimed_root.to_string_lossy().into_owned()),
        "{asked}"
    );
    assert!(
        !asked.contains(&idle_root.to_string_lossy().into_owned()),
        "an idle root is stats only, never a fork: {asked}"
    );
}

/// A subtask is a ticket at whatever depth the runtime nests it
/// (§FS-005-dispatch.15): the floor reads `####` and deeper with the dotted
/// ids the runtime's grammar gives them, so a subtask parked for a person
/// keeps its row on a root where no run is live — the case the runner's
/// listing cannot cover, because the listing is only asked about a root
/// that already holds an operation (§FS-005-dispatch.9).
#[test]
fn a_parked_subtask_keeps_its_row_on_an_idle_root() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, false);
    // The run split a ticket, parked the question it could not answer, and
    // exited: nothing else was schedulable, so the lock went free.
    let plan = root.join("widget-42.rhei.md");
    let text = fs::read_to_string(&plan).unwrap();
    write(
        &plan,
        &text.replacen(
            "work\n\n### Task answer-1",
            "work\n\n#### Task fix-gate-1.1: the split-off question\n**State:** needs-human\n\nchild work\n\n### Task answer-1",
            1,
        ),
    );
    write(&root.join(".rhei/run.lock"), "");

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    assert_eq!(board.operations.len(), 1);
    let op = &board.operations[0];
    assert!(!op.live);
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "fix-gate-1.1");
    assert_eq!(op.tickets[0].doing, Doing::Waiting);
}

/// A plan rendered as a directory keeps its tasks in `tasks/*.md`, and they
/// are as much that plan's tasks as one written into its index
/// (§FS-005-dispatch.28) — so the floor reads them, and a run holding one
/// shows as running like any other, the journal naming which one it holds
/// (§FS-005-dispatch.15).
#[test]
fn a_directory_workspaces_tasks_are_on_the_floor_and_a_run_holding_one_shows() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("work");
    write(
        &root.join("states.yaml"),
        concat!(
            "name: m\n",
            "states:\n",
            "  fix:\n    agent: x\n",
            "  done:\n    final: true\n",
        ),
    );
    // The workspace shape: an index that names no task, and the task in a
    // file beside it, which is where this plan's tasks live.
    write(
        &root.join("widget-42/index.rhei.md"),
        "# Rhei: acme/widget#42\n**States:** m\n",
    );
    write(
        &root.join("widget-42/tasks/001-fix.md"),
        "### Task fix-gate-1: fix the gate\n**State:** fix\n\nwork\n",
    );
    let group = RootPlans {
        root: root.clone(),
        plans: vec![PlanRef {
            project: "widget".to_string(),
            plan_id: "widget-42".to_string(),
            path: root.join("widget-42/index.rhei.md"),
            item: Some("forge:widget/42".to_string()),
            title: "Widen the retry window".to_string(),
        }],
    };
    let holder = hold_lock(&root);
    write(
        &root.join("runtime/transitions.log"),
        "2026-08-14T10:00:00Z  widget-42.fix-gate-1  start@fix  runtime/logs/task-widget-42.fix-gate-1-fix.log\n",
    );
    let (bound, _) = fake_runner(
        tmp.path(),
        r#"[{"id":"widget-42.fix-gate-1","state":"fix"}]"#,
    );

    // The floor reads the plan whole: the index names no task, and the task
    // file beside it names the one there is.
    let plan = Plan::read(&group.plans[0].path).unwrap().unwrap();
    assert_eq!(
        plan.tickets()
            .iter()
            .map(|ticket| ticket.id.as_str())
            .collect::<Vec<_>>(),
        ["fix-gate-1"]
    );

    let board = watch::board(&bound, std::slice::from_ref(&group));
    let op = &board.operations[0];
    let held = op
        .tickets
        .iter()
        .find(|ticket| ticket.ticket == "fix-gate-1")
        .expect("the held ticket has a row");
    assert_eq!(held.doing, Doing::Running);
    assert_eq!(held.plan_id, "widget-42");
    drop(holder);
}

/// A plan that is a **store of its own** — the shape a laid workflow lands
/// as — is judged by the machine beside it and never by the root's
/// (§FS-005-dispatch.28, §FS-006-project-interface.7): a task's state means
/// whatever the machine in force for its own store says it means, and the
/// board is one of the surfaces that has to mean it (§AR-009-surfaces.1).
///
/// So the root here declares no machine at all and nothing is guessed at:
/// the plan's own says which of its states is a question for a person and
/// which is over, both words the root could never have supplied — and the
/// row does not claim nothing was judged, because everything on this floor
/// was.
#[test]
fn a_plan_that_is_a_store_of_its_own_is_judged_by_the_machine_beside_it() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("work");
    // No `states.yaml` in the root: whatever judges this floor comes from
    // beside the plan.
    write(
        &root.join("widget-42/states.yaml"),
        concat!(
            "name: supervised-fix\n",
            "states:\n",
            "  implementing:\n    agent: x\n",
            "  waiting:\n    gating: true\n",
            "  shipped:\n    final: true\n",
        ),
    );
    write(
        &root.join("widget-42/index.rhei.md"),
        "# Rhei: acme/widget#42\n**States:** supervised-fix\n",
    );
    write(
        &root.join("widget-42/tasks/001-fix.md"),
        "### Task fix: fix the ticket\n**State:** waiting\n\nwork\n",
    );
    write(
        &root.join("widget-42/tasks/002-ship.md"),
        "### Task ship: ship it\n**State:** shipped\n\nwork\n",
    );
    let group = RootPlans {
        root: root.clone(),
        plans: vec![PlanRef {
            project: "widget".to_string(),
            plan_id: "widget-42".to_string(),
            path: root.join("widget-42/index.rhei.md"),
            item: Some("forge:widget/42".to_string()),
            title: "Widen the retry window".to_string(),
        }],
    };
    let holder = hold_lock(&root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    assert!(op.live);
    // Parked on a question, not queued: `queued` would promise a turn that
    // never comes, and it is the answer the root's absent machine would have
    // produced under a live run (§FS-005-dispatch.15).
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "fix");
    assert_eq!(op.tickets[0].doing, Doing::Waiting);
    // Finality is that machine's word too.
    assert_eq!(op.done, 1);
    // And nothing leaned on the root's machine, so the row must not say
    // nothing was judged for want of one.
    assert_eq!(op.machine_unread, None);
    drop(holder);
}

/// A store of its own that declares **no** machine beside it is judged by the
/// root's, because that is the machine the runtime resolves such a plan
/// against — a plan naming one names the project's (§FS-005-dispatch.28,
/// §FS-006-project-interface.7). Under a default nobody chose, `done` is not
/// final and `needs-human` is not gating, so the finished task would go
/// uncounted and the parked one would be called queued — the one word
/// §FS-005-dispatch.15 forbids for work that waits on a person.
#[test]
fn a_store_of_its_own_with_no_machine_beside_it_is_judged_by_the_roots() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("work");
    // The root's machine, and no `states.yaml` under `widget-42/`.
    write(
        &root.join("states.yaml"),
        concat!(
            "name: m\n",
            "states:\n",
            "  fix:\n    agent: x\n",
            "  needs-human:\n    gating: true\n",
            "  done:\n    final: true\n",
        ),
    );
    write(
        &root.join("widget-42/index.rhei.md"),
        "# Rhei: acme/widget#42\n**States:** m\n",
    );
    write(
        &root.join("widget-42/tasks/001-fix.md"),
        "### Task fix: fix the ticket\n**State:** needs-human\n\nwork\n",
    );
    write(
        &root.join("widget-42/tasks/002-old.md"),
        "### Task old: shipped\n**State:** done\n\nwork\n",
    );
    let group = RootPlans {
        root: root.clone(),
        plans: vec![PlanRef {
            project: "widget".to_string(),
            plan_id: "widget-42".to_string(),
            path: root.join("widget-42/index.rhei.md"),
            item: Some("forge:widget/42".to_string()),
            title: "Widen the retry window".to_string(),
        }],
    };
    let holder = hold_lock(&root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    assert!(op.live);
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].ticket, "fix");
    assert_eq!(op.tickets[0].doing, Doing::Waiting);
    assert_eq!(op.done, 1);
    // The root's machine was read, so nothing was withheld anywhere.
    assert_eq!(op.machine_unread, None);
    drop(holder);
}

/// Judgment is the plan's own question, so the note about judgment withheld
/// is too (§FS-005-dispatch.15, §FS-005-dispatch.28): on a floor where one
/// plan leans on a root that has no machine and another carries its own, the
/// row names the plan it happened to — and does not deny the count the other
/// plan really earned.
#[test]
fn the_unread_machine_note_names_the_plans_it_happened_to() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("work");
    // No machine in the root: the flat plan has nothing to be judged by.
    write(
        &root.join("widget-42.rhei.md"),
        "# Rhei: acme/widget#42\n**States:** m\n\n## Tasks\n\n\
         ### Task fix-gate-1: fix the gate\n**State:** fix\n\nwork\n",
    );
    write(
        &root.join("widget-99/states.yaml"),
        concat!(
            "name: supervised-fix\n",
            "states:\n",
            "  waiting:\n    gating: true\n",
            "  shipped:\n    final: true\n",
        ),
    );
    write(
        &root.join("widget-99/index.rhei.md"),
        "# Rhei: acme/widget#99\n**States:** supervised-fix\n",
    );
    write(
        &root.join("widget-99/tasks/001-fix.md"),
        "### Task fix: fix the ticket\n**State:** waiting\n\nwork\n",
    );
    write(
        &root.join("widget-99/tasks/002-ship.md"),
        "### Task ship: ship it\n**State:** shipped\n\nwork\n",
    );
    let plan = |id: &str, path: &str| PlanRef {
        project: "widget".to_string(),
        plan_id: id.to_string(),
        path: root.join(path),
        item: None,
        title: String::new(),
    };
    let group = RootPlans {
        root: root.clone(),
        plans: vec![
            plan("widget-42", "widget-42.rhei.md"),
            plan("widget-99", "widget-99/index.rhei.md"),
        ],
    };
    let holder = hold_lock(&root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    // The plan with a machine of its own was judged whole: parked, and one
    // finished task counted.
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(op.tickets[0].plan_id, "widget-99");
    assert_eq!(op.tickets[0].doing, Doing::Waiting);
    assert_eq!(op.done, 1);
    assert_eq!(
        op.machine_unread.as_deref(),
        Some("no states.yaml — nothing judged queued or finished in widget-42")
    );
    drop(holder);
}

/// A store whose own machine will not read says so on the row it withheld
/// (§FS-005-dispatch.15): its rows are dropped because finality and gating
/// are that machine's words, and a floor that dropped them silently would
/// show a live root with nothing on it and no reason why.
#[test]
fn a_store_whose_own_machine_will_not_read_says_so_on_the_row() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("work");
    write(
        &root.join("states.yaml"),
        "name: m\nstates:\n  fix:\n    agent: x\n  done:\n    final: true\n",
    );
    // A states document beside the plan that names no machine: it is there,
    // and it will not read.
    write(&root.join("widget-7/states.yaml"), "states:\n  fix:\n");
    write(
        &root.join("widget-7/index.rhei.md"),
        "# Rhei: acme/widget#7\n**States:** w\n",
    );
    write(
        &root.join("widget-7/tasks/001-fix.md"),
        "### Task fix: fix the ticket\n**State:** fix\n\nwork\n",
    );
    let group = RootPlans {
        root: root.clone(),
        plans: vec![PlanRef {
            project: "widget".to_string(),
            plan_id: "widget-7".to_string(),
            path: root.join("widget-7/index.rhei.md"),
            item: None,
            title: String::new(),
        }],
    };
    let holder = hold_lock(&root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    assert!(op.live);
    assert!(op.tickets.is_empty());
    assert_eq!(op.done, 0);
    assert_eq!(
        op.machine_unread.as_deref(),
        Some("states.yaml unreadable — nothing judged queued or finished in widget-7")
    );
    // And the row says it where a reader would see it.
    assert!(op.badges().iter().any(|badge| badge.contains("widget-7")));
    drop(holder);
}

/// A held task the floor cannot see still shows through the runner's own
/// listing (§FS-005-dispatch.15): the listing sharpens the floor, and a run
/// working a ticket the plan read yields nothing for is exactly what
/// sharpening is for. Here the plan writes a heading whose dotted id does
/// not match its depth — content, by the runtime's own grammar — while the
/// runtime lists it and the journal says it is the one being held.
#[test]
fn a_held_ticket_the_floor_cannot_see_shows_through_the_listing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("work");
    let group = work_root(&root, false);
    let plan = root.join("widget-42.rhei.md");
    let text = fs::read_to_string(&plan).unwrap();
    write(
        &plan,
        &text.replacen(
            "work\n\n### Task answer-1",
            "work\n\n#### Task scratch-1: a heading the grammar reads as content\n             **State:** fix\n\nwork\n\n### Task answer-1",
            1,
        ),
    );
    // The floor really cannot see it: `####` is depth two, and `scratch-1`
    // is one segment.
    let read = Plan::read(&plan).unwrap().unwrap();
    assert!(
        !read.tickets().iter().any(|ticket| ticket.id == "scratch-1"),
        "the plan grammar yields it after all: {:?}",
        read.tickets()
            .iter()
            .map(|t| t.id.clone())
            .collect::<Vec<_>>()
    );

    let holder = hold_lock(&root);
    write(
        &root.join("runtime/transitions.log"),
        "2026-08-14T10:00:00Z  widget-42.scratch-1  start@fix  runtime/logs/task-scratch-1-fix.log\n",
    );
    write(&root.join("runtime/logs/task-scratch-1-fix.log"), "at it\n");
    let (bound, _) = fake_runner(
        tmp.path(),
        r#"[{"id":"widget-42.scratch-1","state":"fix"}]"#,
    );

    let board = watch::board(&bound, std::slice::from_ref(&group));
    let op = &board.operations[0];
    let held = op
        .tickets
        .iter()
        .find(|ticket| ticket.ticket == "scratch-1")
        .unwrap_or_else(|| panic!("the held ticket has a row: {:?}", op.tickets));
    assert_eq!(held.doing, Doing::Running);
    assert_eq!(held.plan_id, "widget-42");
    // Sharpening, not replacing: the floor's own tickets keep their rows.
    assert!(op
        .tickets
        .iter()
        .any(|ticket| ticket.ticket == "fix-gate-1"));
    drop(holder);
}

/// A root whose own state machine cannot be read carries the fact on its
/// operation (§FS-005-dispatch.15): running still shows — the lock and the
/// journal prove it — but nothing is counted finished, and the row says the
/// machine could not be read instead of leaving the zero to be misread.
#[test]
fn a_root_with_no_machine_carries_the_fact_on_its_row() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let group = work_root(root, false);
    fs::remove_file(root.join("states.yaml")).unwrap();
    let holder = hold_lock(root);
    run_artifacts(root);

    let board = watch::board(&floor_config(), std::slice::from_ref(&group));
    let op = &board.operations[0];
    assert!(op.live);
    assert_eq!(op.tickets[0].ticket, "fix-gate-1");
    assert_eq!(op.tickets[0].doing, Doing::Running);
    // The "done" ticket is neither counted nor queued: finality is the
    // machine's word, and the machine is not there to say it.
    assert_eq!(op.done, 0);
    assert_eq!(op.tickets.len(), 1);
    assert_eq!(
        op.machine_unread.as_deref(),
        Some("no states.yaml — nothing judged queued or finished in widget-42")
    );
    drop(holder);
}
