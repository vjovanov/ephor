//! E2E-005-dispatch: a watched item becomes tickets, and a runtime turns them into work.
//!
//! The scenario is the loop ephor exists to close (§FS-005-dispatch): a pull
//! request with a red gate and an unanswered question is handed to an agent
//! runtime, and what the runtime made of it comes back to the person who
//! dispatched it. The two halves are deliberately separate. Writing tickets is
//! ephor's: it happens with no runtime installed at all, and the plans sit on
//! disk waiting. Running them is the runtime's: it is a binding, and with
//! nothing bound on PATH the run is refused in the ladder's own words rather
//! than half-attempted (§FS-006-project-interface.10, §DA-001-runtime-bound-default).
//!
//! What comes back is read out of the work root every time it is asked for: the
//! ticket's state out of the plan the runtime owns (§FS-005-dispatch.4), the
//! verdict out of the file the state machine asks for, and a drafted reply out
//! of the file ephor named — which stays a file until a person posts it
//! (§FS-005-dispatch.13).

#[path = "../support.rs"]
mod support;

use predicates::prelude::*;
use serde_json::json;

use support::*;

/// The plan id ephor derives from the item's key, which is also what it hands
/// the runtime on the command line.
const PLAN: &str = "acmeforge-app-101";

/// A forge with one pull request of the user's: the gate is red and the last
/// word in the conversation is somebody else's, so it is both a thing to fix
/// and a thing to answer.
const ACME_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"pull_requests":true,"conversation":true,"gate":true,"replies":true}'
    ;;
  pull-requests)
    printf '%s' '[
      { "id": "app/101", "repo": "app", "number": "101",
        "title": "Widen the retry window",
        "url": "https://acme.example/pr/101",
        "branch": "you/ABC-42-retry",
        "updated_at": "2026-07-30T12:00:00Z",
        "role": "author", "state": "open", "cited": false,
        "threads": [ { "messages": [
          { "author": "Ada", "text": "does the window reset per attempt?",
            "when": "2026-07-30T11:00:00Z", "mine": false } ] } ],
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime in bash. Asked to `run`, it reads the plan it was handed, writes
/// the verdict the shipped state machine asks each ticket for, drafts the
/// reply where ephor said to, and moves the tickets it finished into the
/// final state. Asked for a `transition`, it moves the one ticket named,
/// checking the state it is expected to be in, and records the result the
/// move carried — which is the verb a cancel asks for
/// (§FS-005-dispatch.16). Work state is the runtime's, which is why it is the
/// runtime that edits the plan (§FS-005-dispatch.4).
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift
if [ "$verb" = transition ]; then
  file="$1"; shift
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
  current="$(awk -v task="$task" '
    /^### Task / { current = $3; sub(":", "", current) }
    /^\*\*State:\*\*/ && current == task { print $2; exit }
  ' "$file")"
  if [ "$current" != "$from" ]; then
    printf '  × Task %s is in state %s, not %s.\n' "$task" "$current" "$from" >&2
    exit 1
  fi
  awk -v task="$task" -v to="$to" '
    /^### Task / { current = $3; sub(":", "", current) }
    /^\*\*State:\*\*/ && current == task { print "**State:** " to; next }
    { print }
  ' "$file" > "$file.tmp" && mv "$file.tmp" "$file"
  stem="$(basename "$file" .rhei.md)"
  mkdir -p "$(dirname "$file")/runtime/results"
  printf '## Result\n\n%s\n' "$result" >> "$(dirname "$file")/runtime/results/$stem.$task.md"
  echo "Task $stem.$task transitioned: '$from' → '$to'"
  exit 0
fi

root="$1"
plan=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --rhei) plan="$2"; shift 2 ;;
    *) shift ;;
  esac
done
artifacts="$root/runtime/ephor"
mkdir -p "$artifacts"
printf '%s\n' "$PWD" > "$artifacts/ran-from"

file="$root/$plan.rhei.md"
for ticket in $(grep -oE '^### Task [A-Za-z0-9_-]+:' "$file" | sed 's/^### Task //; s/:$//'); do
  printf '# %s.%s — verdict\n\nVERDICT: answered — the window resets per attempt\n' \
    "$plan" "$ticket" > "$artifacts/$plan.$ticket.verdict.md"
done
printf 'Yes — the window resets on every attempt, which is what the test asserts.\n' \
  > "$artifacts/$plan.reply.md"

awk '{ if ($0 ~ /^\*\*State:\*\*/) print "**State:** done"; else print }' "$file" > "$file.tmp"
mv "$file.tmp" "$file"
"#;

/// A world watching the forge, with the runtime bound to a name that is not on
/// PATH yet — which is every machine before the runtime is installed.
fn watching() -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": { "runner": "acme-runtime" }
    }));
    world.register(json!({
        "branches": [
            { "id": "demo-retry", "branch": "you/ABC-42-retry", "active": true, "ticket": "ABC-42" }
        ]
    }));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world
}

fn plan_path(world: &World) -> std::path::PathBuf {
    world.forest().join("panta").join(format!("{PLAN}.rhei.md"))
}

/// Autorun is safe to leave enabled because a zero ceiling writes the work
/// but starts none of it, and the due command publishes that decision as a
/// successful machine-readable outcome (§FS-005-dispatch.24).
#[test]
fn an_autorun_ceiling_of_zero_passes_due_work_over_without_failing_the_sweep() {
    let world = watching();
    world.stub("acme-runtime", ACME_RUNTIME);
    world.configure(json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": {
            "runner": "acme-runtime",
            "max_concurrent": 0,
            "recipes": [{
                "id": "fix-gate",
                "icon": "🛠",
                "description": "fix the red gate",
                "state": "fix",
                "needs_checkout": true,
                "autorun": true,
                "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
                "brief": "Fix the gate on {title}."
            }]
        }
    }));

    world
        .ephor()
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed over"));
    let output = world
        .ephor()
        .args(["work", "run", "--due", "--max-concurrent", "0", "--json"])
        .output()
        .expect("the capped sweep runs");
    assert!(output.status.success());
    let reading = json_of(&output);
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert!(reading["runs"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("global work.max_concurrent 0"));
}

/// A budget can be written over the set of projects that actually share a
/// machine, and the reader who runs into it is told which one it was: an
/// organization ceiling passes due work over and names itself, while a project
/// ceiling written above it is said out loud and changes nothing
/// (§FS-005-dispatch.24).
#[test]
fn an_organization_ceiling_passes_due_work_over_and_names_the_organization() {
    let world = watching();
    world.stub("acme-runtime", ACME_RUNTIME);
    world.organize("guild", "The Guild");
    world.configure(json!({
        "organizations": { "guild": { "work": { "max_concurrent": 0 } } },
        "projects": { PROJECT: {
            "providers": [{ "provider": "acmeforge", "user": "you", "repos": ["app"] }],
            "work": { "max_concurrent": 2 }
        } },
        "work": {
            "runner": "acme-runtime",
            "recipes": [{
                "id": "fix-gate",
                "icon": "🛠",
                "description": "fix the red gate",
                "state": "fix",
                "needs_checkout": true,
                "autorun": true,
                "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
                "brief": "Fix the gate on {title}."
            }]
        }
    }));

    world.ephor().args(["work", "dispatch"]).assert().success();
    let output = world
        .ephor()
        .args(["work", "run", "--due", "--json"])
        .output()
        .expect("the capped sweep runs");
    assert!(output.status.success());
    let reading = json_of(&output);
    let said = reading.to_string();
    assert_eq!(reading["failed"], 0, "{reading}");
    assert_eq!(reading["runs"][0]["outcome"], "passed-over", "{reading}");
    assert!(
        reading["runs"][0]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("organizations.guild.work.max_concurrent 0 is full")),
        "{reading}"
    );
    // The project's larger number is honoured as written and named as wrong,
    // rather than being quietly clamped to the organization's.
    assert!(
        said.contains(
            "projects.demo.work.max_concurrent 2 is above \
             organizations.guild.work.max_concurrent 0"
        ),
        "{reading}"
    );
}

/// Tickets are written whether or not anything can run them, and the run says
/// so in the one sentence the whole of ephor refuses with
/// (§AR-005-capabilities.2).
#[test]
fn the_tickets_are_written_and_the_run_is_refused_while_no_runtime_is_installed() {
    let world = watching();

    // A dry run promises where the work goes and writes nothing.
    world
        .ephor()
        .args(["work", "dispatch", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fix-gate"))
        .stdout(predicate::str::contains("1 ticket(s) would be opened"));
    assert!(!world.forest().join("panta").exists());

    world
        .ephor()
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    // The plan carries what the watch knew — the dossier, not a link to it
    // (§FS-005-dispatch.2) — and one ticket in the state the recipe names.
    let plan = std::fs::read_to_string(plan_path(&world)).expect("the plan is on disk");
    assert!(plan.contains("## The item"), "{plan}");
    assert!(plan.contains("https://acme.example/pr/101"), "{plan}");
    assert!(
        plan.contains("does the window reset per attempt?"),
        "{plan}"
    );
    assert!(plan.contains("### Task fix-gate-1:"), "{plan}");
    assert!(plan.contains("**State:** fix"), "{plan}");

    // Nothing on PATH answers to the bound name, so running is refused with
    // the ladder's sentence — and the tickets stay exactly where they are.
    world
        .ephor()
        .args(["work", "run"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "acme-runtime is not on PATH; ephor writes the tickets but the runtime runs them.",
        ));
    assert!(plan_path(&world).is_file());

    let listed = world
        .ephor()
        .args(["work", "list", "--json"])
        .output()
        .expect("the ledger lists");
    let rows = json_of(&listed);
    assert_eq!(rows[0]["item"], "acmeforge:app/101");
    assert_eq!(rows[0]["tickets"][0]["state"], "fix");
    assert_eq!(rows[0]["tickets"][0]["finished"], false);
}

/// With a runtime on PATH the loop closes: it runs from the checkout the work
/// is about, and what it left behind is read back out of the work root
/// (§FS-005-dispatch.3, §FS-005-dispatch.12, §FS-005-dispatch.13).
#[test]
fn a_bound_runtime_runs_the_plan_and_its_verdict_and_drafted_reply_come_back() {
    let world = watching();
    // The question is what this ticket is for, so the reply file is part of
    // what the runtime is asked for.
    world
        .ephor()
        .args([
            "work",
            "dispatch",
            "--item",
            "acmeforge:app/101",
            "--recipe",
            "answer",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 ticket(s) opened"));

    let work_root = world.forest().join("panta");
    let reply = work_root
        .join("runtime/ephor")
        .join(format!("{PLAN}.reply.md"));
    let plan = std::fs::read_to_string(plan_path(&world)).expect("the plan is on disk");
    // The brief names that file absolutely: the runtime runs from the
    // checkout, not from the work root, so a brief that asks for a file has to
    // say which one.
    assert!(
        plan.contains(&reply.to_string_lossy().to_string()),
        "{plan}"
    );

    world.stub("acme-runtime", ACME_RUNTIME);
    world
        .ephor()
        .args(["work", "run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("acme-runtime run"));

    // It ran from the checkout the work is about, which is the one thing the
    // ledger records rather than guesses.
    assert_eq!(
        std::fs::read_to_string(work_root.join("runtime/ephor/ran-from"))
            .expect("the runtime recorded where it ran")
            .trim(),
        world.forest().to_string_lossy()
    );

    // The state is read back out of the plan the runtime owns, never out of
    // the ledger, and the verdict out of the file the state machine asked for.
    let listed = world
        .ephor()
        .args(["work", "list", "--json"])
        .output()
        .expect("the ledger lists");
    let rows = json_of(&listed);
    let ticket = &rows[0]["tickets"][0];
    assert_eq!(ticket["id"], "answer-1");
    assert_eq!(ticket["state"], "done");
    assert_eq!(ticket["finished"], true);
    assert_eq!(
        ticket["verdict"],
        "answered — the window resets per attempt"
    );

    // The reply is read back whole — a reply summarized is a different reply —
    // and it is still a file. Posting is the person's (§FS-005-dispatch.13).
    let proposal =
        ephor::work::runtime::results::proposal(&work_root, PLAN).expect("the run drafted a reply");
    assert_eq!(
        proposal.text,
        "Yes — the window resets on every attempt, which is what the test asserts."
    );
    assert_eq!(proposal.path, reply);
    assert!(reply.is_file());
    assert!(!work_root
        .join("runtime/ephor")
        .join(format!("{PLAN}.reply.posted.md"))
        .exists());
}

/// The same recipe pressed twice is two tickets about one fix, and the second
/// is taken back through the runtime's own move (§FS-005-dispatch.16): the
/// plan keeps it, marked cancelled with the reason as its result; what was
/// ordered after it is named as left waiting; and the next ticket ephor
/// writes is ordered after the last one not taken back — never after the
/// abandoned one. With no runtime installed the cancel is refused in the
/// workable rung's words, and the plan is left exactly as it was.
#[test]
fn a_ticket_asked_for_twice_is_taken_back_and_the_plan_keeps_the_record() {
    let world = watching();
    for again in [false, true] {
        let mut args = vec![
            "work",
            "dispatch",
            "--item",
            "acmeforge:app/101",
            "--recipe",
            "fix-gate",
        ];
        if again {
            args.push("--again");
        }
        world
            .ephor()
            .args(&args)
            .assert()
            .success()
            .stdout(predicate::str::contains("1 ticket(s) opened"));
    }
    let plan = std::fs::read_to_string(plan_path(&world)).expect("the plan is on disk");
    assert!(plan.contains("### Task fix-gate-1:"), "{plan}");
    assert!(plan.contains("### Task fix-gate-2:"), "{plan}");
    assert!(plan.contains("**Prior:** Task fix-gate-1"), "{plan}");

    // Nobody on PATH can make the move: refused in the rung's sentence, and
    // the plan is untouched (§FS-005-dispatch.16).
    world
        .ephor()
        .args([
            "work",
            "cancel",
            "--item",
            "acmeforge:app/101",
            "fix-gate-1",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("acme-runtime is not on PATH"));
    assert_eq!(std::fs::read_to_string(plan_path(&world)).unwrap(), plan);

    world.stub("acme-runtime", ACME_RUNTIME);
    // Cancelling the first names the second as left waiting: it is ordered
    // after a ticket that now satisfies no prior.
    world
        .ephor()
        .args([
            "work",
            "cancel",
            "--item",
            "acmeforge:app/101",
            "fix-gate-1",
            "--why",
            "asked twice by mistake",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("⊘ fix-gate-1 cancelled"))
        .stdout(predicate::str::contains("fix-gate-2 is ordered after it"));
    let plan = std::fs::read_to_string(plan_path(&world)).unwrap();
    assert!(
        plan.contains(
            "### Task fix-gate-1: fix the red gate — Widen the retry window\n**State:** cancelled"
        ),
        "{plan}"
    );
    assert!(
        plan.contains(
            "### Task fix-gate-2: fix the red gate — Widen the retry window\n**State:** fix"
        ),
        "{plan}"
    );

    // Read back: taken back, with the reason the runtime recorded.
    let listed = world
        .ephor()
        .args(["work", "list", "--json"])
        .output()
        .expect("the ledger lists");
    let rows = json_of(&listed);
    let one = &rows[0]["tickets"][0];
    assert_eq!(one["id"], "fix-gate-1");
    assert_eq!(one["state"], "cancelled");
    assert_eq!(one["cancelled"], true);
    assert_eq!(one["finished"], true);
    assert_eq!(one["verdict"], "asked twice by mistake");
    assert_eq!(rows[0]["tickets"][1]["cancelled"], false);

    // Twice is once: the second cancel of the same ticket is refused, and so
    // is one on a ticket that is not there.
    world
        .ephor()
        .args([
            "work",
            "cancel",
            "--item",
            "acmeforge:app/101",
            "fix-gate-1",
            "fix-gate-9",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("fix-gate-1 is already cancelled"))
        .stderr(predicate::str::contains("holds no ticket 'fix-gate-9'"));

    // The next ticket follows the last one not taken back: with fix-gate-2
    // cancelled too, a third ask is ordered after nothing.
    world
        .ephor()
        .args([
            "work",
            "cancel",
            "--item",
            "acmeforge:app/101",
            "fix-gate-2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("⊘ fix-gate-2 cancelled"));
    world
        .ephor()
        .args([
            "work",
            "ask",
            "--item",
            "acmeforge:app/101",
            "just check the retry test",
        ])
        .assert()
        .success();
    let plan = std::fs::read_to_string(plan_path(&world)).unwrap();
    let third = plan
        .split("### Task ask-1:")
        .nth(1)
        .expect("the ask was written");
    assert!(!third.contains("**Prior:**"), "{third}");
    // And the reason left unsaid is recorded as exactly that.
    let rows = json_of(
        &world
            .ephor()
            .args(["work", "list", "--json"])
            .output()
            .unwrap(),
    );
    assert_eq!(
        rows[0]["tickets"][1]["verdict"],
        "Cancelled from ephor; no reason was given."
    );
}

/// An ordering the reader already made, read from a file (§FS-005-dispatch.26).
///
/// Four issues, none needing a checkout or a gate, all matched by the same
/// trivial recipe — so a fixture ranking's effect on the order dispatch comes
/// out in is the only thing any of these scenarios is about. Newest first is
/// today's order among them: `#13`, `#12`, `#11`, `#10`.
mod ranking {
    use super::*;

    const RANKED_FORGE: &str = r#"#!/usr/bin/env bash
set -euo pipefail
cat > /dev/null
case "${1:?subcommand}" in
  capabilities)
    printf '{"issues":true}'
    ;;
  issues)
    printf '%s' '[
      { "key": "acme/widget#10", "title": "Alpha", "url": "https://acme.example/issue/10",
        "updated_at": "2026-07-27T12:00:00Z", "status": "open" },
      { "key": "acme/widget#11", "title": "Beta", "url": "https://acme.example/issue/11",
        "updated_at": "2026-07-28T12:00:00Z", "status": "open" },
      { "key": "acme/widget#12", "title": "Gamma", "url": "https://acme.example/issue/12",
        "updated_at": "2026-07-29T12:00:00Z", "status": "open" },
      { "key": "acme/widget#13", "title": "Delta", "url": "https://acme.example/issue/13",
        "updated_at": "2026-07-30T12:00:00Z", "status": "open" }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

    /// The branch-less replacement for the shipped `implement` recipe, which
    /// keeps these ranking cases about ordering rather than branch workspaces.
    /// Every issue matches: no gate, no role, and no checkout, so nothing
    /// about *whether* an item is worked stands between a fixture ranking and
    /// the order it produces.
    fn trivial_recipe() -> serde_json::Value {
        json!({
            "id": "implement",
            "description": "look at the issue",
            "when": { "kinds": ["issue"] },
            "needs_checkout": false,
            "state": "fix",
            "brief": "Look at {title}."
        })
    }

    fn item(n: u32) -> String {
        format!("acmeforge:acme/widget#{n}")
    }

    fn world_with(work: serde_json::Value) -> World {
        let world = World::new();
        world.stub("ephor-forge-acmeforge", RANKED_FORGE);
        let mut config = json!({
            "projects": { PROJECT: { "providers": [
                { "provider": "acmeforge", "user": "you", "repos": ["widget"] }
            ] } },
        });
        config["work"] = work;
        world.configure(config);
        world.ephor().args(["refresh", PROJECT]).assert().success();
        world
    }

    fn world() -> World {
        world_with(json!({ "recipes": [trivial_recipe()] }))
    }

    fn dispatch(world: &World, extra: &[&str]) -> serde_json::Value {
        let mut args = vec!["work", "dispatch", "--dry-run", "--json"];
        args.extend_from_slice(extra);
        let output = world.ephor().args(&args).output().expect("dispatch runs");
        assert!(
            output.status.success(),
            "dispatch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        json_of(&output)
    }

    fn ids(dispatched: &serde_json::Value) -> Vec<String> {
        dispatched["items"]
            .as_array()
            .expect("items is an array")
            .iter()
            .map(|row| row["item"].as_str().unwrap().to_string())
            .collect()
    }

    fn notes(dispatched: &serde_json::Value) -> Vec<String> {
        dispatched["notes"]
            .as_array()
            .expect("notes is an array")
            .iter()
            .map(|note| note.as_str().unwrap().to_string())
            .collect()
    }

    /// With no ranking named at all, behaviour is exactly today's: newest
    /// `updated_at` first, and nothing new said about it.
    #[test]
    fn with_no_ranking_and_no_limit_the_order_and_the_reading_are_unchanged() {
        let world = world();
        let dispatched = dispatch(&world, &[]);
        assert_eq!(
            ids(&dispatched),
            vec![item(13), item(12), item(11), item(10)]
        );
        assert!(notes(&dispatched).is_empty());
    }

    /// Ranked items come first, in exactly the file's own order; everything
    /// the file does not name follows, keeping the order it already had.
    #[test]
    fn ranked_items_dispatch_first_in_file_order_then_unranked_in_the_old_order() {
        let world = world();
        let ranking = world.path().join("ranking.txt");
        std::fs::write(&ranking, format!("{}\n{}\n", item(10), item(12))).unwrap();

        let dispatched = dispatch(&world, &["--ranking", ranking.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(10), item(12), item(13), item(11)]
        );
        let says = notes(&dispatched);
        assert!(
            says.iter()
                .any(|note| note.contains(&ranking.display().to_string())
                    && note.contains("2 item(s) named")),
            "{says:?}"
        );
    }

    /// An id the file names that matches no item in the feed is skipped and
    /// named in the reading; the sweep continues over everything else.
    #[test]
    fn an_unmatched_id_is_skipped_and_reported_the_sweep_continues() {
        let world = world();
        let ranking = world.path().join("ranking.txt");
        std::fs::write(
            &ranking,
            format!("acmeforge:acme/widget#404\n{}\n", item(11)),
        )
        .unwrap();

        let dispatched = dispatch(&world, &["--ranking", ranking.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(11), item(13), item(12), item(10)]
        );
        let says = notes(&dispatched);
        assert!(
            says.iter()
                .any(|note| note.contains("acmeforge:acme/widget#404")
                    && note.contains("matches no item")),
            "{says:?}"
        );
    }

    /// A ranking file naming the same id twice ranks it once, not twice, and
    /// never reports the repeated line as matching nothing.
    #[test]
    fn a_duplicated_id_is_ranked_once_and_never_reported_unmatched() {
        let world = world();
        let ranking = world.path().join("ranking.txt");
        std::fs::write(
            &ranking,
            format!("{}\n{}\n{}\n", item(10), item(10), item(12)),
        )
        .unwrap();

        let dispatched = dispatch(&world, &["--ranking", ranking.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(10), item(12), item(13), item(11)]
        );
        let says = notes(&dispatched);
        assert!(
            !says.iter().any(|note| note.contains("matches no item")),
            "{says:?}"
        );
    }

    /// A ranking file that is not there falls back to today's order, and the
    /// reading says so rather than failing.
    #[test]
    fn an_absent_ranking_file_falls_back_and_says_so() {
        let world = world();
        let missing = world.path().join("nope.txt");

        let dispatched = dispatch(&world, &["--ranking", missing.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(13), item(12), item(11), item(10)]
        );
        let says = notes(&dispatched);
        assert!(
            says.iter().any(|note| note.contains("is not there")),
            "{says:?}"
        );
    }

    /// An empty ranking file is the same non-error, said in its own words.
    #[test]
    fn an_empty_ranking_file_falls_back_and_says_so() {
        let world = world();
        let empty = world.path().join("empty.txt");
        std::fs::write(&empty, "\n\n").unwrap();

        let dispatched = dispatch(&world, &["--ranking", empty.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(13), item(12), item(11), item(10)]
        );
        let says = notes(&dispatched);
        assert!(
            says.iter().any(|note| note.contains("names nothing")),
            "{says:?}"
        );
    }

    /// `--limit` bounds how many items are dispatched, taken from the top of
    /// the ranked order — never from what a filter or existing work stepped
    /// over, because neither of those consumes the bound.
    #[test]
    fn limit_bounds_the_dispatched_set_from_the_top_of_the_order() {
        let world = world();
        let ranking = world.path().join("ranking.txt");
        std::fs::write(
            &ranking,
            format!("{}\n{}\n{}\n{}\n", item(10), item(11), item(12), item(13)),
        )
        .unwrap();

        let dispatched = dispatch(
            &world,
            &["--ranking", ranking.to_str().unwrap(), "--limit", "2"],
        );
        assert_eq!(dispatched["opened"], 2);
        assert_eq!(ids(&dispatched), vec![item(10), item(11)]);
    }

    /// `--limit` with no ranking bounds the same newest-first sweep.
    #[test]
    fn limit_bounds_the_sweep_even_with_no_ranking() {
        let world = world();
        let dispatched = dispatch(&world, &["--limit", "1"]);
        assert_eq!(dispatched["opened"], 1);
        assert_eq!(ids(&dispatched), vec![item(13)]);
    }

    /// `--limit 0` does not block an explicit `--item` request: naming one
    /// matter is not the sweep the bound bounds.
    #[test]
    fn limit_zero_does_not_block_an_explicit_item_request() {
        let world = world();
        let dispatched = dispatch(&world, &["--item", &item(11), "--limit", "0"]);
        assert_eq!(dispatched["opened"], 1);
        assert_eq!(ids(&dispatched), vec![item(11)]);
    }

    /// A ranking path that cannot be read as a file — a directory, here —
    /// falls back to today's order, and the reading says so.
    #[test]
    fn an_unreadable_ranking_path_falls_back_and_says_so() {
        let world = world();
        let directory = world.path().join("ranking-dir");
        std::fs::create_dir(&directory).unwrap();

        let dispatched = dispatch(&world, &["--ranking", directory.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(13), item(12), item(11), item(10)]
        );
        let says = notes(&dispatched);
        assert!(
            says.iter().any(|note| note.contains("could not be read")),
            "{says:?}"
        );
    }

    /// The `work.ranking` config key orders dispatch with no flag at all, and
    /// `--ranking` displaces it for exactly one run.
    #[test]
    fn the_configured_ranking_orders_dispatch_and_the_flag_displaces_it_for_one_run() {
        let configured = tempfile::tempdir().expect("a temp dir for the configured file");
        let configured_path = configured.path().join("configured.txt");
        std::fs::write(&configured_path, format!("{}\n", item(11))).unwrap();
        let world = world_with(json!({
            "recipes": [trivial_recipe()],
            "ranking": configured_path.to_str().unwrap(),
        }));

        let dispatched = dispatch(&world, &[]);
        assert_eq!(
            ids(&dispatched),
            vec![item(11), item(13), item(12), item(10)]
        );

        let displaced = world.path().join("displaced.txt");
        std::fs::write(&displaced, format!("{}\n", item(10))).unwrap();
        let dispatched = dispatch(&world, &["--ranking", displaced.to_str().unwrap()]);
        assert_eq!(
            ids(&dispatched),
            vec![item(10), item(13), item(12), item(11)]
        );
    }
}
