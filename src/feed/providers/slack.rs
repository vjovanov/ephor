//! Slack channel messages (stub). Becomes active once a token exists at
//! `~/config/secrets/ephor/slack.json` with `{"token": "xoxp-..."}` — fetch
//! will then read `conversations.history` per channel with a persisted
//! cursor. Until then `available()` is false and the provider is skipped.

use serde::Deserialize;
use serde_json::Value;

use crate::feed::provider::{
    secret_exists, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::parse_config;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    #[allow(dead_code)]
    channels: Vec<String>,
}

pub struct Slack {
    #[allow(dead_code)]
    config: Config,
}

impl Slack {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(Slack {
            config: parse_config(config)?,
        })
    }
}

impl Provider for Slack {
    fn name(&self) -> &'static str {
        "slack"
    }

    fn available(&self, ctx: &ProviderContext) -> bool {
        secret_exists(ctx, "slack")
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let _ = ctx;
        Err(ProviderError(
            "slack provider is not implemented yet; token found but fetch is pending".to_string(),
        ))
    }
}
