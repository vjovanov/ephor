//! §FS-001-forge-interface.2, out-of-process transport: a forge implemented as
//! an executable rather than as Rust.
//!
//! The executable is named for the forge — `ephor-forge-<name>`, resolved on
//! PATH, following the convention `git`, `gh`, and `kubectl` use — or named
//! outright with `"command"`. ephor runs it once per capability:
//!
//! ```text
//!     ephor-forge-<name> capabilities   <<< '{"config":…,"project":…}'
//!     ephor-forge-<name> pull-requests  <<< '{"config":…,"tickets":[…],…}'
//!     ephor-forge-<name> issues         <<< '{"config":…,"tickets":[…],…}'
//!     ephor-forge-<name> failures       <<< '{"config":…,"repo":…,"number":…}'
//!     ephor-forge-<name> react          <<< '{"config":…,"target":…,"emoji":…}'
//! ```
//!
//! The [`Request`] goes in on stdin, the answer comes back as JSON on stdout,
//! and stderr is the implementation's own diagnostics. Calls are coarse on
//! purpose: `pull-requests` returns conversation and gate inline, so a refresh
//! costs two spawns rather than one per pull request. `failures` is the one
//! call a refresh never makes — it is asked when a reader opens a red gate, so
//! it may take as long as the forge needs.

use std::process::Command;

use serde_json::{json, Value};

use super::{Capabilities, Forge, Issue, PullRequest, Request};
use crate::feed::gate::Failure;
use crate::feed::provider::{command_exists, run_json_stdin, ProviderError};

pub struct ExternalForge {
    name: String,
    command: String,
}

impl ExternalForge {
    /// `command` overrides the `ephor-forge-<name>` PATH convention.
    pub fn new(name: impl Into<String>, command: Option<String>) -> Self {
        let name = name.into();
        let command = command.unwrap_or_else(|| format!("ephor-forge-{name}"));
        ExternalForge { name, command }
    }

    fn call(
        &self,
        subcommand: &str,
        request: &Request,
        extra: Value,
    ) -> Result<Value, ProviderError> {
        let mut payload = serde_json::to_value(request).unwrap_or_else(|_| json!({}));
        if let (Some(target), Some(source)) = (payload.as_object_mut(), extra.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }

        let mut command = Command::new(&self.command);
        command.arg(subcommand);
        run_json_stdin(
            command,
            Some(serde_json::to_string(&payload).unwrap_or_default()),
            std::time::Duration::from_secs(request.timeout_seconds),
            false,
        )
        .map_err(|err| ProviderError(format!("{} {subcommand}: {err}", self.command)))
    }

    fn decode<T: serde::de::DeserializeOwned>(
        &self,
        subcommand: &str,
        value: Value,
    ) -> Result<T, ProviderError> {
        serde_json::from_value(value).map_err(|err| {
            ProviderError(format!(
                "{} {subcommand}: output does not match the forge interface: {err}",
                self.command
            ))
        })
    }
}

impl Forge for ExternalForge {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn available(&self) -> bool {
        command_exists(&self.command)
    }

    /// An out-of-process forge is missing exactly one thing, and naming it is
    /// the difference between an install command and an afternoon: the
    /// configuration names a forge, so the executable is inferred rather than
    /// written down anywhere the reader can check.
    fn unavailable_reason(&self) -> Option<String> {
        Some(format!("`{}` is not on PATH", self.command))
    }

    /// The probe is a real process launch, so it fails for every reason a
    /// fetch does — the executable crashed, the VPN is down, the host refused
    /// the connection. Each of those is reported as itself; answering
    /// "declared nothing" instead would describe a working extension behind an
    /// unreachable host as a broken one.
    fn capabilities(&self) -> Result<Capabilities, ProviderError> {
        let probe = Request {
            config: Value::Null,
            project: String::new(),
            tickets: Vec::new(),
            user: None,
            timeout_seconds: 15,
        };
        let value = self.call("capabilities", &probe, json!({}))?;
        self.decode("capabilities", value)
    }

    fn pull_requests(&self, request: &Request) -> Result<Vec<PullRequest>, ProviderError> {
        let value = self.call("pull-requests", request, json!({}))?;
        self.decode("pull-requests", value)
    }

    fn issues(&self, request: &Request) -> Result<Vec<Issue>, ProviderError> {
        let value = self.call("issues", request, json!({}))?;
        self.decode("issues", value)
    }

    fn failures(
        &self,
        request: &Request,
        repo: &str,
        number: &str,
    ) -> Result<Vec<Failure>, ProviderError> {
        let value = self.call(
            "failures",
            request,
            json!({ "repo": repo, "number": number }),
        )?;
        self.decode("failures", value)
    }

    fn react(&self, request: &Request, target: &Value, emoji: &str) -> Result<(), ProviderError> {
        self.call(
            "react",
            request,
            json!({ "target": target, "emoji": emoji }),
        )?;
        Ok(())
    }
}
