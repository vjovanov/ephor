//! The provider contract and shared subprocess helpers.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use wait_timeout::ChildExt;

use crate::feed::model::Item;

#[derive(Debug)]
pub struct ProviderError(pub String);

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub type ProviderResult = Result<Vec<Item>, ProviderError>;

/// Everything a provider may need about the project it is fetching for.
#[derive(Clone)]
pub struct ProviderContext {
    pub project_id: String,
    pub project_root: PathBuf,
    #[allow(dead_code)] // for providers that filter by branch
    pub main_branch: String,
    /// Ticket keys harvested from the registry's active branch entries.
    pub tickets: Vec<String>,
    pub github_user: Option<String>,
    pub timeout: Duration,
    pub secrets_dir: PathBuf,
}

/// A source of feed items. Implementations live in `providers/`; adding a new
/// provider is one module plus a match arm in `providers::build_provider`.
pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Cheap gate: required tool on PATH, secret file present, ...
    fn available(&self, _ctx: &ProviderContext) -> bool {
        true
    }

    /// What `available` looked for and did not find. A provider that can name
    /// it should: "missing tool or secret" sends the reader hunting for a
    /// credential when the answer is usually an executable that was never
    /// installed.
    fn unavailable_reason(&self) -> Option<String> {
        None
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult;

    /// Menu entries this provider offers on an item it produced, without the
    /// reader configuring anything (§FS-004-quick-actions.1). Here — and only
    /// here — a provider may name the tool its forge is reached with; ephor
    /// merges what comes back into the action menu and runs it like any
    /// configured action. Offering none is a complete answer, which is why
    /// this defaults to empty (§FS-004-quick-actions.2).
    fn quick_actions(&self, _item: &Item) -> Vec<crate::feed::config::ActionConfig> {
        Vec::new()
    }

    /// What failed under this item's red gate (§FS-001-forge-interface.1).
    /// Asked on demand, never during a refresh, and only of the source that
    /// produced the item. A provider that hands the reader a log instead — by
    /// offering a quick action that pages one — answers nothing here and is
    /// complete, which is why this defaults to an error rather than to empty:
    /// an empty list means "the gate is red and nothing failed", which is a
    /// real answer for a gate blocked on an approval.
    fn failures(
        &self,
        _ctx: &ProviderContext,
        _item: &Item,
    ) -> Result<Vec<crate::feed::gate::Failure>, ProviderError> {
        Err(ProviderError(format!(
            "{} does not report what failed",
            self.name()
        )))
    }

    /// Run this item's gate again, at the scope asked for
    /// (§FS-004-quick-actions.9). Asked only when a reader asks, like
    /// [`Provider::failures`] — and unlike it, this one *writes*: it spends
    /// somebody else's machines, so the scope crosses verbatim and a provider
    /// that cannot re-run a check answers here rather than guessing.
    ///
    /// A provider that restarts by handing the reader a command instead — a
    /// quick action naming its forge's own CLI — answers nothing here and is
    /// complete, which is why this defaults to an error.
    fn restart(
        &self,
        _ctx: &ProviderContext,
        _item: &Item,
        _scope: crate::feed::gate::Scope,
    ) -> Result<crate::forge::Restarted, ProviderError> {
        Err(ProviderError(format!(
            "{} does not restart a gate",
            self.name()
        )))
    }
}

/// Run a command with a timeout, capturing stdout. Non-zero exits are
/// reported as errors unless `allow_failure` is set (some tools, like
/// `gh pr checks`, signal domain state through their exit code).
pub fn run_capture(
    command: Command,
    timeout: Duration,
    allow_failure: bool,
) -> Result<String, ProviderError> {
    run_capture_stdin(command, None, timeout, allow_failure)
}

/// [`run_capture`], first writing `stdin` to the child — how an out-of-process
/// forge receives its request (§FS-001-forge-interface.2). The write runs on
/// its own thread: a child that emits output before draining its input would
/// otherwise deadlock against our write.
pub fn run_capture_stdin(
    mut command: Command,
    stdin: Option<String>,
    timeout: Duration,
    allow_failure: bool,
) -> Result<String, ProviderError> {
    let label = command_label(&command);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = crate::seams::summons::spawn(&mut command)
        .map_err(|err| ProviderError(format!("failed to spawn {label}: {err}")))?;

    // Dropping the handle closes the pipe, which is the child's EOF.
    let writer = stdin.zip(child.stdin.take()).map(|(text, mut pipe)| {
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = pipe.write_all(text.as_bytes());
        })
    });

    // Drain both pipes concurrently: waiting for exit before reading would
    // deadlock once the child fills the pipe buffer (large JSON payloads).
    let drain = |stream: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buffer = String::new();
            if let Some(mut stream) = stream {
                let _ = stream.read_to_string(&mut buffer);
            }
            buffer
        })
    };
    let stdout_thread = drain(
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );
    let stderr_thread = drain(
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>),
    );

    let status = match child
        .wait_timeout(timeout)
        .map_err(|err| ProviderError(format!("failed waiting for {label}: {err}")))?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            // The reader threads finish once the pipes close on child death.
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(ProviderError(format!(
                "timed out after {}s: {label}",
                timeout.as_secs()
            )));
        }
    };

    if let Some(writer) = writer {
        let _ = writer.join();
    }
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    if !status.success() && !allow_failure {
        return Err(ProviderError(format!(
            "{label} failed ({status}): {}",
            diagnosis(&stderr)
        )));
    }
    Ok(stdout)
}

/// Words a line uses when it is reporting what went wrong rather than what is
/// being attempted. Deliberately generic: ephor names no vendor CLI
/// (§FS-001-forge-interface.5), and these are what tools in general print.
const DIAGNOSIS: &[&str] = &[
    "error",
    "fatal",
    "exception",
    "failed",
    "failure",
    "panic",
    "refused",
    "denied",
    "cannot",
    "unable to",
    "❌",
];

/// The one line of a failed command's stderr worth quoting
/// (§FS-001-forge-interface.6).
///
/// Tools narrate their progress on the same stream they fail on, and the
/// narration comes first — so quoting stderr's first line by position reports
/// "Requesting RCA for pull request …" and drops the "No builds found" under
/// it, which is the whole of what the reader needed. Scan for a line that
/// reads as a diagnosis instead.
///
/// With no such line, the first non-empty one is still the best guess: a stack
/// trace leads with its exception and the frames below it say less.
pub fn diagnosis(stderr: &str) -> String {
    let lines: Vec<String> = stderr
        .lines()
        .map(|line| strip_ansi(line).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            DIAGNOSIS.iter().any(|word| lower.contains(word))
        })
        .or_else(|| lines.first())
        .cloned()
        .unwrap_or_default()
}

/// Drop CSI escape sequences. A tool that colours its own error line would
/// otherwise have the colour codes quoted back into ephor's message, where
/// they are noise at best and a wrongly-coloured terminal at worst.
fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        // `ESC [ <params> <final>`, where the final byte is `@`..`~`.
        if chars.next() != Some('[') {
            continue;
        }
        for ch in chars.by_ref() {
            if ('@'..='~').contains(&ch) {
                break;
            }
        }
    }
    out
}

pub fn run_json(
    command: Command,
    timeout: Duration,
    allow_failure: bool,
) -> Result<Value, ProviderError> {
    run_json_stdin(command, None, timeout, allow_failure)
}

pub fn run_json_stdin(
    command: Command,
    stdin: Option<String>,
    timeout: Duration,
    allow_failure: bool,
) -> Result<Value, ProviderError> {
    let label = command_label(&command);
    let stdout = run_capture_stdin(command, stdin, timeout, allow_failure)?;
    serde_json::from_str(stdout.trim())
        .map_err(|err| ProviderError(format!("{label}: invalid JSON output: {err}")))
}

/// A command as a reader would say it — `ephor-forge-acme failures`, not
/// `"ephor-forge-acme" "failures"`. Every message out of here already names
/// the command, so a caller that names it again says it twice.
fn command_label(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|part| part.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                if dir.join(name).is_file() {
                    return true;
                }
                // Where the extension is implied at invocation, a bare name is
                // not what is on disk: `sh` on PATH is `sh.exe`, and looking
                // only for the bare name found nothing, ever. Every rung that
                // asks "is this runner installed" then answered no on a machine
                // where the runner was sitting right there.
                implied_extensions()
                    .iter()
                    .any(|extension| dir.join(format!("{name}{extension}")).is_file())
            })
        })
        .unwrap_or(false)
}

/// The extensions a bare command name may be wearing on disk. Empty where a
/// name is a name — the executable bit is what decides on Unix.
fn implied_extensions() -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read a secret JSON file from the ephor secrets directory
/// (`~/config/secrets/ephor/<name>.json`).
#[allow(dead_code)] // for the slack/discord/email providers once implemented
pub fn load_secret(ctx: &ProviderContext, name: &str) -> Result<Value, ProviderError> {
    let path = ctx.secrets_dir.join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .map_err(|err| ProviderError(format!("cannot read secret {}: {err}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|err| ProviderError(format!("invalid secret {}: {err}", path.display())))
}

pub fn secret_exists(ctx: &ProviderContext, name: &str) -> bool {
    ctx.secrets_dir.join(format!("{name}.json")).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The narration a tool writes before it fails is not the failure
    /// (§FS-001-forge-interface.6).
    #[test]
    fn the_line_quoted_is_the_one_that_says_what_went_wrong() {
        let narrated = "Requesting RCA for pull request [ACME/widget@PR#24898]...\n\
                        Waiting for RCA result for pull request [ACME/widget@PR#24898]...\n\
                        \n\
                        \u{1b}[31m❌  error: RCA failed: No builds found for the pull request\u{1b}[39m\u{1b}[0m\n\
                        \n\
                        \u{1b}[38;5;215m❔  hints: Verify build numbers and retry.\u{1b}[39m\u{1b}[0m\n";
        assert_eq!(
            diagnosis(narrated),
            "❌  error: RCA failed: No builds found for the pull request"
        );

        // A stack trace leads with its cause, and the frames under it say
        // less — so the first line stays the right answer where it is one.
        let trace = "java.net.UnknownHostException: bitbucket.example.com\n\
                     \tat java.base/java.net.Socket.connect(Socket.java:633)\n";
        assert!(diagnosis(trace).starts_with("java.net.UnknownHostException"));
        assert!(crate::feed::reachability::is_unreachable(&diagnosis(trace)));

        // Nothing that reads as a diagnosis: the first non-empty line, which
        // is what this always used to report.
        assert_eq!(
            diagnosis("\n\n  usage: tool <cmd>\nsee --help\n"),
            "usage: tool <cmd>"
        );
        assert_eq!(diagnosis(""), "");
    }

    /// A tool that fails quietly on stderr but narrates on it first used to
    /// have its narration quoted all the way up into the reader's message.
    #[test]
    fn a_failed_command_is_named_once_and_quotes_its_diagnosis() {
        let mut command = Command::new("sh");
        command.arg("-c").arg(
            "echo 'Fetching gate builds for widget/1...' >&2; \
             echo 'error: no such pull request' >&2; exit 3",
        );
        let err = run_capture_stdin(command, None, Duration::from_secs(30), false)
            .expect_err("exit 3 is a failure");
        // Named as it would be typed, once — not `"sh" "-c" "…"`.
        assert!(err.0.starts_with("sh -c "), "{}", err.0);
        let quoted = err
            .0
            .rsplit_once("): ")
            .expect("a detail follows the status")
            .1;
        assert_eq!(quoted, "error: no such pull request");
    }

    /// A non-zero exit is a failure; `allow_failure` is for the tools that
    /// signal domain state through their exit code.
    #[test]
    fn a_tolerated_failure_still_returns_its_output() {
        let mut command = Command::new("sh");
        command.arg("-c").arg("echo out; exit 1");
        let out = run_capture_stdin(command, None, Duration::from_secs(30), true)
            .expect("allow_failure keeps the output");
        assert_eq!(out.trim(), "out");
    }
}
