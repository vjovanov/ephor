//! §FS-001-forge-interface.3: what forge data *means*.
//!
//! Every implementation, in either transport, answers the same questions; this
//! module turns those answers into feed items by one set of rules, so two
//! forges never disagree about what "needs a response" means. It is also what
//! keeps an implementation small: an implementation reports, it does not judge.

use serde_json::{json, Value};

use super::{Issue, Message, PullRequest, Thread};
use crate::feed::model::{Item, ItemKind};

/// Review states that mean the author has work to do. Matched as substrings
/// because forges spell them differently (`CHANGES_REQUESTED`, `NEEDS_WORK`).
const AUTHOR_MUST_ACT: [&str; 3] = ["changes_requested", "needs_work", "rejected"];

/// A conversation awaits the user when its last message is someone else's and
/// the user has not reacted to it. Reacting counts as answering — often it is
/// the honest answer.
fn thread_pending(thread: &Thread) -> bool {
    match thread.messages.last() {
        Some(last) => !last.mine && !last.reactions.iter().any(|reaction| reaction.mine),
        None => false,
    }
}

/// A citation is answered by a later message of the user's in the same thread,
/// or by their reaction on the citing message. A citation with no thread at all
/// — one living in the pull request body — is answered once the user has said
/// anything anywhere in the conversation.
fn citation_answered(threads: &[Thread]) -> bool {
    let recorded = threads.iter().any(|thread| !thread.messages.is_empty());
    // With no conversation recorded there is nothing to have answered with, so
    // a citation stands; otherwise it is answered once the user has had the
    // last word — or reacted — everywhere.
    recorded && !threads.iter().any(thread_pending)
}

/// Whether an item needs the user's response, by role.
fn needs_response(pr: &PullRequest) -> bool {
    let state = pr.state.as_deref().unwrap_or("").to_lowercase();
    let author_must_act = AUTHOR_MUST_ACT.iter().any(|needle| state.contains(needle));
    let pending = pr.threads.iter().any(thread_pending);
    if pr.cited {
        return !citation_answered(&pr.threads);
    }
    author_must_act || pending
}

fn message_json(message: &Message) -> Value {
    let mut value = json!({
        "author": message.author,
        "text": message.text,
        "when": message.when.map(|when| when.to_rfc3339()).unwrap_or_default(),
    });
    if !message.reactions.is_empty() {
        value["reactions"] = Value::Array(
            message
                .reactions
                .iter()
                .map(|reaction| json!({ "emoji": reaction.emoji, "users": reaction.users }))
                .collect(),
        );
    }
    if !message.react.is_null() {
        value["react"] = message.react.clone();
    }
    value
}

fn threads_json(threads: &[Thread]) -> Value {
    Value::Array(
        threads
            .iter()
            .filter(|thread| !thread.messages.is_empty())
            .map(|thread| {
                json!({ "messages": thread.messages.iter().map(message_json).collect::<Vec<_>>() })
            })
            .collect(),
    )
}

/// One pull request as a feed item. `forge` names the implementation and
/// prefixes the item id, which is the unread-tracking key.
pub fn pull_request_item(forge: &str, project: &str, pr: &PullRequest) -> Item {
    let mut raw = serde_json::Map::new();
    if let Some(branch) = &pr.branch {
        raw.insert("branch".to_string(), json!(branch));
    }
    raw.insert("repo".to_string(), json!(pr.repo));
    let threads = threads_json(&pr.threads);
    if threads.as_array().is_some_and(|list| !list.is_empty()) {
        raw.insert("threads".to_string(), threads);
    }
    if let Some(gate) = &pr.gate {
        if !gate.is_empty() {
            raw.insert("gate".to_string(), gate.to_value());
        }
    }

    let mut item = Item {
        id: format!("{forge}:{}", pr.id),
        project: project.to_string(),
        source: forge.to_string(),
        kind: ItemKind::Pr,
        role: Some(pr.role.into()),
        title: pr.title.clone(),
        url: pr.url.clone(),
        state: pr.state.clone(),
        needs_response: needs_response(pr),
        updated_at: pr.updated_at,
        raw: Value::Object(raw),
    };
    settle(&mut item);
    item
}

/// Finished work is news, not a task (§FS-003-feed-categories.2): whoever had
/// the last word, a merged or closed item asks nothing of anyone.
fn settle(item: &mut Item) {
    if item.is_finished() {
        item.needs_response = false;
    }
}

/// One issue as a feed item. An issue awaits the user when its last comment is
/// someone else's — the same rule a conversation follows. Issues are their own
/// category, split by role like pull requests
/// (§FS-003-feed-categories.1).
pub fn issue_item(forge: &str, project: &str, issue: &Issue) -> Item {
    let thread = Thread {
        messages: issue.messages.clone(),
    };
    let pending = thread_pending(&thread);
    let threads = threads_json(std::slice::from_ref(&thread));
    let raw = if threads.as_array().is_some_and(|list| list.is_empty()) {
        Value::Null
    } else {
        json!({ "threads": threads })
    };

    // The key leads the title; the status is not repeated into it, because
    // every renderer already shows `state` beside the title.
    let mut item = Item {
        id: format!("{forge}:{}", issue.key),
        project: project.to_string(),
        source: forge.to_string(),
        kind: ItemKind::Issue,
        role: Some(issue.role.into()),
        title: format!("{} {}", issue.key, issue.title),
        url: issue.url.clone(),
        state: issue.status.as_ref().map(|status| status.to_lowercase()),
        needs_response: pending,
        updated_at: issue.updated_at,
        raw,
    };
    settle(&mut item);
    item
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Role;
    use chrono::Utc;

    fn message(author: &str, mine: bool) -> Message {
        Message {
            author: author.to_string(),
            text: "…".to_string(),
            when: None,
            reactions: Vec::new(),
            react: Value::Null,
            mine,
        }
    }

    fn pr(threads: Vec<Thread>, cited: bool, state: &str, role: Role) -> PullRequest {
        PullRequest {
            id: "app/1".to_string(),
            repo: "app".to_string(),
            number: "1".to_string(),
            title: "t".to_string(),
            url: None,
            branch: None,
            updated_at: Utc::now(),
            role,
            state: Some(state.to_string()),
            cited,
            threads,
            gate: None,
        }
    }

    #[test]
    fn an_author_acts_on_changes_requested_however_the_forge_spells_it() {
        for state in ["open:changes_requested", "open:needs_work", "OPEN:REJECTED"] {
            assert!(
                needs_response(&pr(Vec::new(), false, state, Role::Author)),
                "{state}"
            );
        }
        assert!(!needs_response(&pr(
            Vec::new(),
            false,
            "open",
            Role::Author
        )));
    }

    #[test]
    fn a_thread_whose_last_word_is_not_mine_awaits_me() {
        let theirs = Thread {
            messages: vec![message("me", true), message("them", false)],
        };
        assert!(needs_response(&pr(
            vec![theirs],
            false,
            "open",
            Role::Reviewer
        )));

        let mine = Thread {
            messages: vec![message("them", false), message("me", true)],
        };
        assert!(!needs_response(&pr(
            vec![mine],
            false,
            "open",
            Role::Reviewer
        )));
    }

    #[test]
    fn reacting_answers_as_well_as_replying() {
        let mut last = message("them", false);
        last.reactions = vec![super::super::Reaction {
            emoji: "👍".to_string(),
            users: vec!["me".to_string()],
            mine: true,
        }];
        let reacted = Thread {
            messages: vec![message("me", true), last],
        };
        assert!(!needs_response(&pr(
            vec![reacted],
            true,
            "open",
            Role::Reviewer
        )));
    }

    #[test]
    fn a_citation_with_no_conversation_still_awaits_an_answer() {
        assert!(needs_response(&pr(
            Vec::new(),
            true,
            "open",
            Role::Reviewer
        )));
    }

    #[test]
    fn items_carry_the_branch_repo_threads_and_gate() {
        let mut request = pr(
            vec![Thread {
                messages: vec![message("them", false)],
            }],
            false,
            "open",
            Role::Author,
        );
        request.branch = Some("you/ABC-42".to_string());
        request.gate = Some(crate::feed::gate::Gate {
            repos: vec![crate::feed::gate::RepoGate {
                repo: "app".to_string(),
                passed: 3,
                failed: 0,
                running: 1,
            }],
        });

        let item = pull_request_item("demo-forge", "widget", &request);
        assert_eq!(item.id, "demo-forge:app/1");
        assert_eq!(item.raw["branch"], "you/ABC-42");
        assert_eq!(item.raw["repo"], "app");
        assert_eq!(item.raw["threads"][0]["messages"][0]["author"], "them");
        assert_eq!(item.raw["gate"]["repos"][0]["passed"], 3);
        assert_eq!(item.kind, ItemKind::Pr);
    }

    #[test]
    fn an_issue_awaits_me_when_the_last_comment_is_not_mine() {
        let issue = Issue {
            key: "ABC-42".to_string(),
            title: "Retry window".to_string(),
            status: Some("In Progress".to_string()),
            url: None,
            updated_at: Utc::now(),
            role: Role::Author,
            messages: vec![message("them", false)],
        };
        let item = issue_item("tracker", "widget", &issue);
        assert_eq!(item.id, "tracker:ABC-42");
        // The status lives in `state`, which renderers show; it is not also
        // baked into the title.
        assert_eq!(item.title, "ABC-42 Retry window");
        assert_eq!(item.state.as_deref(), Some("in progress"));
        assert!(item.needs_response);
        assert_eq!(item.kind, ItemKind::Issue);
        assert_eq!(item.role, Some(crate::feed::model::ItemRole::Author));
    }

    /// §FS-003-feed-categories.2: a closed item asks nothing of anyone, even
    /// when someone else had the last word in it.
    #[test]
    fn finished_work_never_needs_a_response() {
        let issue = Issue {
            key: "acme/widget#7".to_string(),
            title: "Hyperlinks".to_string(),
            status: Some("closed".to_string()),
            url: None,
            updated_at: Utc::now(),
            role: Role::Author,
            messages: vec![message("me", true), message("them", false)],
        };
        let item = issue_item("github-issues", "widget", &issue);
        assert!(item.is_finished());
        assert!(!item.needs_response);

        // The same rule for a pull request whose review asked for changes.
        let mut merged = pr(Vec::new(), false, "merged:changes_requested", Role::Author);
        merged.state = Some("merged".to_string());
        assert!(!pull_request_item("forge", "widget", &merged).needs_response);
    }

    /// An issue the user did not open is theirs to follow, but under
    /// Participating rather than My Issues (§FS-003-feed-categories.1).
    #[test]
    fn an_issue_carries_the_role_the_implementation_reported() {
        let issue = Issue {
            key: "acme/widget#7".to_string(),
            title: "Hyperlinks".to_string(),
            status: Some("closed".to_string()),
            url: None,
            updated_at: Utc::now(),
            role: Role::Reviewer,
            messages: Vec::new(),
        };
        let item = issue_item("github-issues", "widget", &issue);
        assert_eq!(item.role, Some(crate::feed::model::ItemRole::Reviewer));
        // Closed, so it belongs under Recent rather than Participating.
        assert!(item.is_finished());
    }
}
