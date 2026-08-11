//! Adapts any [`Forge`] to the [`Provider`] contract the feed is built from.
//!
//! This is the whole bridge between the two abstractions: a `Provider` is a
//! source of items, a `Forge` answers questions about a code-review host, and
//! [`policy`](crate::forge::policy) turns the latter into the former. Both
//! transports arrive here — the in-process implementations built in below, and
//! anything else via [`ExternalForge`] — so neither gets its own item-building
//! path.

use serde_json::Value;

use crate::feed::model::Item;
use crate::feed::provider::{Provider, ProviderContext, ProviderError, ProviderResult};
use crate::forge::external::ExternalForge;
use crate::forge::{policy, Forge, Request};

pub struct ForgeProvider {
    forge: Box<dyn Forge>,
    config: Value,
    /// Leaked so `Provider::name` can hand out a `&'static str`; a provider
    /// lives for the whole refresh, and there is one per configured forge.
    name: &'static str,
}

impl ForgeProvider {
    pub fn new(forge: Box<dyn Forge>, config: Value) -> Self {
        let name: &'static str = Box::leak(forge.name().into_boxed_str());
        ForgeProvider {
            forge,
            config,
            name,
        }
    }

    /// Build from a configuration block whose `provider` names no built-in
    /// provider: it names a forge, reached out of process.
    pub fn external(config: &Value) -> Result<Self, ProviderError> {
        let name = config
            .get("provider")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError("provider entry is missing 'provider'".to_string()))?
            .to_string();
        let command = config
            .get("command")
            .and_then(Value::as_str)
            .map(String::from);
        Ok(ForgeProvider::new(
            Box::new(ExternalForge::new(name, command)),
            config.clone(),
        ))
    }
}

impl Provider for ForgeProvider {
    fn name(&self) -> &'static str {
        self.name
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        self.forge.available()
    }

    fn unavailable_reason(&self) -> Option<String> {
        self.forge.unavailable_reason()
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let request = Request::new(self.config.clone(), ctx);
        // The probe's own failure is reported verbatim: it is the first thing
        // this forge does, so it is where an unreachable host, a crash or a
        // missing dependency surfaces, and each of those needs its own answer.
        let capabilities = self.forge.capabilities()?;
        if capabilities == crate::forge::Capabilities::default() {
            // Declaring nothing is indistinguishable from answering nothing, so
            // treat it as the failure it almost always is: the executable ran,
            // but does not speak the protocol.
            return Err(ProviderError(format!(
                "{} declared no capabilities — is it answering `capabilities` with JSON?",
                self.name
            )));
        }
        let mut items: Vec<Item> = Vec::new();

        if capabilities.pull_requests {
            for pr in self.forge.pull_requests(&request)? {
                items.push(policy::pull_request_item(self.name, &ctx.project_id, &pr));
            }
        }
        if capabilities.issues {
            for issue in self.forge.issues(&request)? {
                items.push(policy::issue_item(self.name, &ctx.project_id, &issue));
            }
        }
        Ok(items)
    }
}
