//! E2E-018-a-ceiling-on-what-is-spent: a person writes down what unattended
//! work may spend, and the sweep stops there.
//!
//! The scenario is a machine left running overnight (§FS-005-dispatch.24). It
//! has ceilings on how many roots may be live and how many may be working, and
//! neither of those is money — so until now the only thing that stopped it was
//! the vendor's quota. Here the person says the number instead, in site
//! configuration and in either denomination, and the sweep reads it against
//! what the agent tools already wrote down (§FS-015-spend-ceiling).
//!
//! Four refusals are what this case is really about, and every one of them is
//! a refusal to do something.
//!
//! The budget refuses the **sweep** and never the person: a run somebody typed
//! warns and proceeds, because a cap is the human's leash on the machine
//! (§FS-015-spend-ceiling.6). It refuses to block on a **hole**: a window
//! nothing priced binds nothing in dollars, and the tokens no price covered
//! are shown beside the total rather than summed as zero
//! (§FS-015-spend-ceiling.7). It refuses to be **silent**: a currently binding
//! ceiling says so on `status`, in one line and in the machine form, because a
//! paused loop and an idle loop look identical otherwise
//! (§FS-015-spend-ceiling.10). And it refuses to **price anything**: every
//! number below is one the transcripts carried, which is why the fixture is a
//! transcript and not a price book (§FS-015-spend-ceiling.2).

#[path = "../support.rs"]
mod support;

use chrono::{DateTime, Duration, SecondsFormat, TimeZone, Utc};
use predicates::prelude::*;
use serde_json::{json, Value};

use support::*;

/// The item the recipe turns into a ticket, and the plan ephor derives from it.
const ITEM: &str = "acmeforge:app/101";

/// One model, spelled the same way by the call and by the rollup, so the
/// tokens and the dollars land in one bucket rather than in two rows nothing
/// can add up (§FS-013-burn.3).
const MODEL: &str = "acme-model-1";

/// How long a bucket of the store is, in seconds. A fixture written on a span
/// boundary lands in a span that starts exactly there, which is what makes the
/// resume instant below an exact number rather than an approximate one.
const SPAN: i64 = 300;

/// A forge with one pull request of the reader's own and a red gate, so there
/// is a matter a recipe will pick up and hand over.
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
        "gate": { "repos": [ { "repo": "app", "passed": 5, "failed": 1, "running": 0 } ] } }
    ]'
    ;;
  *)
    printf '[]'
    ;;
esac
"#;

/// A runtime that detaches and reports itself finished inside the handshake.
/// The case is about which starts are admitted, not about what a run does, so
/// a run that is over by the time the launcher returns leaves no lock behind
/// to confuse the next command (§FS-005-dispatch.20).
///
/// It also carries one workflow, so the person's other way of putting work
/// down — `work lay` — has something to lay. It asks for nothing, because what
/// the plan says is not this case's subject: whether a full ceiling is said
/// over it is (§FS-015-spend-ceiling.6).
const ACME_RUNTIME: &str = r#"#!/usr/bin/env bash
set -euo pipefail
verb="$1"; shift
if [ "$verb" = run ] && [ "${1:-}" = --help ]; then
  printf '%s\n' '      --headless  Detach the run'
  exit 0
fi
if [ "$verb" = templates ]; then
  printf '%s\n' '[ { "name": "tidy", "version": "1.0.0", "source": "user",
    "path": "tidy", "description": "Tidy the change.", "inputs": [] } ]'
  exit 0
fi
if [ "$verb" = instantiate ]; then
  output=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --output) output="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  if [ -n "$output" ]; then
    mkdir -p "$output"
    printf '# tidy\n' > "$output/index.rhei.md"
  fi
  printf '%s\n' 'laid tidy down'
  exit 0
fi
printf '%s\n' '{"id":"acme-run","status":"finished"}'
"#;

/// The start of the five-minute span a moment falls in — the store's own
/// arithmetic, repeated here so a case can name the instant a bucket will
/// leave a trailing window.
fn span_start(at: DateTime<Utc>) -> DateTime<Utc> {
    let seconds = at.timestamp();
    Utc.timestamp_opt(seconds - seconds.rem_euclid(SPAN), 0)
        .single()
        .expect("a span start")
}

/// An instant as the reading writes it: RFC 3339, to the second, in UTC.
fn instant(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// A session the agent tool wrote down, in the tool's own transcript under the
/// person's home: one paid call, and — only where `cost` says so — the tool's
/// own dollar rollup for it.
///
/// This is the whole of the fixture. Ephor is given no prices and no totals:
/// it is given the log the tool was writing anyway, which is the only thing a
/// budget is ever allowed to bind on (§FS-015-spend-ceiling.2).
fn spent(world: &World, session: &str, at: DateTime<Utc>, tokens: u64, cost: Option<f64>) {
    let mut records = vec![json!({
        "type": "assistant",
        "sessionId": session,
        "cwd": world.forest().to_string_lossy(),
        "timestamp": instant(at),
        "requestId": format!("{session}-request"),
        "message": {
            "id": format!("{session}-message"),
            "model": MODEL,
            "usage": { "input_tokens": tokens, "output_tokens": 0 }
        }
    })];
    if let Some(dollars) = cost {
        // The rollup restates the session's running cost and carries no time
        // of its own, so it lands where the call above did (§FS-013-burn.3).
        records.push(json!({
            "type": "cost-state",
            "sessionId": session,
            "modelUsage": { MODEL: { "costUSD": dollars } }
        }));
    }
    let text = records
        .iter()
        .map(|record| record.to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let path = world
        .path()
        .join(".claude/projects/demo")
        .join(format!("{session}.jsonl"));
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the transcript directory");
    std::fs::write(&path, text).expect("the transcript");
}

/// A world watching the forge, with a runtime bound and one recipe that asks
/// to run itself. `work` is merged over that, so a case states only the
/// ceilings its scenario is about.
fn watching(work: Value) -> World {
    let world = World::new();
    world.stub("ephor-forge-acmeforge", ACME_FORGE);
    world.stub("acme-runtime", ACME_RUNTIME);
    // Fetched before the budget is written, so what a case fails on is a
    // command reading the ceiling rather than the fetch that put the matter in
    // the feed. A budget is not a fact about the forge and changes nothing
    // about what is watched.
    world.configure(configured(json!({})));
    world.ephor().args(["refresh", PROJECT]).assert().success();
    world.configure(configured(work));
    world
}

/// The site configuration, with `work` merged over the runner and the recipe
/// every case needs.
fn configured(work: Value) -> Value {
    let mut settings = json!({
        "projects": { PROJECT: { "providers": [
            { "provider": "acmeforge", "user": "you", "repos": ["app"] }
        ] } },
        "work": {
            "runner": "acme-runtime",
            "recipes": [{
                "id": "fix-gate",
                "icon": "🛠",
                "description": "fix the red gate",
                "state": "fix",
                "needs_checkout": false,
                "autorun": true,
                "when": { "kinds": ["pr"], "roles": ["author"], "gate": "failing" },
                "brief": "Fix the gate on {title}."
            }]
        }
    });
    merge_into(&mut settings, json!({ "work": work }));
    settings
}

fn merge_into(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                match base.get_mut(&key) {
                    Some(existing) => merge_into(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

/// Write the ticket the sweep will find, without asking what came of the
/// autorun `dispatch` starts on its way past: `dispatch` is the person's own
/// act and is never bound by a budget, which is [`the_person_who_types_the_command_is_warned_and_never_refused`]'s
/// subject rather than this helper's (§FS-015-spend-ceiling.6).
fn ticketed(world: &World) {
    world.ephor().args(["work", "dispatch"]).assert().success();
}

/// The one sweep a budget binds, as a reading.
fn sweep(world: &World) -> Value {
    let output = world
        .ephor()
        .args(["work", "run", "--due", "--json"])
        .output()
        .expect("the sweep runs");
    assert!(
        output.status.success(),
        "a sweep that started nothing is still a successful sweep:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    json_of(&output)
}

/// A dollar ceiling the site wrote is read, is full, and says so with
/// everything a reader needs to act on it: the key that carries it, the
/// number, what was measured, the tokens no price covered, and the instant the
/// trailing window lets work start again (§FS-015-spend-ceiling.9).
#[test]
fn a_dollar_ceiling_the_site_wrote_passes_the_sweep_over_and_says_when_it_lifts() {
    let world = watching(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));
    spent(&world, "unpriced-work", at, 12_000_000, None);
    ticketed(&world);

    // Nothing has read the transcripts yet. The sweep does it itself — the
    // store is a local file the tools were writing anyway, so refreshing it is
    // not the fetch `refresh` owns (§FS-015-spend-ceiling.1).
    let swept = sweep(&world);
    assert_eq!(swept["failed"], 0, "{swept}");
    assert_eq!(swept["runs"][0]["outcome"], "passed-over", "{swept}");
    let why = swept["runs"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        why.contains("global work.max_spend 50 USD per 24h is full"),
        "the refusal names the key that carries the ceiling: {why}"
    );
    assert!(
        why.contains("52.40 USD measured"),
        "the refusal names what was measured: {why}"
    );
    assert!(
        why.contains("12000000 token(s) unpriced"),
        "the refusal shows the hole rather than summing past it: {why}"
    );
    assert!(
        why.contains(&format!("resumes {}", instant(at + Duration::hours(24)))),
        "the refusal names the instant this spend ages out of the window: {why}"
    );
}

/// The sweep a timer runs is bound wherever it runs, and `ephor work sync`
/// ends in the same one `--due` is. The unit ephor ships runs the two in that
/// order every half hour, so a budget that bound only `--due` would let the
/// sync start the night's work unbound and leave the bound sweep finding those
/// roots already held (§FS-015-spend-ceiling.6).
#[test]
fn the_sweep_at_the_end_of_a_sync_is_bound_exactly_as_the_due_sweep_is() {
    let world = watching(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));
    ticketed(&world);

    // Nobody typed this one either: it is the trigger the timer runs, and it
    // is refused in the same words `--due` is refused in.
    let synced = world
        .ephor()
        .args(["work", "sync"])
        .output()
        .expect("the sync runs");
    assert!(synced.status.success(), "{synced:?}");
    let said = String::from_utf8_lossy(&synced.stdout).to_string();
    assert!(
        said.contains("passed over"),
        "the sweep a sync ends in started nothing: {said}"
    );
    assert!(
        said.contains("global work.max_spend 50 USD per 24h is full"),
        "and it gives the reason a bound sweep gives: {said}"
    );

    // And the root it did not start is still due, which is what tells a root
    // passed over from one a sweep quietly took.
    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "passed-over", "{swept}");
}

/// A ceiling with room admits the start, and a site that never wrote one is
/// the site it always was: omission is unlimited, and nothing configured
/// today changes meaning (§FS-015-spend-ceiling.4).
#[test]
fn a_window_with_room_admits_the_start_and_omitting_the_budget_changes_nothing() {
    let world = watching(json!({
        "max_spend": { "amount": 1000, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));
    ticketed(&world);

    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "done", "{swept}");
    assert!(
        !swept["runs"][0]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("max_spend"),
        "a ceiling with room is not a reason for anything: {swept}"
    );

    // The same world with the key taken out, which is every site before this
    // existed.
    world.configure(configured(json!({})));
    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "done", "{swept}");

    // And the number the ceiling was measured against is the one `burn`
    // publishes from the same transcripts — not a second accounting of
    // ephor's own (§FS-015-spend-ceiling.1). A reading that disagrees with
    // this is a fixture problem rather than a ceiling problem.
    let reading = json_of(
        &world
            .ephor()
            .args(["burn", "--window", "24h", "--by", "project", "--json"])
            .output()
            .expect("the reading runs"),
    );
    assert_eq!(reading["totals"]["tokens"], 1_000_000, "{reading}");
    assert_eq!(reading["totals"]["cost_usd"], 52.40, "{reading}");
}

/// The cap is the human's leash on the machine, not on the human. A person who
/// types a command is present and deciding, so a full ceiling warns them and
/// the run starts anyway — the warning on the error stream, where a warning
/// belongs (§FS-015-spend-ceiling.6, §REQ-002-parity.3).
#[test]
fn the_person_who_types_the_command_is_warned_and_never_refused() {
    let world = watching(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));

    // Dispatch is a person opening work, so it says the ceiling is full and
    // opens it.
    world
        .ephor()
        .args(["work", "dispatch"])
        .assert()
        .success()
        .stderr(predicate::str::contains("global work.max_spend"));

    // And a run asked for by name is never refused for budget, only warned
    // about — the sentence §FS-005-dispatch.24 already wrote: a run a reader
    // explicitly starts keeps being the reader's move.
    let named = world
        .ephor()
        .args(["work", "run", "--item", ITEM, "--json"])
        .output()
        .expect("the named run runs");
    assert!(named.status.success(), "{named:?}");
    let reading = json_of(&named);
    assert_eq!(reading["runs"][0]["outcome"], "done", "{reading}");
    assert!(
        String::from_utf8_lossy(&named.stderr).contains("global work.max_spend"),
        "the person is told what would have stopped the sweep: {}",
        String::from_utf8_lossy(&named.stderr)
    );
}

/// The person's other way of putting work down is `ephor work lay`, and it is
/// the same person: the ceiling is said and the plan is laid. Said on the
/// error stream, where a warning belongs and where it leaves the machine form
/// on standard output a reading somebody can parse
/// (§FS-015-spend-ceiling.6, §REQ-002-parity.3).
#[test]
fn a_workflow_the_person_lays_down_is_warned_about_and_still_laid() {
    let world = watching(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));

    // A run asked what it would do makes nothing at all, so there is no
    // laying for a ceiling to be said over — the same silence `work dispatch
    // --dry-run` keeps, and the reason a ceiling is never announced at
    // nobody.
    world
        .ephor()
        .args(["work", "lay", "tidy", "--item", ITEM, "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("max_spend").not());

    let laid = world
        .ephor()
        .args(["work", "lay", "tidy", "--item", ITEM, "--json"])
        .output()
        .expect("the laying runs");
    assert!(laid.status.success(), "{laid:?}");
    assert!(
        String::from_utf8_lossy(&laid.stderr).contains("global work.max_spend 50 USD per 24h"),
        "the person is told what would have stopped the sweep: {}",
        String::from_utf8_lossy(&laid.stderr)
    );

    // And the plan is there: a budget refuses the sweep and never the person.
    let reading = json_of(&laid);
    assert_eq!(reading["workflow"], "tidy", "{reading}");
    assert_eq!(reading["dry_run"], false, "{reading}");
    let plan = reading["plan"].as_str().unwrap_or_default();
    assert!(
        std::path::Path::new(plan).exists(),
        "the laying went through: {reading}"
    );
}

/// A paused loop and an idle loop look identical unless somebody says
/// otherwise, so a currently binding ceiling is on the habitual command: one
/// line, the scope and the resume instant, and nothing when nothing binds. The
/// machine form carries the same fact (§FS-015-spend-ceiling.10).
#[test]
fn the_pause_is_one_line_on_status_and_the_same_fact_in_the_machine_form() {
    let world = watching(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));
    let resumes = instant(at + Duration::hours(24));

    world
        .ephor()
        .args(["status", "--cached"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Autorun paused"))
        .stdout(predicate::str::contains("global work.max_spend"))
        .stdout(predicate::str::contains(resumes.clone()))
        // The totals and the coverage stay in `burn`; `status` is a pause, not
        // a spend report.
        .stdout(predicate::str::contains("52.40").not());

    let reading = json_of(
        &world
            .ephor()
            .args(["status", "--cached", "--json"])
            .output()
            .expect("the reading runs"),
    );
    let project = reading
        .as_array()
        .and_then(|feeds| feeds.iter().find(|feed| feed["project"] == PROJECT))
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(
        project["autorun_paused"]["scope"], "global work.max_spend",
        "{reading}"
    );
    assert_eq!(
        project["autorun_paused"]["resumes_at"], resumes,
        "{reading}"
    );

    // Nothing binds once the ceiling has room, and the line's absence is the
    // signal that it does not.
    world.configure(configured(json!({
        "max_spend": { "amount": 1000, "currency": "USD", "per": "24h" }
    })));
    world
        .ephor()
        .args(["status", "--cached"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Autorun paused").not());
    let reading = json_of(
        &world
            .ephor()
            .args(["status", "--cached", "--json"])
            .output()
            .expect("the reading runs"),
    );
    let project = reading
        .as_array()
        .and_then(|feeds| feeds.iter().find(|feed| feed["project"] == PROJECT))
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(project["autorun_paused"], Value::Null, "{reading}");
}

/// The load-bearing honesty rule, and the reason both denominations ship. A
/// window nothing priced has measured no dollars, so the dollar ceiling binds
/// nothing there — a cap that stopped the machine over an accounting hole
/// would fail over an extractor outage rather than over money. The token
/// ceiling is the one that still bites, because tokens are always measured
/// (§FS-015-spend-ceiling.2, §FS-015-spend-ceiling.7).
#[test]
fn a_window_nothing_priced_binds_no_dollars_and_the_token_ceiling_still_does() {
    let world = watching(json!({
        "max_spend": { "amount": 1, "currency": "USD", "per": "24h" }
    }));
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "unpriced-work", at, 2_000_000, None);
    ticketed(&world);

    // Two million tokens nothing put a price on. A dollar ceiling of one
    // dollar has measured nothing and refuses nobody.
    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "done", "{swept}");

    // The same spend against a ceiling on the thing that was measured.
    world.configure(configured(json!({
        "max_spend": { "amount": 1, "currency": "USD", "per": "24h" },
        "max_tokens": { "amount": 1000000, "per": "24h" }
    })));
    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "passed-over", "{swept}");
    let why = swept["runs"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        why.contains("global work.max_tokens 1000000 per 24h is full"),
        "the denomination that measured something is the one that refuses: {why}"
    );
    assert!(
        why.contains("2000000 token(s) measured"),
        "the refusal names what was measured: {why}"
    );
    assert!(
        !why.contains("max_spend"),
        "a ceiling that measured nothing is not the reason for anything: {why}"
    );
}

/// Every ceiling is evaluated and the outermost full one is the reason, which
/// is how the concurrency ceilings already resolve. The organization's number
/// is full and the project's is too; the reader is told about the wider one
/// (§FS-015-spend-ceiling.5).
#[test]
fn the_outermost_full_scope_is_the_one_the_reader_is_sent_to() {
    let world = watching(json!({}));
    world.organize("guild", "The Guild");
    world.configure({
        let mut settings = configured(json!({
            "max_spend": { "amount": 1000, "currency": "USD", "per": "24h" }
        }));
        merge_into(
            &mut settings,
            json!({
                "organizations": { "guild": { "work": {
                    "max_spend": { "amount": 10, "currency": "USD", "per": "24h" }
                } } },
                "projects": { PROJECT: { "work": {
                    "max_spend": { "amount": 20, "currency": "USD", "per": "24h" }
                } } }
            }),
        );
        settings
    });
    let at = span_start(Utc::now() - Duration::hours(2));
    spent(&world, "priced-work", at, 1_000_000, Some(52.40));
    ticketed(&world);

    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "passed-over", "{swept}");
    let why = swept["runs"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        why.contains("organizations.guild.work.max_spend 10 USD per 24h is full"),
        "the widest full ceiling is the reason: {why}"
    );
}

/// A ceiling of zero is a pause its author meant rather than a window that
/// will pass, so it admits no new autorun start and names no instant to wait
/// for (§FS-015-spend-ceiling.4).
#[test]
fn a_ceiling_of_zero_admits_no_start_and_names_no_instant() {
    let world = watching(json!({
        "max_tokens": { "amount": 0, "per": "24h" }
    }));
    ticketed(&world);

    let swept = sweep(&world);
    assert_eq!(swept["runs"][0]["outcome"], "passed-over", "{swept}");
    let why = swept["runs"][0]["reason"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        why.contains("global work.max_tokens 0 per 24h admits no new autorun starts"),
        "a zero ceiling refuses in its own words: {why}"
    );
    assert!(
        !why.contains("resumes"),
        "nothing ages out into room under a ceiling of zero: {why}"
    );
}

/// A budget ephor cannot bind is refused where a site's typos already land —
/// when the configuration loads — and the refusal names what is accepted
/// rather than leaving the reader to guess. A window the store cannot answer
/// is a budget that cannot bind (§FS-015-spend-ceiling.3).
#[test]
fn a_currency_or_a_window_the_store_cannot_answer_is_refused_by_name() {
    let world = watching(json!({}));

    world.configure(configured(json!({
        "max_spend": { "amount": 50, "currency": "EUR", "per": "24h" }
    })));
    world
        .ephor()
        .args(["work", "run", "--due", "--json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("EUR"))
        .stderr(predicate::str::contains("USD"));

    world.configure(configured(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "fortnight" }
    })));
    let refused = world
        .ephor()
        .args(["work", "run", "--due", "--json"])
        .output()
        .expect("the configuration is read");
    assert!(!refused.status.success(), "{refused:?}");
    let says = String::from_utf8_lossy(&refused.stderr).to_string();
    for window in ["1h", "6h", "24h", "7d"] {
        assert!(
            says.contains(window),
            "the refusal names the windows the store can answer: {says}"
        );
    }

    // And the two spellings the reading already has names for are accepted.
    world.configure(configured(json!({
        "max_spend": { "amount": 50, "currency": "USD", "per": "day" },
        "max_tokens": { "amount": 5000000, "per": "week" }
    })));
    world
        .ephor()
        .args(["work", "run", "--due", "--json"])
        .assert()
        .success();
}
