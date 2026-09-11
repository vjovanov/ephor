//! E2E-023-what-a-reused-checkout-does-not-say: a checkout that is already
//! there says how far behind it is (§FS-004-quick-actions.7.1).
//!
//! The scenario is the run that prompted this: somebody asked for a checkout,
//! was told it was already there, and worked in it for a day — and only at the
//! end, when the change would not land, did anyone learn that the directory was
//! a hundred commits behind the branch it was going to be rebased onto. Nothing
//! had to be measured to prevent that. It was measured already: the branch row
//! for that very directory reads `3 behind main (as of Sep 11)`, and the
//! checkout command that reused it printed `already checked out` and stopped.
//!
//! So this case stands the two surfaces beside each other and holds them to one
//! answer about one directory. `ephor branches` says the distance; `ephor
//! checkout` on the same branch must say the same count, the same base, and the
//! same day — one fold over one forest (§FS-004-quick-actions.6), not a second
//! measurement that can drift from the first. The machine form carries the same
//! fact as a field rather than only inside a sentence
//! (§REQ-002-parity.3), and the published shape declares it
//! (§REQ-002-parity.4).

#[path = "../support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

use support::*;

/// The branch the reader comes back to. Nothing about it is special: it is the
/// directory that is already there.
const BRANCH: &str = "fix/issue-72";

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn commit(dir: &Path, file: &str, message: &str) {
    std::fs::write(dir.join(file), format!("{message}\n")).expect("write the file");
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// The origin, and the project's main checkout beside the branch workspaces —
/// the layout a project whose checkouts are one per branch actually has.
fn project_with_main_checked_out(world: &World) -> PathBuf {
    let origin = world.path().join("origin");
    std::fs::create_dir_all(&origin).expect("the remote");
    git(&origin, &["init", "-q", "--initial-branch=main"]);
    commit(&origin, "README.md", "the project");

    let main = world.forest().join("main");
    std::fs::create_dir_all(main.parent().expect("a parent")).expect("the project root");
    let status = Command::new("git")
        .args(["clone", "-q"])
        .arg(&origin)
        .arg(&main)
        .status()
        .expect("git clones");
    assert!(status.success());
    git(&main, &["config", "user.email", "t@example.com"]);
    git(&main, &["config", "user.name", "t"]);
    origin
}

/// A world whose project keeps one checkout per branch, with the branch
/// already made and main moved on three commits since — and the machine having
/// heard about it, which is what a fetch is.
fn a_checkout_left_behind() -> (World, PathBuf) {
    let world = World::new();
    world.register(json!({
        "branch_root_template": "{project_root}/{branch}",
        "branches": [{ "id": "demo-72", "branch": BRANCH, "active": true }]
    }));
    let origin = project_with_main_checked_out(&world);

    world
        .ephor()
        .args(["checkout", "--project", PROJECT, "--branch", BRANCH])
        .assert()
        .success();
    let workspace = world.forest().join(BRANCH);
    assert!(workspace.is_dir(), "{} was not made", workspace.display());

    for step in 1..=3 {
        commit(
            &origin,
            &format!("moved-{step}.txt"),
            &format!("main moves {step}"),
        );
    }
    // The distance is only as fresh as the last fetch, and nothing under the
    // watch fetches (§FS-004-quick-actions.6). This is that fetch.
    git(&workspace, &["fetch", "-q", "origin"]);
    git(&world.forest().join("main"), &["fetch", "-q", "origin"]);
    (world, workspace)
}

/// What the branch row says about this directory, as a reader reads it.
fn the_row(world: &World) -> String {
    let rows = world
        .ephor_raw()
        .args(["branches", PROJECT])
        .output()
        .expect("ran");
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&rows.stdout),
        String::from_utf8_lossy(&rows.stderr)
    );
    printed
        .lines()
        .find(|line| line.contains(BRANCH))
        .unwrap_or_else(|| panic!("no branch row for {BRANCH}:\n{printed}"))
        .to_string()
}

/// The distance a row states: the count, the base, and the day — pulled out of
/// the row rather than written down here, so the two surfaces are compared
/// with each other and not with a literal this file invented.
fn distance_of(row: &str) -> (u64, String, String) {
    let behind = row
        .split_whitespace()
        .position(|word| word == "behind")
        .unwrap_or_else(|| panic!("the row states no distance: {row}"));
    let words: Vec<&str> = row.split_whitespace().collect();
    let count: u64 = words[behind - 1]
        .parse()
        .unwrap_or_else(|_| panic!("the count is not a number: {row}"));
    let base = words[behind + 1]
        .trim_matches(|c| c == '(' || c == ')')
        .to_string();
    let day = row
        .split("as of ")
        .nth(1)
        .unwrap_or_else(|| panic!("the row states no day: {row}"))
        .trim_end_matches(|c: char| c == ')' || c.is_whitespace())
        .to_string();
    (count, base, day)
}

/// The gap the ticket is about, from the outside: two commands one after the
/// other about one directory, and only one of them says the thing that decides
/// whether working there is worth anything.
#[test]
fn a_checkout_that_was_already_there_says_how_far_behind_it_is() {
    let (world, workspace) = a_checkout_left_behind();
    let (count, base, day) = distance_of(&the_row(&world));
    assert_eq!(count, 3, "the world was built three commits behind");

    let again = world
        .ephor_raw()
        .args(["checkout", "--project", PROJECT, "--branch", BRANCH])
        .output()
        .expect("ran");
    assert!(again.status.success());
    let said = String::from_utf8(again.stdout).expect("utf-8");

    assert!(
        said.contains("already checked out"),
        "the directory was there and the command did not say so:\n{said}"
    );
    assert!(
        said.contains(&format!("{count} behind {base}")),
        "`ephor branches` says `{count} behind {base}` about {}, and the checkout \
         that reused it says nothing:\n{said}",
        workspace.display()
    );
    assert!(
        said.contains(&format!("as of {day}")),
        "a distance with no day on it is a claim about now that nothing measured \
         (§FS-004-quick-actions.6):\n{said}"
    );
}

/// The same fact as a field, for the program that reads this rather than the
/// person — a sentence a caller has to parse is not parity
/// (§REQ-002-parity.3).
#[test]
fn the_machine_form_carries_the_distance_as_a_field() {
    let (world, _) = a_checkout_left_behind();
    let (count, _, _) = distance_of(&the_row(&world));

    let answer = world
        .ephor_raw()
        .args([
            "checkout",
            "--project",
            PROJECT,
            "--branch",
            BRANCH,
            "--json",
        ])
        .output()
        .expect("ran");
    let reading = shaped("checkout", &answer);
    assert_eq!(
        reading["behind"]["behind"],
        json!(count),
        "the reading states no distance: {reading:#}"
    );
    assert!(
        reading["behind"]["as_of"].is_string(),
        "a distance with no day on it: {reading:#}"
    );
}

/// And the shape says so, because a field nothing declares is a field every
/// refactor may break for whoever automated against it (§REQ-002-parity.4).
#[test]
fn the_published_checkout_shape_declares_the_distance() {
    let document: serde_json::Value =
        serde_json::from_str(ephor::api::schema::VIEWS_SCHEMA).expect("the schema parses");
    let checkout = &document["properties"]["checkout"]["properties"];
    assert!(
        checkout["behind"].is_object(),
        "`ephor checkout --json` prints a distance the published shape does not \
         describe:\n{checkout:#}"
    );
}
