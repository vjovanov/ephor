//! Adapts any [`Forge`] to the [`Provider`] contract the feed is built from.
//!
//! This is the whole bridge between the two abstractions: a `Provider` is a
//! source of items, a `Forge` answers questions about a code-review host, and
//! [`policy`](crate::forge::policy) turns the latter into the former. Both
//! transports arrive here — the in-process implementations built in below, and
//! anything else via [`ExternalForge`] — so neither gets its own item-building
//! path.

use serde_json::Value;

use crate::feed::config::ActionConfig;
use crate::feed::gate::{Failure, Gate};
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

    /// The forge itself, for the calls that are not fetches — the writes a
    /// reader makes on one message (§FS-004-quick-actions.5). Item building
    /// stays here; those go straight to the implementation.
    pub fn into_forge(self) -> Box<dyn Forge> {
        self.forge
    }

    /// The failures action on one item: offered on a gate that is red, on an
    /// item that still names its pull request, and only where the forge can
    /// actually say what failed (§FS-004-quick-actions.2). The capability
    /// probe is what makes the third check honest — a forge that answers
    /// nothing here would give the reader a menu entry that prints only its
    /// own refusal.
    fn failures_action(&self, item: &Item) -> Vec<ActionConfig> {
        let red = Gate::of(item).is_some_and(|gate| gate.is_red());
        let identified = item.repo().is_some() && item.number().is_some();
        if !red || !identified {
            return Vec::new();
        }
        if !matches!(self.forge.capabilities(), Ok(capabilities) if capabilities.failures) {
            return Vec::new();
        }
        vec![ActionConfig {
            id: "ci-failures".to_string(),
            icon: "✗".to_string(),
            description: "see the CI failures".to_string(),
            command: failures_command(),
            ..ActionConfig::default()
        }]
    }
}

/// `ephor failures` on the selected item, paged.
///
/// ephor asks itself rather than the forge: naming this forge's CLI in the
/// command would put a vendor tool back in the menu that
/// §FS-001-forge-interface exists to keep it out of. The binary is named by
/// its own path, so the ephor the reader is looking at is the one that
/// answers.
fn failures_command() -> String {
    let exe = std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ephor".to_string());
    format!(
        "{} failures --project \"$EPHOR_PROJECT\" --source \"$EPHOR_SOURCE\" \
         --repo \"$EPHOR_REPO\" --number \"$EPHOR_NUMBER\" 2>&1 | ${{PAGER:-less -R}}",
        super::shell_quote(&exe)
    )
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

    fn quick_actions(&self, item: &Item) -> Vec<ActionConfig> {
        if !self.forge.available() {
            return Vec::new();
        }
        self.failures_action(item)
    }

    fn failures(&self, ctx: &ProviderContext, item: &Item) -> Result<Vec<Failure>, ProviderError> {
        let (Some(repo), Some(number)) = (item.repo(), item.number()) else {
            return Err(ProviderError(format!(
                "{} does not name a pull request to ask about",
                item.id
            )));
        };
        let request = Request::new(self.config.clone(), ctx);
        self.forge.failures(&request, &repo, &number)
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
        if capabilities.notices {
            for notice in self.forge.notices(&request)? {
                items.push(policy::notice_item(self.name, &ctx.project_id, &notice));
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::gate::RepoGate;
    use crate::feed::model::ItemKind;
    use crate::forge::Capabilities;
    use serde_json::json;

    /// A forge that declares exactly what a test needs it to.
    struct Stub {
        capabilities: Capabilities,
    }

    impl Forge for Stub {
        fn name(&self) -> String {
            "stub".to_string()
        }

        fn capabilities(&self) -> Result<Capabilities, ProviderError> {
            Ok(self.capabilities)
        }

        fn failures(
            &self,
            _request: &Request,
            repo: &str,
            number: &str,
        ) -> Result<Vec<Failure>, ProviderError> {
            Ok(vec![Failure {
                job: format!("{repo}/{number}"),
                url: None,
                trace: "error: boom".to_string(),
            }])
        }
    }

    fn provider(failures: bool) -> ForgeProvider {
        ForgeProvider::new(
            Box::new(Stub {
                capabilities: Capabilities {
                    pull_requests: true,
                    gate: true,
                    failures,
                    ..Capabilities::default()
                },
            }),
            json!({ "provider": "stub" }),
        )
    }

    fn item(gate: Option<Gate>) -> Item {
        // `repo` in raw is what policy records for a forge item; the number
        // comes out of the id.
        let raw = match gate {
            Some(gate) => json!({ "repo": "app", "gate": gate.to_value() }),
            None => json!({ "repo": "app" }),
        };
        Item {
            id: "stub:app/101".to_string(),
            project: "widget".to_string(),
            source: "stub".to_string(),
            kind: ItemKind::Pr,
            role: None,
            title: "Widen the retry window".to_string(),
            url: None,
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw,
        }
    }

    fn gate(passed: u64, failed: u64, blocked: bool) -> Gate {
        Gate {
            repos: vec![RepoGate {
                repo: "app".to_string(),
                passed,
                failed,
                running: 0,
            }],
            blocked,
            blockers: Vec::new(),
        }
    }

    #[test]
    fn the_condition_is_a_red_gate_not_a_kind_of_item() {
        let provider = provider(true);
        let offered = |item: &Item| provider.quick_actions(item).len();

        // Failed jobs, and an all-green gate the forge still refuses: both are
        // red, and both are what a reader opens (§FS-004-quick-actions.4).
        assert_eq!(offered(&item(Some(gate(40, 6, false)))), 1);
        assert_eq!(offered(&item(Some(gate(118, 0, true)))), 1);

        // A green gate has nothing to explain, and neither has an item with no
        // gate at all.
        assert_eq!(offered(&item(Some(gate(40, 0, false)))), 0);
        assert_eq!(offered(&item(None)), 0);

        // An item that no longer names its pull request cannot be asked about.
        let mut anonymous = item(Some(gate(40, 6, false)));
        anonymous.id = "stub:app".to_string();
        assert_eq!(offered(&anonymous), 0);
    }

    #[test]
    fn a_forge_that_cannot_say_what_failed_offers_nothing() {
        // Rather than a menu entry that would only print its own refusal
        // (§FS-004-quick-actions.2).
        assert_eq!(
            provider(false)
                .quick_actions(&item(Some(gate(40, 6, false))))
                .len(),
            0
        );
    }

    #[test]
    fn the_action_asks_ephor_rather_than_naming_the_forges_cli() {
        let command = provider(true).quick_actions(&item(Some(gate(40, 6, false))))[0]
            .command
            .clone();
        assert!(
            command.contains("failures --project \"$EPHOR_PROJECT\""),
            "{command}"
        );
        assert!(command.contains("--source \"$EPHOR_SOURCE\""), "{command}");
        assert!(command.contains("${PAGER:-less -R}"), "{command}");
        // Nothing in it names this forge or a vendor tool (§FS-001-forge-interface).
        assert!(!command.contains("stub"), "{command}");
    }
}
