//! Email messages awaiting a response (stub). Becomes active once IMAP
//! credentials exist at `~/config/secrets/ephor/imap.json` with
//! `{"host": ..., "user": ..., "password": ...}`. Until then `available()`
//! is false and the provider is skipped.

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
    /// IMAP search query matching this project's mail.
    #[allow(dead_code)]
    query: String,
    #[allow(dead_code)]
    #[serde(default)]
    folder: Option<String>,
}

pub struct Email {
    #[allow(dead_code)]
    config: Config,
}

impl Email {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(Email {
            config: parse_config(config)?,
        })
    }
}

impl Provider for Email {
    fn name(&self) -> &'static str {
        "email"
    }

    fn available(&self, ctx: &ProviderContext) -> bool {
        secret_exists(ctx, "imap")
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let _ = ctx;
        Err(ProviderError(
            "email provider is not implemented yet; credentials found but fetch is pending"
                .to_string(),
        ))
    }
}
