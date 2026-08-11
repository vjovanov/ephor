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

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult;
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
    let label = format!("{command:?}");
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command
        .spawn()
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
        let detail = stderr.lines().next().unwrap_or("").trim().to_string();
        return Err(ProviderError(format!(
            "{label} failed ({status}): {detail}"
        )));
    }
    Ok(stdout)
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
    let stdout = run_capture_stdin(command, stdin, timeout, allow_failure)?;
    serde_json::from_str(stdout.trim())
        .map_err(|err| ProviderError(format!("invalid JSON output: {err}")))
}

pub fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(name);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
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
