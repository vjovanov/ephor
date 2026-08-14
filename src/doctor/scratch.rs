//! The self pass: ephor walked end to end against a project it made up
//! (§FS-010-doctor.3).
//!
//! It builds a throwaway site — its own state directory, registry,
//! configuration and checkout, in a temporary place — and asks the real binary
//! to do the things it claims to do. Hermetic by construction: nothing of the
//! reader's is read, no forge is reached, and the temporary place goes away
//! afterwards (§FS-010-doctor.4).
//!
//! It runs the binary rather than calling into the library, because the
//! question is whether *this* executable still works: argument parsing,
//! configuration loading and exit codes are part of the answer, and a
//! library call would skip all three. That is also what a test suite cannot
//! do — `cargo test` speaks for the tree it ran in.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

/// One thing that was tried, and what went wrong if anything did.
pub struct Check {
    pub name: &'static str,
    /// None is a pass. A check that could not run at all says so here too:
    /// "git is not on PATH" is an honest answer and a failed diagnosis is
    /// still a diagnosis.
    pub failure: Option<String>,
}

pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.failure.is_none())
    }

    pub fn to_json(&self) -> Value {
        json!({
            "passed": self.passed(),
            "checks": self
                .checks
                .iter()
                .map(|check| json!({ "check": check.name, "failure": check.failure }))
                .collect::<Vec<_>>(),
        })
    }
}

/// What one step of the walk answered.
type Step = Result<(), String>;

/// Everything the walk needs to invoke ephor against its own scratch site.
struct Site {
    dir: PathBuf,
    /// The temporary directory's guard: dropping it removes the whole site.
    _guard: TempDir,
}

impl Site {
    fn project(&self) -> PathBuf {
        self.dir.join("project")
    }

    /// Run ephor with nothing of the reader's environment that would change
    /// what it reads. Every path is inside the scratch site.
    fn ephor(&self, args: &[&str]) -> Result<std::process::Output, String> {
        let exe = std::env::current_exe()
            .map_err(|err| format!("cannot find the running ephor: {err}"))?;
        let path = match std::env::var_os("PATH") {
            Some(existing) => format!(
                "{}:{}",
                self.dir.join("bin").display(),
                existing.to_string_lossy()
            ),
            None => self.dir.join("bin").display().to_string(),
        };
        Command::new(exe)
            .args(args)
            .current_dir(&self.dir)
            .env("PATH", path)
            .env("XDG_STATE_HOME", self.dir.join("state"))
            .env("EPHOR_REGISTRY", self.dir.join("registry.json"))
            .env("EPHOR_STATUS_CONFIG", self.dir.join("status.json"))
            // A scratch run must not read the reader's own secrets, and has
            // none of its own to read.
            .env("EPHOR_SECRETS_DIR", self.dir.join("secrets"))
            .output()
            .map_err(|err| format!("cannot run ephor: {err}"))
    }

    /// Run ephor and require an exit code, quoting what it said when it
    /// disagreed — a check that only said "failed" would send the reader to
    /// reproduce it by hand.
    fn expect(&self, args: &[&str], code: i32) -> Result<String, String> {
        let out = self.ephor(args)?;
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.code() != Some(code) {
            return Err(format!(
                "`ephor {}` exited {:?}, expected {code}: {}",
                args.join(" "),
                out.status.code(),
                first_line(&String::from_utf8_lossy(&out.stderr)).unwrap_or_else(|| stdout
                    .lines()
                    .next()
                    .unwrap_or("no output")
                    .to_string())
            ));
        }
        Ok(stdout)
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Walk the seams, handing each answer to `report` as it arrives
/// (§FS-010-doctor.3).
///
/// Streaming rather than returning the lot at the end is what keeps the pass
/// from being a second of silence, and it means the caller's report and its
/// progress are one thing rather than two that can disagree.
pub fn run(mut report: impl FnMut(&Check)) -> Report {
    let site = match build() {
        Ok(site) => site,
        Err(why) => {
            let check = Check {
                name: "a scratch site can be built",
                failure: Some(why),
            };
            report(&check);
            return Report {
                checks: vec![check],
            };
        }
    };

    let steps: Vec<(&'static str, fn(&Site) -> Step)> = vec![
        ("the published schemas print", schemas),
        (
            "a manifest is validated, and a broken one refused",
            manifest,
        ),
        ("a check verb is probed, run, and parked", checks),
        ("a verb's answer envelope is read back", envelope),
        ("a forge out of process answers a refresh", forge),
        (
            "a source that fails is reported, not silent",
            failing_source,
        ),
        ("a branch workspace is checked out", checkout),
        ("a trailing branch is replayed", rebase),
        (
            "an item becomes a ticket, and the ledger reads it back",
            dispatch,
        ),
        ("a local ticket store is read where it lives", ticket_store),
    ];

    let mut checks = Vec::with_capacity(steps.len());
    for (name, step) in steps {
        let check = Check {
            name,
            failure: step(&site).err(),
        };
        report(&check);
        checks.push(check);
    }
    Report { checks }
}

/// The schemas are the interface's stability surface, printable without a
/// registry to read (§FS-006-project-interface.11).
fn schemas(site: &Site) -> Step {
    for name in ["manifest", "answer", "registry", "forge"] {
        let printed = site.expect(&["schema", name], 0)?;
        serde_json::from_str::<Value>(&printed)
            .map_err(|err| format!("the {name} schema is not JSON: {err}"))?;
    }
    // An unknown name is refused rather than printing nothing.
    site.expect(&["schema", "nonesuch"], 2)?;
    Ok(())
}

/// A project that speaks places one file, and it is held to the published
/// schema (§FS-006-project-interface.2, §FS-006-project-interface.11).
fn manifest(site: &Site) -> Step {
    let project = site.project();
    site.expect(&["validate", "--manifest", &project.to_string_lossy()], 0)?;

    let broken = site.dir.join("broken");
    std::fs::create_dir_all(&broken).map_err(io)?;
    std::fs::write(
        broken.join(crate::manifest::FILE),
        // `checks` is an object of bindings; a list is the shape error a
        // schema exists to catch.
        r#"{"checks": ["not-an-object"]}"#,
    )
    .map_err(io)?;
    site.expect(&["validate", "--manifest", &broken.to_string_lossy()], 2)?;
    Ok(())
}

/// Three well-known names probed at the forest root, or the same three
/// declared in the manifest (§FS-006-project-interface.5). Exit `75` is
/// parked — not applicable now — and a verb that parks has not failed
/// (§FS-006-project-interface.3).
fn checks(site: &Site) -> Step {
    let project = site.project();
    let root = project.to_string_lossy().into_owned();
    let out = site.expect(&["check", "--root", &root], 0)?;
    if !out.contains("checked") {
        return Err(format!("the check verb ran but said nothing: {out:?}"));
    }
    // Parked, by the verb the manifest binds to a script that exits 75.
    let parked = site.expect(&["check", "--root", &root, "--verb", "style"], 0)?;
    if !parked.contains("parked") {
        return Err(format!("exit 75 was not read as parked: {parked:?}"));
    }
    // A verb asked for by name and bound nowhere is refused, rather than
    // skipped: a check nobody ran that nobody was told about is the failure.
    site.expect(&["check", "--root", &root, "--verb", "smoke"], 1)?;
    Ok(())
}

/// A summons may answer in structure, and what it says reaches the surface
/// (§FS-006-project-interface.4).
fn envelope(site: &Site) -> Step {
    let root = site.project().to_string_lossy().into_owned();
    let out = site.expect(&["check", "--root", &root], 0)?;
    if !out.contains("2 things checked") {
        return Err(format!(
            "the envelope's summary did not reach the surface: {out:?}"
        ));
    }
    Ok(())
}

/// An implementation reached as an executable on PATH, answering the
/// capability set as JSON (§FS-001-forge-interface.2).
fn forge(site: &Site) -> Step {
    site.expect(&["refresh", "scratch"], 0)?;
    let feed = site.expect(&["feed", "--project", "scratch", "--json"], 0)?;
    let items: Value = serde_json::from_str(&feed).map_err(|err| format!("feed JSON: {err}"))?;
    let rows = items.as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        return Err("the forge answered but nothing reached the feed".to_string());
    }
    // The kind and role a pull request lands with are what put it in a
    // category (§FS-003-feed-categories.1).
    let pr = rows
        .iter()
        .find(|row| row["kind"] == "pr")
        .ok_or("no pull request in the feed")?;
    if pr["role"] != "author" {
        return Err(format!("the role did not survive: {}", pr["role"]));
    }
    Ok(())
}

/// A provider that cannot deliver fails explicitly, and the run that lost it
/// exits non-zero (§FS-001-forge-interface.6). The partial loss is what is
/// walked: a refresh where one source of two answered is exactly the shape a
/// reader could mistake for "nothing is waiting".
fn failing_source(site: &Site) -> Step {
    let out = site.ephor(&["refresh", "broken"])?;
    if out.status.code() != Some(4) {
        return Err(format!(
            "a partly lost refresh exited {:?}, expected 4",
            out.status.code()
        ));
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !stderr.contains("broken") {
        return Err(format!("the lost provider was not named: {stderr}"));
    }
    // And the survivor still delivered, so an explicit failure does not cost
    // the sources that worked (§FS-001-forge-interface.6).
    let feed = site.expect(&["feed", "--project", "broken", "--json"], 0)?;
    let rows: Value = serde_json::from_str(&feed).map_err(|err| format!("feed JSON: {err}"))?;
    if rows.as_array().map(Vec::is_empty).unwrap_or(true) {
        return Err("one source failed and took the other's items with it".to_string());
    }
    Ok(())
}

/// A missing workspace is offered the checkout, and ephor supplies the
/// command (§FS-004-quick-actions.7).
fn checkout(site: &Site) -> Step {
    let workspace = site.dir.join("workspaces/doctor-branch");
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace).map_err(io)?;
    }
    site.expect(
        &[
            "checkout",
            "--branch",
            "doctor-branch",
            "--project",
            "scratch",
        ],
        0,
    )?;
    if !workspace.join(".git").exists() {
        return Err(format!(
            "{} was reported made and is not a working tree",
            workspace.display()
        ));
    }
    Ok(())
}

/// The deterministic move, run on its own: the same operation the reader's
/// key and a state machine both reach (§FS-004-quick-actions.6,
/// §FS-005-dispatch.12).
fn rebase(site: &Site) -> Step {
    let workspace = site.dir.join("workspaces/doctor-branch");
    if !workspace.is_dir() {
        return Err("no workspace to replay — the checkout step is the one to read".to_string());
    }
    let out = site.expect(
        &[
            "rebase",
            "--project",
            "scratch",
            "--checkout",
            &workspace.to_string_lossy(),
        ],
        0,
    )?;
    if !out.contains("main") {
        return Err(format!("the replay said nothing about its base: {out:?}"));
    }
    Ok(())
}

/// An item becomes a ticket carrying what ephor knew, and the work's state is
/// read back out of the plan rather than out of the ledger
/// (§FS-005-dispatch.2, §FS-005-dispatch.4).
fn dispatch(site: &Site) -> Step {
    site.expect(&["work", "dispatch", "--project", "scratch"], 0)?;
    let listed = site.expect(&["work", "list", "--json"], 0)?;
    let rows: Value =
        serde_json::from_str(&listed).map_err(|err| format!("work list JSON: {err}"))?;
    let first = rows
        .as_array()
        .and_then(|rows| rows.first())
        .ok_or("nothing was dispatched")?;
    if first["tickets"]
        .as_array()
        .map(Vec::is_empty)
        .unwrap_or(true)
    {
        return Err("the ledger points at a plan with no tickets in it".to_string());
    }
    Ok(())
}

/// A store the project keeps in its checkout is read through its own files,
/// into the same feed under the same rules (§FS-006-project-interface.7).
fn ticket_store(site: &Site) -> Step {
    // The dispatch above wrote a plan into the project's work root, which is
    // the store this reads back. Asking the seam for the directory rather than
    // spelling it keeps the store's name in its own adapter
    // (§REQ-001-boundary.5).
    let store = site.project().join(crate::work::runtime::plan::PROJECT_DIR);
    if !store.is_dir() {
        return Err("dispatch wrote no work root to read back".to_string());
    }
    site.expect(&["refresh", "scratch"], 0)?;
    let feed = site.expect(&["feed", "--project", "scratch", "--json"], 0)?;
    let rows: Value = serde_json::from_str(&feed).map_err(|err| format!("feed JSON: {err}"))?;
    let store_source = crate::seams::tickets::Kind::Plans.name();
    let found = rows
        .as_array()
        .map(|rows| {
            rows.iter()
                .any(|row| row["source"].as_str() == Some(store_source))
        })
        .unwrap_or(false);
    if !found {
        return Err(format!(
            "a plan sits in {} and no matter came back from it",
            store.display()
        ));
    }
    Ok(())
}

fn io(err: std::io::Error) -> String {
    err.to_string()
}

// ---------------------------------------------------------------------------
// The scratch site
// ---------------------------------------------------------------------------

/// A temporary directory removed when it drops. Small enough to own here:
/// `tempfile` is a dev-dependency, and the diagnosis ships in the binary.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn build() -> Result<Site, String> {
    let base = std::env::temp_dir().join(format!("ephor-doctor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).map_err(io)?;
    let guard = TempDir(base.clone());

    write_forge(&base)?;
    let main = write_repository(&base)?;
    write_manifest_and_verbs(&base)?;
    write_registry(&base, &main)?;
    write_config(&base)?;

    Ok(Site {
        dir: base,
        _guard: guard,
    })
}

/// A complete forge implementation: an executable on PATH answering a fixed
/// set of subcommands with JSON on stdout (§FS-001-forge-interface.2). A shell
/// script is a complete implementation, and this is the proof of it.
fn write_forge(base: &Path) -> Result<(), String> {
    let bin = base.join("bin");
    std::fs::create_dir_all(&bin).map_err(io)?;
    executable(
        &bin.join("ephor-forge-scratch"),
        r#"#!/bin/sh
case "$1" in
  capabilities)
    printf '{"pull_requests":true,"issues":true,"conversation":true}' ;;
  pull-requests)
    printf '[{"id":"acme/widget/1","repo":"acme/widget","number":"1",'
    printf '"title":"A change the doctor made up","url":"https://example.invalid/1",'
    printf '"branch":"doctor-branch","updated_at":"2026-01-01T00:00:00Z",'
    printf '"role":"author","state":"open","reasons":["authored"],"cited":false,'
    printf '"threads":[{"messages":[{"author":"someone","text":"why this way?",'
    printf '"when":"2026-01-01T00:00:00Z","mine":false}]}]}]' ;;
  issues|notices) printf '[]' ;;
  *) printf '[]' ;;
esac
"#,
    )?;
    // The other half of the same law: a source that cannot deliver says so
    // rather than answering empty (§FS-001-forge-interface.6).
    executable(
        &bin.join("ephor-forge-broken"),
        "#!/bin/sh\necho 'the doctor broke this one on purpose' >&2\nexit 1\n",
    )
}

/// An origin and a checkout of it, so the git operations have something real
/// to work on. Returns the main branch's name.
fn write_repository(base: &Path) -> Result<String, String> {
    let main = "main";
    let origin = base.join("origin");
    std::fs::create_dir_all(&origin).map_err(io)?;
    git(
        &origin,
        &["init", &format!("--initial-branch={main}"), "-q"],
    )?;
    git(&origin, &["config", "user.email", "doctor@localhost"])?;
    git(&origin, &["config", "user.name", "ephor doctor"])?;
    std::fs::write(origin.join("README.md"), "scratch\n").map_err(io)?;
    git(&origin, &["add", "-A"])?;
    git(&origin, &["commit", "-q", "-m", "one"])?;

    let project = base.join("project");
    git(
        base,
        &[
            "clone",
            "-q",
            &origin.to_string_lossy(),
            &project.to_string_lossy(),
        ],
    )?;
    git(&project, &["config", "user.email", "doctor@localhost"])?;
    git(&project, &["config", "user.name", "ephor doctor"])?;

    // Master moves after the clone, so a branch grown from it trails by one
    // and the replay has something to do.
    std::fs::write(origin.join("later.txt"), "later\n").map_err(io)?;
    git(&origin, &["add", "-A"])?;
    git(&origin, &["commit", "-q", "-m", "two"])?;
    git(&project, &["fetch", "-q", "origin"])?;
    Ok(main.to_string())
}

/// What the project says about itself, and the scripts it names
/// (§FS-006-project-interface.2, §FS-006-project-interface.5).
fn write_manifest_and_verbs(base: &Path) -> Result<(), String> {
    let project = base.join("project");
    // The aggregate is probed under its well-known name; the style pass is
    // declared elsewhere by the manifest, which is the other half of §5.
    let probed = crate::seams::checks::Verb::Check.probed();
    executable(
        &project.join(probed),
        "#!/bin/sh\nprintf '{\"v\":1,\"summary\":\"2 things checked\"}' > \"$EPHOR_ANSWER\"\n\
         echo 'checked'\nexit 0\n",
    )?;
    std::fs::create_dir_all(project.join("ci")).map_err(io)?;
    executable(
        &project.join("ci/style.sh"),
        "#!/bin/sh\necho 'not applicable right now'\nexit 75\n",
    )?;
    std::fs::write(
        project.join(crate::manifest::FILE),
        serde_json::to_string_pretty(&json!({
            "checks": { "style": "./ci/style.sh" }
        }))
        .unwrap(),
    )
    .map_err(io)
}

/// The registry row: where the forest is, and how a branch becomes a
/// workspace (§FS-006-project-interface.1).
fn write_registry(base: &Path, main: &str) -> Result<(), String> {
    let template = base.join("agents.md.tmpl");
    std::fs::write(&template, "# AGENTS.md\n\n{summary}\n").map_err(io)?;
    let project_type = json!({
        "id": "scratch-type",
        "layout": "monorepo",
        "repos": [{
            "id": "root",
            "path": ".",
            "role": "The repository itself",
            "required": true,
            "update_mode": "branch",
            "default_branch": main
        }],
        "agents": {
            "template": template.to_string_lossy(),
            "structure_intro": "One repository.",
            "summary_template": "A scratch project the doctor made up, on `{branch}`."
        }
    });
    // Two rows, because the walk needs a source that answers and one that
    // does not. Derived workspace ids are unique across the registry, so each
    // row names its own branch.
    let project = |id: &str, branch: &str| {
        json!({
            "id": id,
            "type": "scratch-type",
            "display_name": id,
            "root": base.join("project").to_string_lossy(),
            "main_branch": main,
            "branch_root_template": base.join("workspaces/{branch}").to_string_lossy(),
            "branches": [{ "id": format!("{id}-branch"), "branch": branch, "active": true }]
        })
    };
    std::fs::write(
        base.join("registry.json"),
        serde_json::to_string_pretty(&json!({
            "project_types": [project_type],
            "hook_sets": [],
            "projects": [
                project("scratch", "doctor-branch"),
                project("broken", "doctor-broken-branch")
            ]
        }))
        .unwrap(),
    )
    .map_err(io)
}

/// The site configuration: which implementation watches which project, and
/// where the work goes (§FS-006-project-interface.1).
fn write_config(base: &Path) -> Result<(), String> {
    std::fs::write(
        base.join("status.json"),
        serde_json::to_string_pretty(&json!({
            "defaults": { "ttl_seconds": 0, "provider_timeout_seconds": 30 },
            "work": {
                // The plan goes in the project's own checkout, which is also
                // where the ticket-store reader looks for it.
                "root": format!("{{root}}/{}", crate::work::runtime::plan::PROJECT_DIR)
            },
            "projects": {
                "scratch": { "providers": [{ "provider": "scratch" }] },
                // One that answers and one that does not: the partial loss is
                // the case worth walking, because it is the one an empty
                // section could be mistaken for
                // (§FS-001-forge-interface.6).
                "broken": { "providers": [
                    { "provider": "scratch" },
                    { "provider": "broken" }
                ] }
            }
        }))
        .unwrap(),
    )
    .map_err(io)
}

fn executable(path: &Path, body: &str) -> Result<(), String> {
    std::fs::write(path, body).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).map_err(io)?;
    }
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|err| format!("git is not usable here: {err}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            first_line(&String::from_utf8_lossy(&out.stderr))
                .unwrap_or_else(|| "no output".to_string())
        ));
    }
    Ok(())
}
