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
//!     ephor-forge-<name> react          <<< '{"config":…,"target":…,"emoji":…}'
//! ```
//!
//! The [`Request`] goes in on stdin, the answer comes back as JSON on stdout,
//! and stderr is the implementation's own diagnostics. Calls are coarse on
//! purpose: `pull-requests` returns conversation and gate inline, so a refresh
//! costs two spawns rather than one per pull request.

use std::process::Command;

use serde_json::{json, Value};

use super::{Capabilities, Forge, Issue, PullRequest, Request};
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

    /// An implementation that cannot say what it does answers nothing, rather
    /// than failing the whole refresh.
    fn capabilities(&self) -> Capabilities {
        let probe = Request {
            config: Value::Null,
            project: String::new(),
            tickets: Vec::new(),
            user: None,
            timeout_seconds: 15,
        };
        match self.call("capabilities", &probe, json!({})) {
            Ok(value) => serde_json::from_value(value).unwrap_or_default(),
            Err(_) => Capabilities::default(),
        }
    }

    fn pull_requests(&self, request: &Request) -> Result<Vec<PullRequest>, ProviderError> {
        let value = self.call("pull-requests", request, json!({}))?;
        self.decode("pull-requests", value)
    }

    fn issues(&self, request: &Request) -> Result<Vec<Issue>, ProviderError> {
        let value = self.call("issues", request, json!({}))?;
        self.decode("issues", value)
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
