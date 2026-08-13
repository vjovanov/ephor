//! Recipes: which items deserve work, and what the ticket asks for
//! (§FS-005-dispatch.1).
//!
//! A recipe is a selector and a brief. The selector is ephor's own vocabulary
//! — kind, role, gate, whether a response is owed — so one recipe matches the
//! same items whichever forge reported them
//! (§FS-001-forge-interface.3). The brief is the reader's, rendered with the
//! item's fields where it names them.

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
}

impl Default for WorkConfig {
    fn default() -> Self {
        WorkConfig {
            root: default_root(),
            states: None,
            runner: None,
            recipes: Vec::new(),
        }
    }
}

fn default_root() -> String {
    "{workspace}/panta".to_string()
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
    /// What the ticket asks for, in the reader's words. `{...}` placeholders
    /// are filled from the item (see [`super::dossier::Subject::placeholders`]),
    /// plus `{reply}` — where a proposed answer for this matter belongs, which
    /// ephor reads back and offers beside the conversation
    /// (§FS-005-dispatch.13).
    pub brief: String,
    /// Pin the runtime's execution identity for this ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

fn default_icon() -> String {
    "◆".to_string()
}

fn default_state() -> String {
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
    /// `pr`, `ci`, `issue`, `message`, `status`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    /// `author`, `reviewer`. An item whose source reported no role matches
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
}

/// What a selector asks about that the item does not carry: facts measured in
/// the reader's own checkout.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    /// Commits the item's branch trails its main branch; None where it could
    /// not be measured.
    pub behind: Option<u64>,
}

impl Selector {
    /// Whether this selector holds for an item. Public because the same
    /// language selects menu offers, whoever wrote them
    /// (§FS-006-project-interface.9).
    pub fn matches(&self, item: &Item, facts: &Facts) -> bool {
        if let Some(want) = self.behind {
            match facts.behind {
                Some(behind) => {
                    if (behind > 0) != want {
                        return false;
                    }
                }
                None => return false,
            }
        }
        if !self.kinds.is_empty() && !self.kinds.iter().any(|kind| kind_matches(item.kind, kind)) {
            return false;
        }
        if !self.roles.is_empty() && !self.roles.iter().any(|role| role_matches(item.role, role)) {
            return false;
        }
        if !self.sources.is_empty() && !self.sources.contains(&item.source) {
            return false;
        }
        if let Some(needs_response) = self.needs_response {
            if item.needs_response != needs_response {
                return false;
            }
        }
        match self.gate.as_deref() {
            None => true,
            Some(want) => gate_matches(Gate::of(item), want),
        }
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
        brief: brief.to_string(),
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
        recipe(
            "rebase",
            "⤴",
            "rebase onto the main branch",
            // Replaying a branch happens where the branch is.
            true,
            Selector {
                kinds: vec!["pr".to_string()],
                roles: vec!["author".to_string()],
                // Only where the branch has actually fallen behind: a recipe
                // offered on a change that is already current is a ticket to
                // do nothing (§FS-004-quick-actions.6). It is last of the
                // shipped recipes because a red gate or an owed answer is the
                // more urgent thing about the same pull request.
                behind: Some(true),
                ..Selector::default()
            },
            "{title} is on {branch}, which has fallen behind its main branch.\n\n\
             Run `ephor rebase --checkout {workspace}` first — it fetches and replays every\n\
             repository in the checkout, and it is not a judgment call\n\
             (§FS-005-dispatch.12). Where a conflict is already standing in the working\n\
             tree, that is what this ticket is about and the report above says which files.\n\n\
             Resolve each conflict as the change itself would have been written against\n\
             the new base — take neither side on principle, work out what the two commits\n\
             were each trying to do, and where that is not decidable from the code, stop\n\
             and say so rather than guessing. `git add` what you resolved and\n\
             `git rebase --continue` until the replay finishes.\n\n\
             Then check the result: build or test what the conflicting files belong to, so\n\
             that \"it rebased\" is not the same claim as \"it still works\". Do not push.",
        ),
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
        assert!(!ids_with(&recipes, &mine, Facts { behind: Some(0) }).contains(&rebase));
        assert!(ids_with(&recipes, &mine, Facts { behind: Some(3) }).contains(&rebase));

        // Someone else's change is not mine to replay.
        let theirs = item(ItemKind::Pr, Some(ItemRole::Reviewer));
        assert!(!ids_with(&recipes, &theirs, Facts { behind: Some(3) }).contains(&rebase));

        // A red gate on the same pull request is the more urgent thing about
        // it, and a sweep takes the first match.
        let failing = with_gate(item(ItemKind::Pr, Some(ItemRole::Author)), 2, false);
        assert_eq!(
            ids_with(&recipes, &failing, Facts { behind: Some(3) }).first(),
            Some(&"fix-gate".to_string())
        );

        // Merged, and behind by a mile: there is nothing to replay onto.
        let mut merged = item(ItemKind::Pr, Some(ItemRole::Author));
        merged.state = Some("merged".to_string());
        assert!(ids_with(&recipes, &merged, Facts { behind: Some(9) }).is_empty());
    }

    #[test]
    fn a_selector_can_ask_for_a_branch_that_is_level_with_main() {
        let level = Selector {
            behind: Some(false),
            ..Selector::default()
        };
        let pr = item(ItemKind::Pr, None);
        assert!(level.matches(&pr, &Facts { behind: Some(0) }));
        assert!(!level.matches(&pr, &Facts { behind: Some(2) }));
        // Unmeasurable answers neither question.
        assert!(!level.matches(&pr, &Facts::default()));
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

    #[test]
    fn a_recipe_typo_fails_loudly() {
        assert!(serde_json::from_value::<Recipe>(
            json!({ "id": "x", "description": "d", "brief": "b", "kinds": ["pr"] })
        )
        .is_err());
    }
}
