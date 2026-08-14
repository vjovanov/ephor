//! The one executor: `Summons { verb, binding, place, dossier } → Answer
//! { exit_code, answer, output }` (§AR-002-summons).
//!
//! Every command the project interface names — a check verb, a gate verb, a
//! checkout, a ticket store's CLI, an offer, the runtime's run — is invoked
//! here and nowhere else (§FS-006-project-interface.3). One executor means one
//! refusal path, one environment contract, and one answer reader, and that the
//! same operation invoked from a menu key and from a state machine cannot drift
//! apart (§FS-005-dispatch.12).
//!
//! What crosses is environment, an exit code, and an optional answer file — the
//! seam's contract in materials, so the other side can always be a shell script
//! (§REQ-001-boundary.1).

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::error::{EphorError, Result};
use crate::seams::answer::{self, Normalized};

/// The exit code that means *parked*: not applicable now, ask again later
/// (§FS-006-project-interface.3). It is `EX_TEMPFAIL`, which is what a shell
/// script reaching for a "come back later" code already has.
pub const PARKED: i32 = 75;

/// The environment variable naming the file an answer may be written to.
pub const ANSWER_VAR: &str = "EPHOR_ANSWER";

/// Start a command, waiting out the one failure that is never the command's
/// fault: a binary still open for writing somewhere reports `ETXTBSY` at exec
/// (a script being rewritten as ephor reaches for it, a fork in another thread
/// holding the descriptor). It clears in microseconds, so a bounded wait is
/// the honest answer where refusing would report the world's race as the
/// project's error.
pub fn spawn(command: &mut Command) -> std::io::Result<std::process::Child> {
    const BUSY: i32 = 26;
    for attempt in 0..BUSY_RETRIES {
        match command.spawn() {
            Err(err) if err.raw_os_error() == Some(BUSY) && attempt + 1 < BUSY_RETRIES => {
                std::thread::sleep(Duration::from_millis(5));
            }
            other => return other,
        }
    }
    unreachable!("the loop returns on its last attempt")
}

const BUSY_RETRIES: u32 = 20;

/// A value as one `sh` word, for building a binding out of parts. Single
/// quotes take everything literally, so only the single quote itself has to be
/// broken out.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Where a summons runs (§AR-002-summons.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The matter's branch workspace where one resolves, the forest root
    /// otherwise. What a command asks for when it says nothing.
    Workspace,
    /// The forest root, whatever the matter's branch is.
    Root,
    /// One repository of the forest, named as `repo:<name>` by the binding.
    Repo(String),
}

impl Place {
    /// Read a binding's `cwd` field. Anything else is a configuration error,
    /// reported as one rather than run somewhere surprising.
    pub fn parse(spec: &str) -> Result<Place> {
        match spec.trim() {
            "workspace" => Ok(Place::Workspace),
            "root" => Ok(Place::Root),
            other => match other.strip_prefix("repo:") {
                Some(name) if !name.is_empty() => Ok(Place::Repo(name.to_string())),
                _ => Err(EphorError::Registry(format!(
                    "unknown place '{other}': expected 'workspace', 'root', or 'repo:<name>'"
                ))),
            },
        }
    }
}

/// The positions a summons can be placed at: one project's forest root, and the
/// branch workspace where the matter resolves to one.
#[derive(Debug, Clone)]
pub struct Site {
    pub root: PathBuf,
    pub workspace: Option<PathBuf>,
}

impl Site {
    /// A project with no branch workspace resolved — the root is the checkout.
    pub fn root(root: impl Into<PathBuf>) -> Self {
        Site {
            root: root.into(),
            workspace: None,
        }
    }

    /// A matter whose branch workspace is on disk.
    pub fn workspace(root: impl Into<PathBuf>, workspace: impl Into<PathBuf>) -> Self {
        Site {
            root: root.into(),
            workspace: Some(workspace.into()),
        }
    }

    /// The directory a summons runs in. A place that does not exist is a
    /// refusal naming it, never a command run somewhere surprising
    /// (§AR-002-summons.1).
    pub fn resolve(&self, place: &Place) -> Result<PathBuf> {
        let base = || self.workspace.clone().unwrap_or_else(|| self.root.clone());
        let resolved = match place {
            Place::Workspace => base(),
            Place::Root => self.root.clone(),
            Place::Repo(name) => base().join(name),
        };
        if !resolved.is_dir() {
            return Err(EphorError::Command(format!(
                "{} is not there",
                resolved.display()
            )));
        }
        Ok(resolved)
    }
}

/// What ephor asks of the world, once.
#[derive(Debug, Clone)]
pub struct Summons {
    /// The verb being filled, for messages: `check`, `gate.status`, an offer's
    /// id. It names the seam, not the command.
    pub verb: String,
    /// The command filling it, resolved from site configuration, the manifest,
    /// or a probe (§FS-006-project-interface.1) — the seam's configured
    /// binding (§REQ-001-boundary.1).
    pub binding: String,
    pub place: Place,
    /// The matter as `EPHOR_*` pairs — one vocabulary, identical to what a
    /// shell action and a state-machine script receive (§FS-005-dispatch.8).
    pub dossier: Vec<(String, String)>,
}

impl Summons {
    /// A summons in the default place, carrying nothing but its command.
    pub fn new(verb: impl Into<String>, binding: impl Into<String>) -> Self {
        Summons {
            verb: verb.into(),
            binding: binding.into(),
            place: Place::Workspace,
            dossier: Vec::new(),
        }
    }

    pub fn at(mut self, place: Place) -> Self {
        self.place = place;
        self
    }

    pub fn carrying(mut self, dossier: Vec<(String, String)>) -> Self {
        self.dossier = dossier;
        self
    }
}

/// Whether the person is watching. Which one applies is a property of the call
/// site, not of the binding (§AR-002-summons.2): a menu action and the runtime
/// hand over the terminal, a verb called during refresh is captured.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// The command inherits the terminal — its output is the person's.
    Interactive,
    /// Output is captured and the command is given this long to finish.
    Captured(Duration),
}

/// How a summons ended, by its exit code alone (§FS-006-project-interface.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Failed,
    /// Not applicable now, ask again later.
    Parked,
}

/// What came back.
#[derive(Debug, Clone)]
pub struct Answer {
    pub outcome: Outcome,
    /// None where a signal killed the command, which is a failure with no code
    /// to report.
    pub exit_code: Option<i32>,
    /// The envelope the command wrote, already validated and normalized. None
    /// is a complete answer: the exit code stands alone (§AR-002-summons.3).
    pub answer: Option<Normalized>,
    /// Captured standard output, present only for [`Mode::Captured`]. Structure
    /// is never parsed out of it — a contract that read stdout would make every
    /// honest build log a protocol violation.
    pub output: Option<String>,
    /// Where the command ran, for messages about what it did.
    pub place: PathBuf,
}

impl Answer {
    pub fn is_done(&self) -> bool {
        self.outcome == Outcome::Done
    }

    /// The one line worth showing when a summons did not succeed.
    pub fn refusal(&self, verb: &str) -> String {
        match self.outcome {
            Outcome::Done => format!("{verb}: ok"),
            Outcome::Parked => format!("{verb}: parked"),
            Outcome::Failed => match self.exit_code {
                Some(code) => format!("{verb}: failed ({code})"),
                None => format!("{verb}: killed"),
            },
        }
    }
}

/// Run one summons.
///
/// The command runs via `sh -c` in the resolved place with the dossier exported
/// as `EPHOR_*` and a fresh `$EPHOR_ANSWER` named for it. Its exit code is read
/// as `0` done, `75` parked, anything else failed; where it wrote an answer,
/// that answer is validated against the published envelope and normalized
/// before it is handed back, and the file is discarded either way.
pub fn run(summons: &Summons, site: &Site, mode: Mode) -> Result<Answer> {
    // The two things a summons leans on are re-checked here, at invocation,
    // however recently a capability table answered about them: a table speaks
    // for the moment it was resolved, and a directory deleted since then has
    // to fail as the world rather than as a lie (§AR-005-capabilities.3).
    let place = site
        .resolve(&summons.place)
        .map_err(|err| EphorError::Command(format!("{}: cannot run — {err}", summons.verb)))?;
    if let Some(missing) = missing_binding(&summons.binding, &place) {
        return Err(EphorError::Command(format!(
            "{}: cannot run — {} is not there",
            summons.verb,
            missing.display()
        )));
    }
    let answer_file = AnswerFile::reserve()?;

    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(&summons.binding)
        .current_dir(&place)
        .envs(summons.dossier.iter().cloned())
        // The command is about to redirect into this, so it is spelled for the
        // shell doing the redirecting (§FS-006-project-interface.3). ephor
        // reads the file back through its own path, which names the same file.
        .env(ANSWER_VAR, crate::paths::for_shell(&answer_file.path));

    let (status, output) = match mode {
        Mode::Interactive => (interactive(command, &summons.verb)?, None),
        Mode::Captured(timeout) => {
            let (status, stdout) = captured(command, &summons.verb, timeout)?;
            (status, Some(stdout))
        }
    };

    let exit_code = status.code();
    let outcome = match exit_code {
        Some(0) => Outcome::Done,
        Some(PARKED) => Outcome::Parked,
        _ => Outcome::Failed,
    };
    let answer = match answer_file.read()? {
        Some(text) => Some(answer::parse(&text, &summons.verb, &place)?),
        None => None,
    };

    Ok(Answer {
        outcome,
        exit_code,
        answer,
        output,
        place,
    })
}

/// The path a binding names, where it names one and it is gone. Only a
/// binding that *is* a path is checked: `gh pr list` is the world's business
/// and `./check.sh` is ephor's, and the difference is visible in the word.
fn missing_binding(binding: &str, place: &std::path::Path) -> Option<PathBuf> {
    let word = binding
        .split_whitespace()
        .next()?
        .trim_matches(|character| character == '\'' || character == '"');
    // What counts as absolute is the platform's answer, not a leading slash:
    // a binding named `C:\tools\check.bat` is as much a path as `/usr/bin/check`
    // is, and testing for the slash alone left it looking like a bare command —
    // so a script that had been deleted was handed to the shell to fail as
    // "command not found" rather than refused here by name.
    let candidate = std::path::Path::new(word);
    let absolute = candidate.is_absolute();
    if !absolute && !word.starts_with("./") && !word.starts_with("../") {
        return None;
    }
    let path = if absolute {
        PathBuf::from(word)
    } else {
        place.join(word)
    };
    (!path.exists()).then_some(path)
}

/// Hand over the terminal and wait. No timeout: the person is at the keyboard,
/// and a command they are watching is theirs to stop.
fn interactive(mut command: Command, verb: &str) -> Result<std::process::ExitStatus> {
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|err| EphorError::Command(format!("{verb}: failed to run: {err}")))
}

/// Capture output under a timeout. Both pipes are drained on their own threads:
/// waiting for exit before reading deadlocks the moment a child fills a pipe
/// buffer, which a build log does immediately.
fn captured(
    mut command: Command,
    verb: &str,
    timeout: Duration,
) -> Result<(std::process::ExitStatus, String)> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = spawn(&mut command)
        .map_err(|err| EphorError::Command(format!("{verb}: failed to run: {err}")))?;

    let drain = |stream: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buffer = String::new();
            if let Some(mut stream) = stream {
                let _ = stream.read_to_string(&mut buffer);
            }
            buffer
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|stream| Box::new(stream) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|stream| Box::new(stream) as Box<dyn Read + Send>),
    );

    let status = child
        .wait_timeout(timeout)
        .map_err(|err| EphorError::Command(format!("{verb}: failed waiting: {err}")))?;
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        // The reader threads finish once the pipes close on the child's death.
        let _ = stdout.join();
        let _ = stderr.join();
        return Err(EphorError::Command(format!(
            "{verb}: timed out after {}s",
            timeout.as_secs()
        )));
    };
    let captured = stdout.join().unwrap_or_default();
    // The command's own standard error stays the command's; nothing here parses
    // it, and an unwatched summons has it in the log where it was streamed.
    let _ = stderr.join();
    Ok((status, captured))
}

/// The file a command may write its answer to. It is named, never created: an
/// absent file is what "no structured answer" looks like, and creating one
/// would make silence indistinguishable from an empty answer. The directory
/// holding it is fresh, so the name cannot collide with a concurrent summons,
/// and both go away when the run is over.
struct AnswerFile {
    dir: PathBuf,
    path: PathBuf,
}

impl AnswerFile {
    fn reserve() -> Result<AnswerFile> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let base = std::env::temp_dir();
        let pid = std::process::id();
        for _ in 0..64 {
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir = base.join(format!("ephor-answer-{pid}-{serial}"));
            // create_dir fails when the name is taken, which is the lock.
            match std::fs::create_dir(&dir) {
                Ok(()) => {
                    let path = dir.join("answer.json");
                    return Ok(AnswerFile { dir, path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(EphorError::Command(format!(
                        "cannot make a place for the answer in {}: {err}",
                        base.display()
                    )))
                }
            }
        }
        Err(EphorError::Command(format!(
            "cannot make a place for the answer in {}",
            base.display()
        )))
    }

    fn read(&self) -> Result<Option<String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) if text.trim().is_empty() => Ok(None),
            Ok(text) => Ok(Some(text)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(EphorError::Command(format!(
                "cannot read the answer at {}: {err}",
                self.path.display()
            ))),
        }
    }
}

impl Drop for AnswerFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SECOND: Duration = Duration::from_secs(10);

    fn stub(dir: &Path, script: &str) -> String {
        let path = dir.join("stub.sh");
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        format!("{} ", path.display())
    }

    fn site(dir: &Path) -> Site {
        Site::root(dir)
    }

    #[test]
    fn a_place_is_read_from_the_bindings_own_words() {
        assert_eq!(Place::parse("workspace").unwrap(), Place::Workspace);
        assert_eq!(Place::parse("root").unwrap(), Place::Root);
        assert_eq!(
            Place::parse("repo:compiler").unwrap(),
            Place::Repo("compiler".to_string())
        );
        assert!(Place::parse("somewhere").is_err());
        assert!(Place::parse("repo:").is_err());
    }

    #[test]
    fn the_workspace_is_the_place_where_one_resolves_and_the_root_otherwise() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace = tmp.path().join("branch");
        std::fs::create_dir(&workspace).unwrap();

        let with = Site::workspace(tmp.path(), &workspace);
        assert_eq!(with.resolve(&Place::Workspace).unwrap(), workspace);
        assert_eq!(with.resolve(&Place::Root).unwrap(), tmp.path());

        let without = Site::root(tmp.path());
        assert_eq!(without.resolve(&Place::Workspace).unwrap(), tmp.path());
    }

    #[test]
    fn a_repo_place_resolves_inside_the_checkout() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("ce");
        std::fs::create_dir(&repo).unwrap();
        let site = Site::root(tmp.path());
        assert_eq!(site.resolve(&Place::Repo("ce".to_string())).unwrap(), repo);
    }

    #[test]
    fn a_place_that_is_not_there_refuses_with_the_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("check", "true").at(Place::Repo("missing".to_string()));
        let err = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("check: cannot run"), "{message}");
        assert!(message.contains("missing"), "{message}");
    }

    #[test]
    fn exit_zero_is_done_and_stdout_comes_back_captured() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("check", "echo 'built 3 things'");
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert_eq!(answer.outcome, Outcome::Done);
        assert_eq!(answer.exit_code, Some(0));
        assert_eq!(answer.output.as_deref(), Some("built 3 things\n"));
        assert!(answer.answer.is_none());
    }

    #[test]
    fn a_nonzero_exit_is_a_failure_carrying_its_code() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("check", "exit 4");
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert_eq!(answer.outcome, Outcome::Failed);
        assert_eq!(answer.exit_code, Some(4));
        assert_eq!(answer.refusal("check"), "check: failed (4)");
    }

    #[test]
    fn seventy_five_is_parked_not_failed() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("gate.status", "exit 75");
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert_eq!(answer.outcome, Outcome::Parked);
        assert_eq!(answer.refusal("gate.status"), "gate.status: parked");
    }

    #[test]
    fn the_dossier_arrives_as_the_one_environment_vocabulary() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new(
            "action",
            "printf '%s/%s' \"$EPHOR_PROJECT\" \"$EPHOR_TICKET\"",
        )
        .carrying(vec![
            ("EPHOR_PROJECT".to_string(), "widget".to_string()),
            ("EPHOR_TICKET".to_string(), "GR-73955".to_string()),
        ]);
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert_eq!(answer.output.as_deref(), Some("widget/GR-73955"));
    }

    #[test]
    fn the_command_runs_in_the_place_and_not_wherever_ephor_stands() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("ce");
        std::fs::create_dir(&repo).unwrap();
        let summons = Summons::new("check", "pwd").at(Place::Repo("ce".to_string()));
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        let printed = answer.output.unwrap();
        // macOS resolves the temp root through a symlink; compare the tail.
        assert!(printed.trim().ends_with("ce"), "{printed}");
    }

    #[test]
    fn an_answer_written_to_the_named_file_comes_back_normalized() {
        let tmp = tempfile::tempdir().unwrap();
        let script = stub(
            tmp.path(),
            "#!/bin/sh\ncat > \"$EPHOR_ANSWER\" <<'JSON'\n\
             {\"v\":1,\"summary\":\"2 jobs failed\",\
             \"failures\":[{\"job\":\"style\"},{\"job\":\"build\"}]}\nJSON\nexit 1\n",
        );
        let summons = Summons::new("gate.failures", script);
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert_eq!(answer.outcome, Outcome::Failed);
        let envelope = answer.answer.expect("the command wrote an answer");
        assert_eq!(envelope.facts.summary.as_deref(), Some("2 jobs failed"));
        assert_eq!(envelope.events[0].failures.len(), 2);
    }

    #[test]
    fn no_answer_file_is_a_complete_answer() {
        let tmp = tempfile::tempdir().unwrap();
        let answer = run(
            &Summons::new("check", "true"),
            &site(tmp.path()),
            Mode::Captured(SECOND),
        )
        .unwrap();
        assert!(answer.answer.is_none());
        assert!(answer.is_done());
    }

    #[test]
    fn an_empty_answer_file_is_silence_too() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("check", ": > \"$EPHOR_ANSWER\"");
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert!(answer.answer.is_none());
    }

    #[test]
    fn an_answer_that_breaks_the_envelope_is_reported_not_half_read() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new(
            "check",
            "echo '{\"v\":1,\"features\":[{}]}' > \"$EPHOR_ANSWER\"",
        );
        let err = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap_err();
        assert!(err.to_string().contains("envelope schema"), "{err}");
    }

    #[test]
    fn the_answer_file_is_fresh_and_discarded_afterwards() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new(
            "check",
            "test ! -e \"$EPHOR_ANSWER\" && printf '%s' \"$EPHOR_ANSWER\"",
        );
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        let named = answer.output.expect("captured").trim().to_string();
        assert!(
            !named.is_empty(),
            "the executor names a file for the command"
        );
        assert!(!Path::new(&named).exists(), "{named} outlived its summons");
    }

    #[test]
    fn a_binding_that_named_a_script_and_lost_it_refuses_at_invocation() {
        let tmp = tempfile::tempdir().unwrap();
        let script = tmp.path().join("check.sh");
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        let summons = Summons::new("check", format!("{} --fast", script.display()));
        // It was there a moment ago, and the table would still say so.
        std::fs::remove_file(&script).unwrap();
        let err = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("check: cannot run"), "{message}");
        assert!(message.contains("check.sh is not there"), "{message}");
    }

    #[test]
    fn a_binding_that_is_a_command_rather_than_a_path_is_left_to_the_world() {
        let tmp = tempfile::tempdir().unwrap();
        // Not ephor's business whether `no-such-tool` exists: that is the
        // command's own failure to report, in its own words.
        let summons = Summons::new("check", "no-such-tool-anywhere --please");
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert_eq!(answer.outcome, Outcome::Failed);
    }

    #[test]
    fn stdout_is_never_read_for_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("check", "echo '{\"v\":1,\"summary\":\"not an answer\"}'");
        let answer = run(&summons, &site(tmp.path()), Mode::Captured(SECOND)).unwrap();
        assert!(answer.answer.is_none());
        assert!(answer.output.unwrap().contains("not an answer"));
    }

    #[test]
    fn a_command_that_never_finishes_is_stopped_and_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("check", "sleep 30");
        let err = run(
            &summons,
            &site(tmp.path()),
            Mode::Captured(Duration::from_millis(150)),
        )
        .unwrap_err();
        assert!(err.to_string().contains("timed out"), "{err}");
    }

    #[test]
    fn an_interactive_summons_answers_by_its_code_and_captures_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let summons = Summons::new("action", "exit 75");
        let answer = run(&summons, &site(tmp.path()), Mode::Interactive).unwrap();
        assert_eq!(answer.outcome, Outcome::Parked);
        assert!(answer.output.is_none());
    }
}
