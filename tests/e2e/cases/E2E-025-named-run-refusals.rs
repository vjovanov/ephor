//! E2E-025-named-run-refusals: one named run has one answer on every surface.
//!
//! A reader can start a matter's work from `ephor work run --item …` or from
//! the work screen's `R` key, and those are one ability rather than two guard
//! sequences (§FS-005-dispatch.30). Root validity is settled before live-run
//! safety, `--force` lifts only the latter, and a full budget is warned about
//! only for groups that passed root validation into ordinary start and safety
//! handling (§FS-015-spend-ceiling.6).

#[path = "../support.rs"]
mod support;

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::*;

const ITEM: &str = "acme:app/1";
const OTHER_ITEM: &str = "acme:app/2";
const ROOT_REFUSAL: &str = "no state machine that will read";
const LIVE_REFUSAL: &str = "a run is live in this checkout";
const SPEND_WARNING: &str = "max_spend 0 USD per 24h admits no new autorun starts";

const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '%s' '{"pull_requests":true,"conversation":true,"gate":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/1", "repo": "app", "number": "1",
        "title": "Reproduce refusal ordering", "url": "https://acme.example/pr/1",
        "updated_at": "2026-09-12T10:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [],
        "gate": { "repos": [{ "repo": "app", "passed": 0, "failed": 1, "running": 0 }] } }
    ]'
    ;;
  *)
    printf '%s' '[]'
    ;;
esac
"#;

/// The marker is the assertion: a refusal may ask the runner what it offers,
/// but it must never cross the `run` seam (§FS-005-dispatch.30). A forced run
/// does cross it and reports itself finished so the next matrix row is clean.
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  templates)
    printf '%s' '[]'
    ;;
  run)
    if [ "${2:-}" = "--help" ]; then
      printf '%s\n' '      --headless  Detach the run'
    else
      : > "$HOME/runtime.started"
      printf '%s\n' '{"id":"fixture-run","status":"finished","exit_code":0}'
    fi
    ;;
  *)
    printf '%s\n' 'runtime fixture received an unexpected request' >&2
    exit 9
    ;;
esac
"#;

fn configured(max_spend: bool) -> Value {
    let mut work = json!({
        "runner": "acme-runtime",
        "recipes": [{
            "id": "fix-gate",
            "icon": "x",
            "description": "fix the gate",
            "state": "fix",
            "needs_checkout": false,
            "when": { "kinds": ["pr"] },
            "brief": "Fix {title}."
        }]
    });
    if max_spend {
        work["max_spend"] = json!({ "amount": 0, "currency": "USD", "per": "24h" });
    }
    json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acme", "user": "you", "repos": ["app"] }
        ] } },
        "work": work
    })
}

fn ticketed(max_spend: bool) -> World {
    let world = World::new();
    world.stub("ephor-forge-acme", ACME_FORGE);
    world.stub("acme-runtime", ACME_RUNTIME);
    // Fetch before the budget is written so fixture construction cannot be
    // mistaken for the warning behavior this scenario pins.
    world.configure(configured(false));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world.configure(configured(max_spend));
    world
        .ephor()
        .args(["work", "dispatch", "--item", ITEM, "--recipe", "fix-gate"])
        .assert()
        .success();
    world
}

fn root(world: &World) -> PathBuf {
    world.forest().join("panta")
}

fn marker(world: &World) -> PathBuf {
    world.path().join("runtime.started")
}

fn unreadable(world: &World) {
    fs::remove_file(root(world).join("states.yaml")).expect("remove the root's machine");
}

fn hold(root: &Path) -> fs::File {
    fs::create_dir_all(root.join(".rhei")).expect("the runtime directory");
    fs::write(root.join(".rhei/run.lock"), "").expect("the run lock");
    let holder = fs::File::open(root.join(".rhei/run.lock")).expect("open the run lock");
    holder.lock().expect("hold the run lock");
    holder
}

fn run(world: &World, args: &[&str]) -> Output {
    world.ephor().args(args).output().expect("ephor runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn check_refusal(
    failures: &mut Vec<String>,
    label: &str,
    output: &Output,
    wanted: &str,
    unwanted: &str,
) {
    let said = stderr(output);
    if output.status.code() != Some(1) {
        failures.push(format!(
            "{label}: established refusal exit changed from 1: {:?}",
            output.status.code()
        ));
    }
    if !said.contains(wanted) {
        failures.push(format!("{label}: missing {wanted:?} in stderr:\n{said}"));
    }
    if said.contains(unwanted) {
        failures.push(format!("{label}: selected {unwanted:?} instead:\n{said}"));
    }
}

fn check_json_reading(
    failures: &mut Vec<String>,
    label: &str,
    output: &Output,
    refused: usize,
) -> Option<Value> {
    let reading: Value = match serde_json::from_slice(&output.stdout) {
        Ok(reading) => reading,
        Err(error) => {
            failures.push(format!(
                "{label}: stdout is not the established work-run JSON shape: {error}\n{}",
                String::from_utf8_lossy(&output.stdout)
            ));
            return None;
        }
    };
    let problems = ephor::api::schema::holds("work-run", &reading);
    if !problems.is_empty() {
        failures.push(format!(
            "{label}: work-run JSON no longer matches its schema: {problems:?}\n{reading}"
        ));
    }
    if reading["refused"] != json!(refused) || reading["failed"] != json!(0) {
        failures.push(format!(
            "{label}: expected refused={refused} and failed=0 without changing the reading: {reading}"
        ));
    }
    Some(reading)
}

/// Root refusal, live safety, `--force`, and warning eligibility are one
/// ordered matrix (§FS-005-dispatch.30, §FS-015-spend-ceiling.6). The checks
/// are accumulated so a pre-fix run records every wrong cell, not merely the
/// first one encountered.
#[test]
fn issue_90_named_run_decision_orders_root_live_force_and_warning_eligibility() {
    let mut failures = Vec::new();

    let broken = ticketed(true);
    unreadable(&broken);
    let _broken_holder = hold(&root(&broken));
    for (label, force) in [
        ("root plus live lock", false),
        ("root plus live lock under --force", true),
    ] {
        let mut args = vec!["work", "run", "--item", ITEM, "--json"];
        if force {
            args.push("--force");
        }
        let output = run(&broken, &args);
        check_refusal(&mut failures, label, &output, ROOT_REFUSAL, LIVE_REFUSAL);
        check_json_reading(&mut failures, label, &output, 1);
        if stderr(&output).contains(SPEND_WARNING) {
            failures.push(format!(
                "{label}: an all-root-refused selection emitted a spend warning:\n{}",
                stderr(&output)
            ));
        }
    }
    if marker(&broken).exists() {
        failures.push("a root refusal launched the runtime, including under --force".to_string());
    }

    let valid = ticketed(true);
    let valid_holder = hold(&root(&valid));
    let held = run(&valid, &["work", "run", "--item", ITEM, "--json"]);
    check_refusal(
        &mut failures,
        "valid root plus live lock",
        &held,
        LIVE_REFUSAL,
        ROOT_REFUSAL,
    );
    check_json_reading(&mut failures, "valid root plus live lock", &held, 1);
    if !stderr(&held).contains(SPEND_WARNING) {
        failures.push(format!(
            "valid root plus live lock lost warning eligibility:\n{}",
            stderr(&held)
        ));
    }
    if marker(&valid).exists() {
        failures.push("an unforced live refusal launched the runtime".to_string());
    }
    let forced = run(
        &valid,
        &["work", "run", "--item", ITEM, "--json", "--force"],
    );
    if !forced.status.success() {
        failures.push(format!(
            "--force did not lift the live-run refusal:\n{}",
            stderr(&forced)
        ));
    }
    if !marker(&valid).exists() {
        failures.push("--force lifted the live refusal but did not launch the runtime".to_string());
    }
    if let Some(reading) = check_json_reading(&mut failures, "forced valid root", &forced, 0) {
        if reading["runs"][0]["outcome"] != "done"
            || reading["runs"][0]["plans"] != json!(["acme-app-1"])
        {
            failures.push(format!(
                "--force changed the successful launch or its plan selection: {reading}"
            ));
        }
    }
    drop(valid_holder);

    let mixed = ticketed(false);
    add_root_refused_project(&mixed);
    let _ordinary_holder = hold(&root(&mixed));
    set_project_budget(&mixed, "broken", true);
    let filtered = run(&mixed, &["--act", "work", "run", "--json"]);
    check_json_reading(&mut failures, "mixed filtered budget", &filtered, 2);
    if stderr(&filtered).contains("projects.broken.work.max_spend") {
        failures.push(format!(
            "a root-refused project entered the mixed-selection budget lookup:\n{}",
            stderr(&filtered)
        ));
    }
    set_project_budget(&mixed, PROJECT, true);
    set_project_budget(&mixed, "broken", false);
    let eligible = run(&mixed, &["--act", "work", "run", "--json"]);
    check_json_reading(&mut failures, "mixed eligible budget", &eligible, 2);
    if !stderr(&eligible).contains("projects.demo.work.max_spend") {
        failures.push(format!(
            "the ordinary live-refused group in a mixed selection was not warning-eligible:\n{}",
            stderr(&eligible)
        ));
    }
    if marker(&mixed).exists() {
        failures.push("the mixed selection launched through either refusal".to_string());
    }

    assert!(
        failures.is_empty(),
        "the named-run decision matrix drifted:\n- {}",
        failures.join("\n- ")
    );
}

/// The ticket's original one-item reproducer, as a repository scenario: one
/// unreadable root, one held run lock, and one zero spend ceiling. Both
/// reader-facing surfaces must render the root decision and start nothing
/// (§FS-005-dispatch.30, §FS-015-spend-ceiling.6).
#[cfg(unix)]
#[test]
fn issue_90_cli_and_work_screen_render_the_same_root_refusal() {
    let world = ticketed(true);
    unreadable(&world);
    let _holder = hold(&root(&world));
    let mut failures = Vec::new();

    let cli = run(&world, &["work", "run", "--item", ITEM]);
    check_refusal(&mut failures, "command", &cli, ROOT_REFUSAL, LIVE_REFUSAL);
    if stderr(&cli).contains(SPEND_WARNING) {
        failures.push(format!(
            "command emitted a warning over an all-root-refused selection:\n{}",
            stderr(&cli)
        ));
    }

    let tui = screen(&world);
    let transcript = terminal_text(&tui.stdout);
    if !tui.status.success() {
        failures.push(format!(
            "the work screen did not preserve its successful terminal exit: {:?}\n{transcript}\n{}",
            tui.status.code(),
            stderr(&tui)
        ));
    }
    if !terminal_has(&transcript, ROOT_REFUSAL) {
        failures.push(format!(
            "work screen omitted the root refusal:\n{transcript}"
        ));
    }
    if terminal_has(&transcript, LIVE_REFUSAL) {
        failures.push(format!(
            "work screen replaced the shared root decision with its live-run guard:\n{transcript}"
        ));
    }
    if terminal_has(&transcript, SPEND_WARNING) {
        failures.push(format!(
            "work screen emitted a warning over an all-root-refused selection:\n{transcript}"
        ));
    }
    if marker(&world).exists() {
        failures.push("either surface launched the runtime".to_string());
    }

    assert!(
        failures.is_empty(),
        "the two named-run surfaces did not render one decision:\n- {}",
        failures.join("\n- ")
    );
}

/// Run `jwRqq` through a real pseudo-terminal: down to the project, open its
/// work, press the run key, then leave both screens. Each key waits out a
/// fixed settle window first, so a slower renderer (seen under load on the
/// macOS runner) gets to finish one redraw before the next key lands
/// mid-frame, the way a real reader's keystrokes are spaced out rather than
/// arriving as one burst. The window is fixed rather than "wait for quiet"
/// because the screen redraws on its own tick and is never actually quiet.
///
/// The pty is unusually wide (see `COLS` on `terminal_text` below): the work
/// screen's header is one fixed row with no wrap, and the refusal names the
/// work root's absolute path twice. macOS hands out a `/private/var/folders/…`
/// world path far longer than Linux's `/tmp/…`, long enough on its own to
/// clip the message before the words this case checks for — a fact about
/// where the operating system puts a temp directory, not about the decision
/// under test, so the terminal is sized to never clip it rather than
/// narrowed to whatever a real reader's window happens to be. Python's
/// standard-library PTY is the test-only scaffold; the ephor binary and
/// every seam it calls are the real path under test.
#[cfg(unix)]
fn screen(world: &World) -> Output {
    let ephor = world.ephor_raw();
    let mut command = Command::new("python3");
    for (key, value) in ephor.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        }
    }
    command
        .env("NO_COLOR", "1")
        .arg("-c")
        .arg(
            r#"import fcntl, os, pty, select, struct, subprocess, sys, termios, time
master, slave = pty.openpty()
fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 400, 0, 0))
child = subprocess.Popen([sys.argv[1], "tui"], stdin=slave, stdout=slave, stderr=slave)
os.close(slave)
chunks = []
deadline = time.monotonic() + 15

def settle(seconds):
    end = min(time.monotonic() + seconds, deadline)
    while time.monotonic() < end:
        ready, _, _ = select.select([master], [], [], 0.05)
        if ready:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                return
            if not chunk:
                return
            chunks.append(chunk)
        if child.poll() is not None:
            return

settle(0.5)
for key in b"jwRqq":
    os.write(master, bytes([key]))
    settle(0.5)

while time.monotonic() < deadline:
    ready, _, _ = select.select([master], [], [], 0.1)
    if ready:
        try:
            chunk = os.read(master, 65536)
        except OSError:
            break
        if not chunk:
            break
        chunks.append(chunk)
    if child.poll() is not None:
        break

if child.poll() is None:
    child.terminate()
    child.wait(timeout=2)
    status = 124
else:
    status = child.wait()
os.close(master)
sys.stdout.buffer.write(b"".join(chunks))
sys.exit(status)
"#,
        )
        .arg(ephor.get_program())
        .output()
        .expect("drive the work screen through a pseudo-terminal")
}

/// Replay the raw stream onto a fixed grid the size of the pty (the
/// TIOCSWINSZ above) and read back the cells, rather than concatenating
/// bytes in write order. A full-screen renderer repaints by moving the
/// cursor and writing only the cells that changed, so two halves of one line
/// can be written with an unrelated, later-changing region of the screen (a
/// header clock, say) landing in between them; write order is not reading
/// order. Replaying cursor moves and erases onto a grid gets back what a
/// person watching the terminal actually saw. The assertion is on that
/// answer, not on a renderer's cursor movement.
fn terminal_text(bytes: &[u8]) -> String {
    const ROWS: usize = 24;
    const COLS: usize = 400;
    let mut grid = vec![vec![' '; COLS]; ROWS];
    let mut row = 0usize;
    let mut col = 0usize;

    let chars: Vec<char> = String::from_utf8_lossy(bytes).chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c != '\u{1b}' {
            match c {
                '\r' => col = 0,
                '\n' => row = (row + 1).min(ROWS - 1),
                _ if (c as u32) < 0x20 => {}
                _ => {
                    if col >= COLS {
                        row = (row + 1).min(ROWS - 1);
                        col = 0;
                    }
                    grid[row][col] = c;
                    col += 1;
                }
            }
            at += 1;
            continue;
        }
        if chars.get(at + 1) != Some(&'[') {
            at += 1;
            continue;
        }
        let mut end = at + 2;
        while end < chars.len() && !('\u{40}'..='\u{7e}').contains(&chars[end]) {
            end += 1;
        }
        if end >= chars.len() {
            break;
        }
        let final_byte = chars[end];
        let params: Vec<Option<i64>> = chars[at + 2..end]
            .iter()
            .collect::<String>()
            .trim_start_matches('?')
            .split(';')
            .map(|param| param.parse::<i64>().ok())
            .collect();
        let moved_by = |index: usize| -> usize {
            params
                .get(index)
                .copied()
                .flatten()
                .filter(|value| *value > 0)
                .unwrap_or(1) as usize
        };
        let erase_mode =
            |index: usize| -> i64 { params.get(index).copied().flatten().unwrap_or(0) };
        match final_byte {
            'H' | 'f' => {
                row = (moved_by(0) - 1).min(ROWS - 1);
                col = (moved_by(1) - 1).min(COLS - 1);
            }
            'A' => row = row.saturating_sub(moved_by(0)),
            'B' => row = (row + moved_by(0)).min(ROWS - 1),
            'C' => col = (col + moved_by(0)).min(COLS - 1),
            'D' => col = col.saturating_sub(moved_by(0)),
            'J' => {
                if matches!(erase_mode(0), 2 | 3) {
                    grid = vec![vec![' '; COLS]; ROWS];
                }
            }
            'K' => match erase_mode(0) {
                0 => grid[row][col..].fill(' '),
                1 => grid[row][..=col].fill(' '),
                _ => grid[row].fill(' '),
            },
            'h' if erase_mode(0) == 1049 => grid = vec![vec![' '; COLS]; ROWS],
            _ => {}
        }
        at = end + 1;
    }

    grid.into_iter()
        .map(|line| line.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn terminal_has(transcript: &str, phrase: &str) -> bool {
    let letters = |text: &str| {
        text.chars()
            .filter(|character| character.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    letters(transcript).contains(&letters(phrase))
}

/// Add another known matter in its own project by copying the dispatch's own
/// plan and ledger row. This keeps the mixed-selection check about named-run
/// classification rather than about a second dispatch implementation.
fn add_root_refused_project(world: &World) {
    let mut registry = world.registry_doc();
    let mut project = registry["projects"][0].clone();
    project["id"] = json!("broken");
    project["display_name"] = json!("Broken");
    project["root"] = json!(world.path().join("broken"));
    project["branches"] = json!([]);
    registry["projects"]
        .as_array_mut()
        .expect("project rows")
        .push(project);
    write_json(&world.registry_path(), &registry);

    let ledger_path = world.path().join("state/ephor/work.json");
    let mut ledger = read_json(&ledger_path);
    let mut copied = ledger["entries"][ITEM].clone();
    let source_plan = PathBuf::from(copied["plan"].as_str().expect("the dispatched plan's path"));
    let second_root = world.path().join("broken/panta");
    fs::create_dir_all(&second_root).expect("the second work root");
    let second_plan = second_root.join("acme-app-2.rhei.md");
    fs::copy(source_plan, &second_plan).expect("copy the dispatched plan");
    copied["project"] = json!("broken");
    copied["root"] = json!(second_root);
    copied["checkout"] = json!(world.path().join("broken"));
    copied["branch"] = Value::Null;
    copied["plan_id"] = json!("acme-app-2");
    copied["plan"] = json!(second_plan);
    ledger["entries"][OTHER_ITEM] = copied;
    write_json(&ledger_path, &ledger);

    let mut config = read_json(&world.config_path());
    config["projects"]["broken"] = json!({ "providers": [] });
    write_json(&world.config_path(), &config);
}

fn set_project_budget(world: &World, project: &str, full: bool) {
    let mut config = read_json(&world.config_path());
    config["projects"][project]["work"]["max_spend"] = match full {
        true => json!({ "amount": 0, "currency": "USD", "per": "24h" }),
        false => Value::Null,
    };
    write_json(&world.config_path(), &config);
}
