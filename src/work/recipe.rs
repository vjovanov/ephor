//! Recipes: which items deserve work, and what the ticket asks for
//! (§FS-005-dispatch.1).
//!
//! A recipe is a selector and a brief. The selector is ephor's own vocabulary
//! — kind, role, gate, whether a response is owed — so one recipe matches the
//! same items whichever forge reported them
//! (§FS-001-forge-interface.3). The brief is the reader's, rendered with the
//! item's fields where it names them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::feed::gate::Gate;
use crate::feed::model::{Item, ItemKind, ItemRole};

/// The work half of the feed configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkConfig {
    /// Where an item's plan is written. `{workspace}` is the checkout the item
    /// resolves to, `{root}` the project root, `{project}` its id.
    #[serde(default = "default_root")]
    pub root: String,
    /// A states YAML to install into a work root that has none, instead of the
    /// one ephor ships.
    #[serde(default)]
    pub states: Option<String>,
    /// What runs a plan. The runtime is a binding with one shipped wired and
    /// ready (§FS-005-dispatch lead, §DA-001-runtime-bound-default); naming
    /// another here is how a person who works differently points work at it.
    #[serde(default)]
    pub runner: Option<String>,
    /// Recipes, appended to the shipped ones; one reusing a shipped id
    /// replaces it (§FS-005-dispatch.1).
    #[serde(default)]
    pub recipes: Vec<Recipe>,
    /// Who does which action, by the action's own id, with [`DEFAULT_HAND`]
    /// answering for every id the table does not name
    /// (§FS-006-project-interface.9). The site's answer, which a project's
    /// table displaces.
    #[serde(default)]
    pub hands: BTreeMap<String, HandPin>,
}

impl Default for WorkConfig {
    fn default() -> Self {
        WorkConfig {
            root: default_root(),
            states: None,
            runner: None,
            recipes: Vec::new(),
            hands: BTreeMap::new(),
        }
    }
}

fn default_root() -> String {
    format!("{{workspace}}/{}", crate::work::runtime::plan::PROJECT_DIR)
}

/// Work per project: extra recipes, and a work root of its own.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectWorkConfig {
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub states: Option<String>,
    #[serde(default)]
    pub recipes: Vec<Recipe>,
    /// Who does which action on this project — read before the site's table,
    /// this action's id before [`DEFAULT_HAND`] (§FS-006-project-interface.9).
    #[serde(default)]
    pub hands: BTreeMap<String, HandPin>,
    /// The hands that may be used on this project at all. Empty asks nothing;
    /// a non-empty list refuses everything outside it with that reason,
    /// wherever it was named (§FS-006-project-interface.9) — which is what a
    /// repository under a policy about which models may see its code needs.
    #[serde(default)]
    pub permitted_hands: Vec<String>,
}

/// The key a hands table answers every unnamed action with
/// (§FS-006-project-interface.9).
pub const DEFAULT_HAND: &str = "default";

/// Who a piece of work is meant for, as configuration names them
/// (§FS-006-project-interface.9): a hand the roster knows, or — for a pair the
/// runtime's registry never enumerated — the agent and the model it carries,
/// spelled out. Nothing here is the runtime's own grammar: a hand is an id and
/// an effort, and rendering one into whatever the binding executes happens in
/// the runtime adapter alone (§REQ-001-boundary.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandPin {
    /// `<hand-id>[:<effort>]`.
    Named { id: String, effort: Option<String> },
    /// `{ "agent": …, "model": …, "effort": … }` — both halves of the pair
    /// named, because half of one is a hand the roster already has an id for.
    Spelled {
        agent: String,
        model: String,
        effort: Option<String>,
    },
}

impl HandPin {
    /// The effort asked for, where one was.
    pub fn effort(&self) -> Option<&str> {
        match self {
            HandPin::Named { effort, .. } | HandPin::Spelled { effort, .. } => effort.as_deref(),
        }
    }

    /// How a message names this pin back to the person who wrote it.
    pub fn describe(&self) -> String {
        let effort = match self.effort() {
            Some(effort) => format!(" at effort '{effort}'"),
            None => String::new(),
        };
        match self {
            HandPin::Named { id, .. } => format!("'{id}'{effort}"),
            HandPin::Spelled { agent, model, .. } => format!("{agent} carrying {model}{effort}"),
        }
    }

    /// `<hand-id>[:<effort>]`, refusing what cannot be one. A selector in the
    /// runtime's own words is not a hand: it is spelled out in full instead,
    /// so that what configuration names is checkable against the roster.
    /// Public because the same words are typed at a command line as written in
    /// configuration, and one grammar cannot be read two ways.
    pub fn parse(text: &str) -> std::result::Result<HandPin, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("a hand is named '<hand>' or '<hand>:<effort>'; this one is empty".into());
        }
        let mut parts = text.splitn(2, ':');
        let id = parts.next().unwrap_or_default().trim();
        let effort = parts.next().map(str::trim);
        if id.is_empty() {
            return Err(format!("hand '{text}' names no hand before the ':'"));
        }
        match effort {
            Some(effort) if effort.is_empty() || effort.contains(':') => Err(format!(
                "hand '{text}' is not '<hand>:<effort>'; to name an agent and a model that the \
                 runtime's registry does not list, spell them out as \
                 {{ \"agent\": …, \"model\": … }}"
            )),
            effort => Ok(HandPin::Named {
                id: id.to_string(),
                effort: effort.map(str::to_string),
            }),
        }
    }
}

impl<'de> Deserialize<'de> for HandPin {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<HandPin, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Long {
            #[serde(default)]
            agent: Option<String>,
            #[serde(default)]
            model: Option<String>,
            #[serde(default)]
            effort: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Short(String),
            Long(Long),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Short(text) => HandPin::parse(&text).map_err(serde::de::Error::custom),
            Raw::Long(long) => match (long.agent, long.model) {
                (Some(agent), Some(model)) => Ok(HandPin::Spelled {
                    agent,
                    model,
                    effort: long.effort,
                }),
                // Half a pair is a hand the roster names on its own, and
                // naming it by id is what lets ephor check it.
                _ => Err(serde::de::Error::custom(
                    "a hand spelled out in full names both 'agent' and 'model'; \
                     to name one of them alone, use the roster's id for it",
                )),
            },
        }
    }
}

impl Serialize for HandPin {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        match self {
            HandPin::Named { id, effort } => match effort {
                Some(effort) => serializer.serialize_str(&format!("{id}:{effort}")),
                None => serializer.serialize_str(id),
            },
            HandPin::Spelled {
                agent,
                model,
                effort,
            } => {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(None)?;
                map.serialize_entry("agent", agent)?;
                map.serialize_entry("model", model)?;
                if let Some(effort) = effort {
                    map.serialize_entry("effort", effort)?;
                }
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    pub description: String,
    /// The state a fresh ticket starts in, from the work root's machine.
    #[serde(default = "default_state")]
    pub state: String,
    #[serde(default)]
    pub when: Selector,
    /// The work needs the item's own branch on disk. Where it does and the
    /// branch is not checked out, dispatch refuses rather than writing a
    /// ticket about code that is not on the machine (§FS-005-dispatch.3).
    /// Work that reads a change rather than editing it — a review, a reply —
    /// says `false` and runs in the project's own checkout.
    #[serde(default = "yes")]
    pub needs_checkout: bool,
    /// Which branch this work belongs on, where the matter has none of its own
    /// (§FS-005-dispatch.25). A template rendered from the matter's fields
    /// exactly as [`Recipe::brief`] is — `fix/issue-{number}` — which dispatch
    /// resolves and makes the workspace of. Saying it means the work needs the
    /// checkout; a recipe that says nothing is placed as it always was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// This work needs nobody to start it: a ticket written from this recipe
    /// gets its run without anyone pressing a key (§FS-005-dispatch.24). The
    /// reader's deliberate act is adopting the recipe, made once, rather than
    /// starting each of its tickets. Silence means the key, as it always did —
    /// per recipe and nowhere else, because trusting one kind of work to start
    /// itself says nothing about the rest.
    #[serde(default)]
    pub autorun: bool,
    /// What the ticket asks for, in the reader's words. `{...}` placeholders
    /// are filled from the item (see [`super::dossier::Subject::placeholders`]),
    /// plus `{reply}` — where a proposed answer for this matter belongs, which
    /// ephor reads back and offers beside the conversation
    /// (§FS-005-dispatch.13).
    pub brief: String,
    /// A deterministic opening move ephor makes itself, before the ticket
    /// costs a model (§FS-005-dispatch.12). Where the move finishes, nothing
    /// is dispatched at all; where it stops, what it reached is written into
    /// the brief and that is the ticket. `rebase` is the one ephor knows —
    /// the same operation the reader presses a key for
    /// (§FS-004-quick-actions.6), so two of them cannot disagree about what a
    /// clean rebase is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens_with: Option<String>,
    /// Whose work this is, when it is not whoever the tables would default to
    /// — the second of the seven steps, and the portable spelling: a hand id
    /// the roster knows, checked against it (§FS-006-project-interface.9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<HandPin>,
    /// Pin the runtime's execution identity for this ticket. The runtime's own
    /// words rather than a hand, so nothing checks it — it pins this recipe
    /// the same way `hand` does, and a project's tables do not displace it. A
    /// recipe carrying both this and `hand` is refused at dispatch: one of
    /// them would silently lose (§FS-006-project-interface.9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// The deterministic moves ephor can make on its own behalf
/// (§FS-005-dispatch.12). The rebase is the first of these, not the shape of
/// the only one.
pub const OPENING_REBASE: &str = "rebase";

fn default_icon() -> String {
    "◆".to_string()
}

/// Where a ticket starts when nothing says otherwise: the shipped machine's
/// working state. Public because a menu entry that carries a brief instead of
/// a command is a recipe under another name (§FS-005-dispatch.1), and it
/// starts where the recipes do.
pub fn default_state() -> String {
    "fix".to_string()
}

fn yes() -> bool {
    true
}

/// Which items a recipe applies to. An empty field asks nothing; every field
/// that is set must hold.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    /// `pr`, `ci`, `issue`, `task`, `message`, `status`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    /// `author`, `reviewer`. An item whose source reported no role — a
    /// project's own task among them (§FS-003-feed-categories.1) — matches
    /// only when this is empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    /// What the gate has to be doing. The two ways a gate is red are separate
    /// conditions because they ask for different work: `failing` — jobs
    /// failed, which is something a checkout can fix; `blocked` — the forge
    /// refuses the merge, which is often an approval nobody can give from
    /// here. Also `red` (either), `green` (neither), and `any` (a gate at
    /// all).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_response: Option<bool>,
    /// Provider names, for a recipe that only makes sense on one source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    /// The item's branch trails its main branch (`true`), or is level with it
    /// (`false`) — measured in the checkout, not asked of a forge
    /// (§FS-004-quick-actions.6). An item whose checkout cannot be measured —
    /// no branch, nothing on disk — matches neither, because a recipe that
    /// asks about the checkout and gets no answer is being offered blind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behind: Option<bool>,
    /// The item's branch trails its own **published copy** (`true`), or is
    /// level with it (`false`) — the other distance, and a different question
    /// from the one above (§FS-004-quick-actions.8). A branch published
    /// nowhere matches neither, for the same reason an unmeasurable checkout
    /// does not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behind_upstream: Option<bool>,
}

/// What a selector asks about that the item does not carry: facts measured in
/// the reader's own checkout.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    /// Commits the item's branch trails its main branch; None where it could
    /// not be measured.
    pub behind: Option<u64>,
    /// Commits it trails its own published copy; None where there is no copy
    /// to measure against (§FS-004-quick-actions.8).
    pub behind_upstream: Option<u64>,
}

impl Selector {
    /// Whether this selector holds for an item. Public because the same
    /// language selects menu offers, whoever wrote them
    /// (§FS-006-project-interface.9).
    pub fn matches(&self, item: &Item, facts: &Facts) -> bool {
        self.explain(item, facts).is_empty()
    }

    /// The explain-capable companion to [`Selector::matches`]: every field
    /// that refused, and what it found instead of what it asked for
    /// (§FS-005-dispatch.27). Empty exactly where `matches` would answer
    /// `true` — this decides nothing `matches` did not already decide, it
    /// only says why where `matches` only said no.
    pub fn explain(&self, item: &Item, facts: &Facts) -> Vec<Refusal> {
        let mut refusals = Vec::new();
        if let Some(want) = self.behind {
            match facts.behind {
                Some(behind) if (behind > 0) == want => {}
                Some(behind) => refusals.push(Refusal::new(
                    "behind",
                    format!(
                        "the branch is {} its main branch; the selector asks for {}",
                        if behind > 0 { "behind" } else { "level with" },
                        if want { "behind" } else { "level with it" }
                    ),
                )),
                None => refusals.push(Refusal::new(
                    "behind",
                    "the branch could not be measured against a main branch here",
                )),
            }
        }
        // The same shape for the other distance, and asked the same way: a
        // branch with no published copy answers neither `true` nor `false`
        // (§FS-004-quick-actions.8).
        if let Some(want) = self.behind_upstream {
            match facts.behind_upstream {
                Some(behind) if (behind > 0) == want => {}
                Some(behind) => refusals.push(Refusal::new(
                    "behind_upstream",
                    format!(
                        "the branch is {} its published copy; the selector asks for {}",
                        if behind > 0 { "behind" } else { "level with" },
                        if want { "behind" } else { "level with it" }
                    ),
                )),
                None => refusals.push(Refusal::new(
                    "behind_upstream",
                    "the branch has no published copy to measure against",
                )),
            }
        }
        if !self.kinds.is_empty() && !self.kinds.iter().any(|kind| kind_matches(item.kind, kind)) {
            refusals.push(Refusal::new(
                "kinds",
                format!(
                    "the matter's kind is `{}`; the selector asks for {}",
                    item.kind.label(),
                    join_quoted(&self.kinds)
                ),
            ));
        }
        if !self.roles.is_empty() && !self.roles.iter().any(|role| role_matches(item.role, role)) {
            refusals.push(Refusal::new(
                "roles",
                match item.role {
                    // The role-less case this exists for: a project's own
                    // task carries no role at all, so it is not merely a
                    // role the selector did not ask for
                    // (§FS-003-feed-categories.1).
                    None => format!(
                        "the matter carries no role; the selector asks for {}",
                        join_quoted(&self.roles)
                    ),
                    Some(role) => format!(
                        "the matter's role is `{}`; the selector asks for {}",
                        role_label(role),
                        join_quoted(&self.roles)
                    ),
                },
            ));
        }
        if !self.sources.is_empty() && !self.sources.contains(&item.source) {
            refusals.push(Refusal::new(
                "sources",
                format!(
                    "the matter's source is `{}`; the selector asks for {}",
                    item.source,
                    join_quoted(&self.sources)
                ),
            ));
        }
        if let Some(needs_response) = self.needs_response {
            if item.needs_response != needs_response {
                refusals.push(Refusal::new(
                    "needs_response",
                    format!(
                        "the matter {} an answer; the selector asks for one that {}",
                        if item.needs_response {
                            "needs"
                        } else {
                            "does not need"
                        },
                        if needs_response { "does" } else { "does not" }
                    ),
                ));
            }
        }
        if let Some(want) = self.gate.as_deref() {
            if !gate_matches(Gate::of(item), want) {
                refusals.push(Refusal::new(
                    "gate",
                    format!("the matter's gate does not match `{want}`"),
                ));
            }
        }
        refusals
    }
}

/// Why a selector refused an item, one per field that asked for something the
/// item did not answer (§FS-005-dispatch.27). `field` is the selector's own
/// name for it, so a caller naming the refusal names the same word a person
/// would edit in the recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub field: &'static str,
    pub reason: String,
}

impl Refusal {
    fn new(field: &'static str, reason: impl Into<String>) -> Self {
        Refusal {
            field,
            reason: reason.into(),
        }
    }
}

fn join_quoted(values: &[String]) -> String {
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn role_label(role: ItemRole) -> &'static str {
    match role {
        ItemRole::Author => "author",
        ItemRole::Reviewer => "reviewer",
    }
}

fn kind_matches(kind: ItemKind, name: &str) -> bool {
    // The Message label is "msg"; accept the config-friendly spelling too.
    name == kind.label() || (kind == ItemKind::Message && name == "message")
}

fn role_matches(role: Option<ItemRole>, name: &str) -> bool {
    match (role, name) {
        (Some(ItemRole::Author), "author") => true,
        (Some(ItemRole::Reviewer), "reviewer") => true,
        _ => false,
    }
}

fn gate_matches(gate: Option<Gate>, want: &str) -> bool {
    let Some(gate) = gate else {
        return false;
    };
    match want {
        "failing" => gate.failed() > 0,
        "blocked" => gate.blocked,
        "red" => gate.is_red(),
        "green" => !gate.is_red(),
        _ => true,
    }
}

impl Recipe {
    /// Whether this recipe applies to the item. Finished work never does
    /// (§FS-005-dispatch.6): asking an agent to fix a merged pull request is
    /// asking it to invent something to do.
    pub fn matches(&self, item: &Item, facts: &Facts) -> bool {
        !item.is_finished() && self.when.matches(item, facts)
    }
}

/// The recipes ephor knows without being told (§FS-005-dispatch.1). They ask
/// for what is true on every forge — read the failures, answer the question,
/// read the change, do the issue — and stop at a local change
/// (§FS-005-dispatch.7).
pub fn shipped() -> Vec<Recipe> {
    let recipe = |id: &str,
                  icon: &str,
                  description: &str,
                  needs_checkout: bool,
                  when: Selector,
                  brief: &str| Recipe {
        id: id.to_string(),
        icon: icon.to_string(),
        description: description.to_string(),
        state: default_state(),
        when,
        needs_checkout,
        // What ships is about a matter the forge already put on a branch, or
        // about no branch at all: a template naming one is a thing a project
        // says about its own work (§FS-005-dispatch.25).
        branch: None,
        // Silence means the key: what ships is started by the reader, and
        // saying otherwise is a thing configuration does (§FS-005-dispatch.24).
        autorun: false,
        brief: brief.to_string(),
        opens_with: None,
        // The shipped recipes name nobody: who does them is the reader's
        // table to write, and unwritten is the runtime's to pick
        // (§FS-006-project-interface.9).
        hand: None,
        target: None,
        model: None,
    };
    vec![
        recipe(
            "fix-gate",
            "🛠",
            "fix the red gate",
            // Fixing a gate is editing the change, so the change has to be here.
            true,
            Selector {
                kinds: vec!["pr".to_string(), "ci".to_string()],
                roles: vec!["author".to_string()],
                // Jobs that failed, not a gate that merely refuses: a change
                // waiting on an approval has nothing for a checkout to do, and
                // sending an agent at it spends a pass to be told so.
                gate: Some("failing".to_string()),
                ..Selector::default()
            },
            "The gate on {title} is red.\n\n\
             Find out what actually failed — the dossier above says what the watch\n\
             knew, and the forge itself says the rest — and fix the cause of it. A\n\
             job that failed for a reason unrelated to this change (a flake, an\n\
             infrastructure error, a broken upstream) is not something to fix here:\n\
             say so in the report and stop.\n\n\
             Where the gate is blocked rather than failing, the blockers above say\n\
             why. Some of them are nobody's to fix from a checkout — an approval, a\n\
             downstream repository — and those belong in the report, not in a\n\
             change.",
        ),
        recipe(
            "answer",
            "💬",
            "answer the conversation",
            // An answer is usually words, and the ones that need code can fetch
            // it: refusing every question asked about a branch that happens not
            // to be checked out would leave most of them unanswered
            // (§FS-005-dispatch.13).
            false,
            Selector {
                kinds: vec!["pr".to_string(), "issue".to_string(), "message".to_string()],
                needs_response: Some(true),
                ..Selector::default()
            },
            // The reply is asked for as a file of its own, because ephor reads
            // it back and offers it beside the conversation it answers
            // (§FS-005-dispatch.13). Prose about the reply belongs in the
            // report; that file is the reply and nothing else.
            "{title} is waiting on an answer from me.\n\n\
             The conversation is above, last message last. Work out what is being\n\
             asked. Where the answer is a change, make it. Where the answer is a\n\
             sentence, write the sentence.\n\n\
             Write the reply itself to {reply} — the whole message, in my voice,\n\
             exactly as it would be posted, and nothing else in that file: no\n\
             heading, no preamble, no notes about it. Say in the report what you\n\
             based it on and what you were unsure of.\n\n\
             Do not post it anywhere. Posting is mine to do.",
        ),
        recipe(
            "review",
            "👓",
            "review this change",
            // Someone else's branch is almost never checked out here, and a
            // review reads a change rather than editing it.
            false,
            Selector {
                kinds: vec!["pr".to_string()],
                roles: vec!["reviewer".to_string()],
                ..Selector::default()
            },
            "{title} is a change I am reviewing.\n\n\
             Read the change itself — fetch the branch if it is not here — and\n\
             review it as someone who has to live with it: correctness first, then\n\
             what it does to the code around it, then what it will cost to keep.\n\n\
             The report is the review: each point as the file and line it is about,\n\
             what is wrong, and what would be right. Say plainly which points would\n\
             block a merge and which are opinions. Do not post anything.",
        ),
        recipe(
            "implement",
            "🧩",
            "do the work in this issue",
            // An issue has no branch of its own yet; it starts from the project.
            false,
            Selector {
                kinds: vec!["issue".to_string()],
                roles: vec!["author".to_string()],
                ..Selector::default()
            },
            "{title} is an issue of mine.\n\n\
             The issue and its comments are above. Work out what is actually being\n\
             asked for — an issue is a description of a problem, not a\n\
             specification — and do the smallest thing that answers it, with a test\n\
             where the project tests that kind of change.\n\n\
             Where the issue is under-specified in a way that changes what the code\n\
             should do, do not guess: write the question in the report and stop.",
        ),
        Recipe {
            // The replay itself is ephor's, made before this ticket exists
            // (§FS-005-dispatch.12): a clean rebase is a done thing and never
            // reaches a model, and what is dispatched is the conflict the
            // algorithm stopped at.
            opens_with: Some(OPENING_REBASE.to_string()),
            ..recipe(
                "rebase",
                "⤴",
                "rebase onto the main branch, handing over what conflicts",
                // Replaying a branch happens where the branch is.
                true,
                Selector {
                    // Only where the branch has actually fallen behind: a
                    // recipe offered on a change that is already current is a
                    // ticket to do nothing (§FS-004-quick-actions.6). It is
                    // last of the shipped recipes because a red gate or an
                    // owed answer is the more urgent thing about the same
                    // pull request.
                    //
                    // And nothing else. This is the one recipe an entry of the
                    // menu dispatches by name — the replay hands its conflict
                    // to `rebase` (§FS-005-dispatch.12) — so what the entry is
                    // offered on and what the recipe applies to have to be the
                    // same set (§FS-005-dispatch.1). The entry asks about a
                    // branch on disk that trails its base and nothing else
                    // (§FS-004-quick-actions.6), so neither does this: the
                    // kind of row that mentions the branch, and whose change
                    // the forge says it is, are facts about how the reader
                    // arrived, not about the checkout being replayed — and a
                    // distance can only be measured where the branch is here.
                    behind: Some(true),
                    ..Selector::default()
                },
                "{title} is on {branch}, which has fallen behind its main branch, and the\n\
                 replay has already been run — the report above says where it stopped.\n\n\
                 A conflict is standing in the working tree of the repository it names.\n\
                 Resolve each conflict as the change itself would have been written against\n\
                 the new base — take neither side on principle, work out what the two commits\n\
                 were each trying to do, and where that is not decidable from the code, stop\n\
                 and say so rather than guessing. `git add` what you resolved and\n\
                 `git rebase --continue` until the replay finishes.\n\n\
                 Then check the result: build or test what the conflicting files belong to, so\n\
                 that \"it rebased\" is not the same claim as \"it still works\". Do not push.",
            )
        },
    ]
}

/// The recipes offered on a project: the shipped ones, then configuration,
/// with a configured recipe replacing a shipped one of the same id.
pub fn resolve(global: &[Recipe], project: &[Recipe]) -> Vec<Recipe> {
    let mut resolved = shipped();
    for recipe in global.iter().chain(project) {
        match resolved
            .iter()
            .position(|existing| existing.id == recipe.id)
        {
            Some(index) => resolved[index] = recipe.clone(),
            None => resolved.push(recipe.clone()),
        }
    }
    resolved
}

/// The recipes that apply to one item, in offer order.
pub fn applicable(recipes: &[Recipe], item: &Item, facts: &Facts) -> Vec<Recipe> {
    recipes
        .iter()
        .filter(|recipe| recipe.matches(item, facts))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::gate::RepoGate;
    use serde_json::json;

    fn item(kind: ItemKind, role: Option<ItemRole>) -> Item {
        Item {
            id: "github-prs:acme/widget#42".to_string(),
            project: "widget".to_string(),
            source: "github-prs".to_string(),
            kind,
            role,
            title: "Retry window".to_string(),
            url: None,
            state: Some("open".to_string()),
            needs_response: false,
            updated_at: chrono::Utc::now(),
            raw: json!({}),
        }
    }

    fn with_gate(mut item: Item, failed: u64, blocked: bool) -> Item {
        let gate = Gate {
            repos: vec![RepoGate {
                repo: "widget".to_string(),
                passed: 12,
                failed,
                running: 0,
            }],
            blocked,
            blockers: Vec::new(),
        };
        item.raw = json!({ "gate": gate.to_value() });
        item
    }

    /// Which recipes apply, with the checkout unmeasured — the answer for
    /// every item whose branch is not on this machine.
    fn ids(recipes: &[Recipe], item: &Item) -> Vec<String> {
        ids_with(recipes, item, Facts::default())
    }

    fn ids_with(recipes: &[Recipe], item: &Item, facts: Facts) -> Vec<String> {
        applicable(recipes, item, &facts)
            .into_iter()
            .map(|recipe| recipe.id)
            .collect()
    }

    #[test]
    fn the_red_gate_recipe_is_offered_on_my_own_failing_change_only() {
        let recipes = shipped();
        let mine = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 2, false);
        assert!(ids(&recipes, &mine).contains(&"fix-gate".to_string()));

        // Green: nothing to fix.
        let green = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 0, false);
        assert!(!ids(&recipes, &green).contains(&"fix-gate".to_string()));

        // Someone else's change with a red gate is not mine to fix.
        let theirs = with_gate(item(ItemKind::Pr, Some(ItemRole::Reviewer)), 2, false);
        assert_eq!(ids(&recipes, &theirs), ["review"]);

        // A gate whose jobs all passed and which the forge refuses anyway is
        // waiting on a person, not on a fix.
        let blocked = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 0, true);
        assert!(!ids(&recipes, &blocked).contains(&"fix-gate".to_string()));
        // Both at once is still work for a checkout.
        let both = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 2, true);
        assert!(ids(&recipes, &both).contains(&"fix-gate".to_string()));
    }

    #[test]
    fn the_two_ways_a_gate_is_red_are_separate_conditions() {
        let selector = |gate: &str| Selector {
            gate: Some(gate.to_string()),
            ..Selector::default()
        };
        let failing = with_gate(item(ItemKind::Pr, None), 2, false);
        let refused = with_gate(item(ItemKind::Pr, None), 0, true);
        let clean = with_gate(item(ItemKind::Pr, None), 0, false);

        assert!(selector("failing").matches(&failing, &Facts::default()));
        assert!(!selector("failing").matches(&refused, &Facts::default()));
        assert!(selector("blocked").matches(&refused, &Facts::default()));
        assert!(!selector("blocked").matches(&failing, &Facts::default()));
        assert!(
            selector("red").matches(&failing, &Facts::default())
                && selector("red").matches(&refused, &Facts::default())
        );
        assert!(
            selector("green").matches(&clean, &Facts::default())
                && !selector("green").matches(&refused, &Facts::default())
        );
        assert!(selector("any").matches(&clean, &Facts::default()));
        // No gate at all answers no gate question.
        assert!(!selector("any").matches(&item(ItemKind::Pr, None), &Facts::default()));
    }

    /// A project's own task carries no role at all (§FS-003-feed-categories.1),
    /// and a `roles` selector matches a role-less item only when it is empty
    /// — that does not change here (§FS-005-dispatch.27). What changes is
    /// that the refusal now says so, naming `roles` and that the matter
    /// carries no role, instead of leaving the exclusion silent.
    #[test]
    fn a_role_less_item_explains_the_roles_refusal_by_name() {
        let wants_author = Selector {
            roles: vec!["author".to_string()],
            ..Selector::default()
        };
        let task = item(ItemKind::Task, None);

        assert!(!wants_author.matches(&task, &Facts::default()));
        let refused = wants_author.explain(&task, &Facts::default());
        assert_eq!(refused.len(), 1);
        assert_eq!(refused[0].field, "roles");
        assert!(refused[0].reason.contains("carries no role"));
        assert!(refused[0].reason.contains("`author`"));

        // An empty `roles` still matches the same role-less item, and
        // explains nothing because nothing refused.
        let no_roles = Selector::default();
        assert!(no_roles.matches(&task, &Facts::default()));
        assert!(no_roles.explain(&task, &Facts::default()).is_empty());

        // A role that is merely the wrong one is named the same way, without
        // the role-less wording.
        let reviewer = item(ItemKind::Pr, Some(ItemRole::Reviewer));
        let wants_reviewer_role = wants_author.explain(&reviewer, &Facts::default());
        assert_eq!(wants_reviewer_role.len(), 1);
        assert_eq!(wants_reviewer_role[0].field, "roles");
        assert!(wants_reviewer_role[0].reason.contains("`reviewer`"));
        assert!(!wants_reviewer_role[0].reason.contains("carries no role"));
    }

    #[test]
    fn finished_work_is_never_dispatched() {
        let recipes = shipped();
        let mut merged = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 2, false);
        merged.state = Some("merged".to_string());
        assert!(ids(&recipes, &merged).is_empty());
    }

    #[test]
    fn an_owed_answer_is_recognized_wherever_it_arrived() {
        let recipes = shipped();
        for kind in [ItemKind::Pr, ItemKind::Issue, ItemKind::Message] {
            let mut waiting = item(kind, None);
            waiting.needs_response = true;
            assert!(
                ids(&recipes, &waiting).contains(&"answer".to_string()),
                "{kind:?}"
            );
        }
        // A status line is not a conversation.
        let mut status = item(ItemKind::Status, None);
        status.needs_response = true;
        assert!(ids(&recipes, &status).is_empty());
    }

    /// The rebase is offered on what has actually fallen behind, and nothing
    /// else (§FS-004-quick-actions.6).
    #[test]
    fn the_rebase_recipe_is_offered_only_where_the_branch_trails_main() {
        let recipes = shipped();
        let rebase = "rebase".to_string();
        let mine = item(ItemKind::Pr, Some(ItemRole::Author));

        // Nothing measured — no branch, or nothing on disk — is not an
        // invitation to guess.
        assert!(!ids(&recipes, &mine).contains(&rebase));
        // Level with main: replaying it would be a ticket to do nothing.
        assert!(!ids_with(
            &recipes,
            &mine,
            Facts {
                behind: Some(0),
                ..Facts::default()
            }
        )
        .contains(&rebase));
        assert!(ids_with(
            &recipes,
            &mine,
            Facts {
                behind: Some(3),
                ..Facts::default()
            }
        )
        .contains(&rebase));

        // The kind of row that mentions the branch, and whose change the forge
        // says it is, do not enter it: this is the one recipe a menu entry
        // dispatches by name, and the entry asks about a branch on disk that
        // trails its base and nothing else (§FS-005-dispatch.1,
        // §FS-004-quick-actions.6). Gating the two differently would mean the
        // key handing over work its own recipe says does not apply here.
        for (kind, role) in [
            (ItemKind::Pr, Some(ItemRole::Reviewer)),
            (ItemKind::Issue, Some(ItemRole::Author)),
            (ItemKind::Message, None),
            (ItemKind::Status, None),
        ] {
            assert!(
                ids_with(
                    &recipes,
                    &item(kind, role),
                    Facts {
                        behind: Some(3),
                        ..Facts::default()
                    }
                )
                .contains(&rebase),
                "{kind:?} {role:?}"
            );
        }

        // A red gate on the same pull request is the more urgent thing about
        // it, and a sweep takes the first match.
        let failing = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 2, false);
        assert_eq!(
            ids_with(
                &recipes,
                &failing,
                Facts {
                    behind: Some(3),
                    ..Facts::default()
                }
            )
            .first(),
            Some(&"fix-gate".to_string())
        );

        // Merged, and behind by a mile: there is nothing to replay onto.
        let mut merged = item(ItemKind::Pr, Some(ItemRole::Author));
        merged.state = Some("merged".to_string());
        assert!(ids_with(
            &recipes,
            &merged,
            Facts {
                behind: Some(9),
                ..Facts::default()
            }
        )
        .is_empty());
    }

    #[test]
    fn a_selector_can_ask_for_a_branch_that_is_level_with_main() {
        let level = Selector {
            behind: Some(false),
            ..Selector::default()
        };
        let pr = item(ItemKind::Pr, None);
        assert!(level.matches(
            &pr,
            &Facts {
                behind: Some(0),
                ..Facts::default()
            }
        ));
        assert!(!level.matches(
            &pr,
            &Facts {
                behind: Some(2),
                ..Facts::default()
            }
        ));
        // Unmeasurable answers neither question.
        assert!(!level.matches(&pr, &Facts::default()));
    }

    /// The other distance is asked about in the same words, and answered from
    /// the same fold (§FS-004-quick-actions.8). The two are separate
    /// questions: a branch level with main can be well behind its own copy.
    #[test]
    fn a_selector_can_ask_about_the_published_copy_instead_of_main() {
        let trails_copy = Selector {
            behind_upstream: Some(true),
            ..Selector::default()
        };
        let pr = item(ItemKind::Pr, None);
        assert!(trails_copy.matches(
            &pr,
            &Facts {
                behind: Some(0),
                behind_upstream: Some(2),
            }
        ));
        assert!(!trails_copy.matches(
            &pr,
            &Facts {
                behind: Some(9),
                behind_upstream: Some(0),
            }
        ));
        // Published nowhere answers neither, like an unmeasurable checkout.
        assert!(!trails_copy.matches(&pr, &Facts::default()));

        // And both at once ask for both.
        let both = Selector {
            behind: Some(true),
            behind_upstream: Some(true),
            ..Selector::default()
        };
        assert!(both.matches(
            &pr,
            &Facts {
                behind: Some(1),
                behind_upstream: Some(2),
            }
        ));
        assert!(!both.matches(
            &pr,
            &Facts {
                behind: Some(0),
                behind_upstream: Some(2),
            }
        ));
    }

    #[test]
    fn configuration_adds_recipes_and_replaces_a_shipped_one_by_id() {
        let configured: Vec<Recipe> = serde_json::from_value(json!([
            { "id": "fix-gate", "description": "our own gate fix", "brief": "do it our way" },
            { "id": "bench", "description": "run the benchmarks", "brief": "bench {title}",
              "when": { "kinds": ["pr"] }, "state": "fix" }
        ]))
        .unwrap();
        let resolved = resolve(&configured, &[]);
        let fix = resolved.iter().find(|r| r.id == "fix-gate").unwrap();
        assert_eq!(fix.description, "our own gate fix");
        // Replacing keeps the position; the new one lands at the end.
        assert_eq!(resolved.len(), shipped().len() + 1);
        assert_eq!(resolved.last().unwrap().id, "bench");
        // The replacement's own selector applies — no gate condition now.
        let plain = item(ItemKind::Pr, None);
        assert!(ids(&resolved, &plain).contains(&"fix-gate".to_string()));
    }

    /// The table a project writes to say who does what
    /// (§FS-006-project-interface.9): action id to hand, `default` for the
    /// rest, the short form for a hand the roster names and the long one for a
    /// pair it never enumerated.
    #[test]
    fn who_does_which_action_is_a_table_of_hands() {
        let work: ProjectWorkConfig = serde_json::from_value(json!({
            "hands": {
                "default": "sonnet",
                "rebase": "luna:high",
                "fix-gate": { "agent": "claude-code", "model": "our-proxy-model", "effort": "high" }
            },
            "permitted_hands": ["sonnet", "luna"]
        }))
        .unwrap();
        assert_eq!(
            work.hands["default"],
            HandPin::Named {
                id: "sonnet".to_string(),
                effort: None
            }
        );
        assert_eq!(
            work.hands["rebase"],
            HandPin::Named {
                id: "luna".to_string(),
                effort: Some("high".to_string())
            }
        );
        assert_eq!(
            work.hands["fix-gate"],
            HandPin::Spelled {
                agent: "claude-code".to_string(),
                model: "our-proxy-model".to_string(),
                effort: Some("high".to_string())
            }
        );
        assert_eq!(work.permitted_hands, ["sonnet", "luna"]);
        assert_eq!(work.hands["rebase"].describe(), "'luna' at effort 'high'");

        // The same table at site level, and a recipe pinning its own hand.
        let site: WorkConfig = serde_json::from_value(json!({
            "hands": { "default": "sonnet" },
            "recipes": [{ "id": "bench", "description": "d", "brief": "b", "hand": "gpt-5:high" }]
        }))
        .unwrap();
        assert_eq!(site.hands.len(), 1);
        assert_eq!(
            site.recipes[0].hand.as_ref().unwrap().effort(),
            Some("high")
        );
        // And it survives the round trip a recipe makes through JSON.
        assert_eq!(
            serde_json::to_value(&site.recipes[0]).unwrap()["hand"],
            json!("gpt-5:high")
        );

        // Absence is the ordinary case: no table anywhere names nobody.
        assert!(WorkConfig::default().hands.is_empty());
        assert!(ProjectWorkConfig::default().permitted_hands.is_empty());
    }

    /// What a hand is not: the runtime's own selector, half a pair, or a
    /// spelling with nothing after the colon. Each fails at the parse, where
    /// the person can still see what they wrote.
    #[test]
    fn what_cannot_be_a_hand_fails_loudly() {
        for text in [
            "claude-code[yolo]:anthropic:sonnet",
            "sonnet:",
            ":high",
            "  ",
        ] {
            assert!(HandPin::parse(text).is_err(), "{text}");
        }
        assert!(serde_json::from_value::<HandPin>(json!({ "agent": "codex" })).is_err());
        assert!(serde_json::from_value::<HandPin>(json!({ "model": "m5" })).is_err());
        assert!(
            serde_json::from_value::<HandPin>(json!({ "agent": "codex", "modle": "m5" })).is_err()
        );
        assert!(serde_json::from_value::<ProjectWorkConfig>(json!({ "hand": "sonnet" })).is_err());
    }

    #[test]
    fn a_recipe_typo_fails_loudly() {
        assert!(serde_json::from_value::<Recipe>(
            json!({ "id": "x", "description": "d", "brief": "b", "kinds": ["pr"] })
        )
        .is_err());
    }
}
