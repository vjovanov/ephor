//! §FS-001-forge-interface.3: what forge data *means*.
//!
//! Every implementation, in either transport, answers the same questions; this
//! module turns those answers into feed items by one set of rules, so two
//! forges never disagree about what "needs a response" means. It is also what
//! keeps an implementation small: an implementation reports, it does not judge.

use serde_json::{json, Value};

use super::{Issue, Message, Notice, PullRequest, Reason, Role, SubjectKind, Thread};
use crate::feed::model::{Item, ItemKind, ItemRole};

/// Review states that mean the author has work to do. Matched as substrings
/// because forges spell them differently (`CHANGES_REQUESTED`, `NEEDS_WORK`).
const AUTHOR_MUST_ACT: [&str; 3] = ["changes_requested", "needs_work", "rejected"];

/// Reasons for a notice that mean somebody is waiting, as opposed to being
/// kept informed (§FS-001-forge-interface.1). Matched as substrings against the
/// canonical form, so `team_mention` is a mention and a forge that spells one
/// of these with its own prefix still lands correctly.
const NOTICE_AWAITS: [&str; 5] = [
    "mention",
    "review-requested",
    "assign",
    "ci_activity",
    "security_alert",
];

/// One vocabulary for one fact. Each forge has its own word for being asked to
/// review — GitHub's `review_requested`, GitLab's todo of the same name — and
/// ephor has a third in [`Reason`]. A row that carries two spellings of one
/// fact reads as two facts, and a merged row carries exactly that, so the word
/// is settled here rather than at the source (§FS-001-forge-interface.3). A
/// reason ephor has no word for is passed through as the forge said it: it is
/// still worth showing, and inventing a translation would be worse than
/// quoting.
fn canonical_reason(reason: &str) -> String {
    match reason {
        "author" | "authored" => Reason::Authored.label().to_string(),
        "mention" | "mentioned" | "directly_addressed" => Reason::Mentioned.label().to_string(),
        "review_requested" | "review-requested" => Reason::ReviewRequested.label().to_string(),
        "assign" | "assigned" => Reason::Assigned.label().to_string(),
        other => other.to_string(),
    }
}

/// A conversation awaits the user when its last message is someone else's and
/// the user has not reacted to it. Reacting counts as answering — often it is
/// the honest answer.
///
/// A task the forge tracks outranks both, in either direction
/// (§FS-003-feed-categories.4): an unresolved one is work left however the
/// conversation ended, and a resolved one is an answer even though a robot had
/// the last word. Without this a bot checklist is pending forever — its last
/// message is never the reader's and never will be.
fn thread_pending(thread: &Thread) -> bool {
    if thread.messages.iter().any(Message::open_task) {
        return true;
    }
    match thread.messages.last() {
        Some(last) if last.resolved_task() => false,
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

/// Which side of the review the user is on. Reported reasons decide it where
/// there are any — opening it outranks everything else that can also be true
/// of the same pull request — and `role` is the fallback for an implementation
/// with no notion of reasons (§FS-001-forge-interface.3).
fn role_of(pr: &PullRequest) -> Role {
    if pr.reasons.is_empty() {
        return pr.role;
    }
    if pr.reasons.contains(&Reason::Authored) {
        Role::Author
    } else {
        Role::Reviewer
    }
}

/// The user is named in it, however the implementation said so.
fn cited(pr: &PullRequest) -> bool {
    pr.cited || pr.reasons.contains(&Reason::Mentioned)
}

/// The reason a row leads with, most demanding first: what was asked of the
/// reader outranks what they happen to be near. The author's own pull request
/// leads with its review state instead, which is the thing they are waiting on.
fn leading_reason(pr: &PullRequest) -> Option<Reason> {
    if role_of(pr) == Role::Author {
        return None;
    }
    [
        Reason::ReviewRequested,
        Reason::Mentioned,
        Reason::Assigned,
        Reason::InThread,
    ]
    .into_iter()
    .find(|reason| pr.reasons.contains(reason))
}

/// The state a row shows: what the forge reported, and — where the user is not
/// the author — why the pull request is theirs, which is the question a
/// reviewing row has to answer first. Composed here rather than in an
/// implementation, so every forge spells it the same way
/// (§FS-001-forge-interface.3). An implementation that already said it in its
/// own state is not made to say it twice.
fn compose_state(pr: &PullRequest) -> Option<String> {
    let lead = leading_reason(pr);
    match (pr.state.as_deref(), lead) {
        (Some(state), Some(reason)) if !state.contains(reason.label()) => {
            Some(format!("{state}:{}", reason.label()))
        }
        (Some(state), _) => Some(state.to_string()),
        (None, Some(reason)) => Some(reason.label().to_string()),
        (None, None) => None,
    }
}

/// Whether an item needs the user's response, by role.
fn needs_response(pr: &PullRequest) -> bool {
    // A review asked for and not yet given is the plainest case of somebody
    // waiting, and the one no conversation rule can find: being asked leaves
    // no message behind, so a pull request nobody has spoken in looks exactly
    // like one with nothing to do (§FS-001-forge-interface.1).
    if pr.reasons.contains(&Reason::ReviewRequested) {
        return true;
    }
    let state = pr.state.as_deref().unwrap_or("").to_lowercase();
    let author_must_act = AUTHOR_MUST_ACT.iter().any(|needle| state.contains(needle));
    let pending = pr.threads.iter().any(thread_pending);
    if cited(pr) {
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
    if !message.task.is_null() {
        value["task"] = message.task.clone();
    }
    value
}

fn threads_json(threads: &[Thread]) -> Value {
    Value::Array(
        threads
            .iter()
            .filter(|thread| !thread.messages.is_empty())
            .map(|thread| {
                let mut value =
                    json!({ "messages": thread.messages.iter().map(message_json).collect::<Vec<_>>() });
                // What the channel can carry travels with the conversation, so
                // a surface knows whether an answer can be sent from here
                // (§FS-007-matters.4).
                if !thread.reply.is_null() {
                    value["reply"] = thread.reply.clone();
                }
                value
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
    // Every reason, not just the one the state leads with: the row shows one,
    // and a reader asking why a pull request is in their feed at all deserves
    // the whole answer (§FS-001-forge-interface.1).
    if !pr.reasons.is_empty() {
        let mut reasons: Vec<Reason> = pr.reasons.clone();
        reasons.sort_unstable();
        reasons.dedup();
        raw.insert(
            "reasons".to_string(),
            json!(reasons
                .iter()
                .map(|reason| reason.label())
                .collect::<Vec<_>>()),
        );
    }

    let mut item = Item {
        id: format!("{forge}:{}", pr.id),
        project: project.to_string(),
        source: forge.to_string(),
        kind: ItemKind::Pr,
        role: Some(role_of(pr).into()),
        title: pr.title.clone(),
        url: pr.url.clone(),
        state: compose_state(pr),
        needs_response: needs_response(pr),
        updated_at: pr.updated_at,
        raw: Value::Object(raw),
    };
    settle(&mut item);
    item
}

/// One notice as a feed item (§FS-001-forge-interface.1).
///
/// A notice takes the kind its subject implies rather than a kind of its own,
/// so a pull request ephor heard about only because the forge mentioned it
/// lands under Reviewing with the rest of them — and merges with the fuller
/// report when another source also found it
/// (§FS-003-feed-categories.5). What has no subject ephor models is a message:
/// a release, an advisory, an invitation is still something addressed to the
/// reader (§FS-003-feed-categories.1).
pub fn notice_item(forge: &str, project: &str, notice: &Notice) -> Item {
    let reason = canonical_reason(&notice.reason.to_lowercase());
    let kind = match notice.subject {
        SubjectKind::PullRequest => ItemKind::Pr,
        SubjectKind::Issue => ItemKind::Issue,
        SubjectKind::Other => ItemKind::Message,
    };
    // Having opened it is the one reason that puts the reader on the authoring
    // side; everything else is somebody else's work they have been drawn into.
    let role = match kind {
        ItemKind::Message => None,
        _ if reason == Reason::Authored.label() => Some(ItemRole::Author),
        _ => Some(ItemRole::Reviewer),
    };

    let mut raw = serde_json::Map::new();
    if let Some(repo) = &notice.repo {
        raw.insert("repo".to_string(), json!(repo));
    }
    raw.insert("reasons".to_string(), json!([reason]));
    // A notice is the thinnest report there is: nothing but a title and a
    // reason. Saying so is what lets a fuller report of the same subject win
    // the merge without the rule having to know which sources exist.
    raw.insert("notice".to_string(), json!(true));

    // Subject identity where the forge gave one, so the merge has something
    // exact to match on, and the notice's own id where it did not
    // (§FS-003-feed-categories.5).
    let key = match (&notice.repo, &notice.number) {
        (Some(repo), Some(number)) => format!("{repo}#{number}"),
        _ => notice.id.clone(),
    };

    let mut item = Item {
        id: format!("{forge}:{key}"),
        project: project.to_string(),
        source: forge.to_string(),
        kind,
        role,
        title: notice.title.clone(),
        url: notice.url.clone(),
        state: (!reason.is_empty()).then(|| reason.clone()),
        needs_response: !notice.read && NOTICE_AWAITS.iter().any(|needle| reason.contains(needle)),
        updated_at: notice.updated_at,
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

/// Merging is the model's (§AR-006-matters.2): these tests build reports with
/// the constructors above and read what the reader would be shown, which is
/// the merged matters rendered back to the flat shape.
#[cfg(test)]
fn merge_reports(reports: Vec<Item>) -> Vec<Item> {
    crate::matter::merge(reports.iter().map(crate::matter::Matter::of_item).collect())
        .into_iter()
        .map(|matter| matter.as_item())
        .collect()
}

/// Whether a source counts an issue nobody has taken as work awaiting somebody
/// (§FS-003-feed-categories.4).
///
/// Configuration rather than a rule, and the spec says why: *unclaimed* only
/// means *yours* where the reader is answerable for the backlog. On a project
/// they run it is the whole point; among issues they once commented on
/// somewhere it would turn every stranger's open bug into their work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Unclaimed {
    /// An unclaimed issue is an ordinary issue: the conversation decides.
    #[default]
    Ignored,
    /// An issue nobody has taken awaits somebody, however the talk ended.
    Awaits,
}

/// One issue as a feed item. An issue awaits the user when its last comment is
/// someone else's — the same rule a conversation follows — and, where the
/// source asked for it, when nobody has taken the issue at all
/// (§FS-003-feed-categories.4). Issues are their own category, split by role
/// like pull requests (§FS-003-feed-categories.1).
pub fn issue_item(forge: &str, project: &str, issue: &Issue, unclaimed: Unclaimed) -> Item {
    let thread = Thread {
        messages: issue.messages.clone(),
        ..Thread::default()
    };
    // Only `Some(false)` is a claim that nobody has it. `None` is an
    // implementation with no notion of assignment, and reading that as
    // unclaimed would invent work out of a silence (§FS-001-forge-interface.1).
    let unclaimed = unclaimed == Unclaimed::Awaits && issue.assigned == Some(false);
    let pending = unclaimed || thread_pending(&thread);
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
            task: Value::Null,
            mine,
        }
    }

    /// A task message as a forge reports one: nobody's own, and carrying the
    /// state that decides whether it is still work.
    fn task(state: &str) -> Message {
        Message {
            task: json!({ "state": state, "comment": 7 }),
            ..message("Bot", false)
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
            reasons: Vec::new(),
            cited,
            threads,
            gate: None,
        }
    }

    fn reported(reasons: Vec<Reason>, state: &str) -> PullRequest {
        PullRequest {
            reasons,
            ..pr(Vec::new(), false, state, Role::Author)
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
            ..Thread::default()
        };
        assert!(needs_response(&pr(
            vec![theirs],
            false,
            "open",
            Role::Reviewer
        )));

        let mine = Thread {
            messages: vec![message("them", false), message("me", true)],
            ..Thread::default()
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
            ..Thread::default()
        };
        assert!(!needs_response(&pr(
            vec![reacted],
            true,
            "open",
            Role::Reviewer
        )));
    }

    /// The bot-checklist shape: a prompt and a task under it, neither of them
    /// ever the reader's. Ticked, the thread is done — read by last word
    /// alone it would await an answer that can never arrive
    /// (§FS-003-feed-categories.4).
    #[test]
    fn a_ticked_task_answers_a_thread_nobody_of_mine_spoke_in() {
        let ticked = Thread {
            messages: vec![message("Bot", false), task("resolved")],
            ..Thread::default()
        };
        assert!(!needs_response(&pr(
            vec![ticked],
            false,
            "open",
            Role::Author
        )));

        let unticked = Thread {
            messages: vec![message("Bot", false), task("open")],
            ..Thread::default()
        };
        assert!(needs_response(&pr(
            vec![unticked],
            false,
            "open",
            Role::Author
        )));
    }

    /// State outranks the last word in the other direction too: replying is
    /// not ticking, and a box left open is work the reader still owes.
    #[test]
    fn an_open_task_awaits_me_even_when_i_had_the_last_word() {
        let answered_but_unticked = Thread {
            messages: vec![task("open"), message("me", true)],
            ..Thread::default()
        };
        assert!(needs_response(&pr(
            vec![answered_but_unticked],
            false,
            "open",
            Role::Author
        )));
    }

    /// A state no forge of ephor's acquaintance uses is unfinished work, not
    /// silently finished work.
    #[test]
    fn an_unknown_task_state_is_not_ticked() {
        let odd = Thread {
            messages: vec![task("pending-review")],
            ..Thread::default()
        };
        assert!(needs_response(&pr(vec![odd], false, "open", Role::Author)));
    }

    /// The descriptor survives into the item the thread screen reads, or the
    /// box could not be drawn or ticked (§FS-004-quick-actions.5).
    #[test]
    fn a_task_descriptor_reaches_the_item() {
        let item = pull_request_item(
            "forge",
            "widget",
            &pr(
                vec![Thread {
                    messages: vec![task("open")],
                    ..Thread::default()
                }],
                false,
                "open",
                Role::Author,
            ),
        );
        assert_eq!(
            item.raw.pointer("/threads/0/messages/0/task"),
            Some(&json!({ "state": "open", "comment": 7 }))
        );
    }

    /// The gap this whole capability exists for (§FS-001-forge-interface.1):
    /// being asked for a review leaves nothing behind in the conversation, so
    /// every rule that reads the conversation says there is nothing to do.
    #[test]
    fn a_review_asked_for_awaits_an_answer_in_a_pull_request_nobody_has_spoken_in() {
        let asked = reported(vec![Reason::ReviewRequested], "open");
        assert!(asked.threads.is_empty());
        assert!(needs_response(&asked));

        // Merely being near it is not being asked.
        assert!(!needs_response(&reported(vec![Reason::InThread], "open")));
        assert!(!needs_response(&reported(vec![Reason::Assigned], "open")));
    }

    /// Opening it outranks everything else that may also be true, and the
    /// reason the reader is on somebody else's pull request leads the state.
    #[test]
    fn reasons_decide_the_role_and_lead_the_state() {
        let mine = reported(vec![Reason::Authored, Reason::Assigned], "open");
        assert_eq!(role_of(&mine), Role::Author);
        // An author's state is their review decision, not why it is theirs.
        assert_eq!(compose_state(&mine).as_deref(), Some("open"));

        let asked = reported(vec![Reason::InThread, Reason::ReviewRequested], "open");
        assert_eq!(role_of(&asked), Role::Reviewer);
        assert_eq!(
            compose_state(&asked).as_deref(),
            Some("open:review-requested")
        );

        // An implementation that composed the reason into its own state is
        // not made to say it twice.
        let already = reported(vec![Reason::Mentioned], "open:mentioned");
        assert_eq!(compose_state(&already).as_deref(), Some("open:mentioned"));

        // Reporting no reasons at all leaves `role` and `state` as they were,
        // which is what an implementation with no notion of them relies on.
        let plain = pr(Vec::new(), false, "open", Role::Reviewer);
        assert_eq!(role_of(&plain), Role::Reviewer);
        assert_eq!(compose_state(&plain).as_deref(), Some("open"));

        // Every reason reaches the item, not only the one that leads.
        let item = pull_request_item("forge", "widget", &asked);
        assert_eq!(
            item.raw["reasons"],
            json!(["in-thread", "review-requested"])
        );
        assert_eq!(item.role, Some(ItemRole::Reviewer));
    }

    /// §FS-003-feed-categories.5: the fuller report is the row, and what the
    /// thinner one knew — here, that a team was named, which no search would
    /// ever have found — survives the merge.
    #[test]
    fn one_subject_reported_twice_is_one_row_that_keeps_both_reasons() {
        let mut full = pull_request_item(
            "github-prs",
            "widget",
            &PullRequest {
                reasons: vec![Reason::InThread],
                threads: vec![Thread {
                    messages: vec![message("them", false), message("me", true)],
                    ..Thread::default()
                }],
                ..pr(Vec::new(), false, "open", Role::Reviewer)
            },
        );
        full.id = "github-prs:acme/widget#42".to_string();
        full.raw["repo"] = json!("acme/widget");

        let notice = notice_item(
            "github-notifications",
            "widget",
            &Notice {
                id: "thread-9".to_string(),
                title: "Widen the retry window".to_string(),
                url: None,
                reason: "team_mention".to_string(),
                subject: SubjectKind::PullRequest,
                repo: Some("acme/widget".to_string()),
                number: Some("42".to_string()),
                updated_at: Utc::now(),
                read: false,
            },
        );
        assert!(notice.needs_response);

        let merged = merge_reports(vec![full, notice]);
        assert_eq!(merged.len(), 1);
        let row = &merged[0];
        // The report the reader can act on wins the row …
        assert_eq!(row.source, "github-prs");
        assert!(row.raw.get("threads").is_some());
        assert!(row.raw.get("notice").is_none());
        // … and the reason only the thin one knew comes with it, along with
        // the fact that somebody is waiting.
        assert_eq!(row.raw["reasons"], json!(["in-thread", "team_mention"]));
        assert!(row.needs_response);
    }

    /// Two subjects that merely look alike stay two rows: identity is what the
    /// forge stated, never the title (§FS-003-feed-categories.5).
    #[test]
    fn reports_of_different_subjects_are_never_merged() {
        let one = notice(SubjectKind::PullRequest, Some("42"), "mention");
        let two = notice(SubjectKind::PullRequest, Some("43"), "mention");
        // Same repository, same title, different number.
        assert_eq!(merge_reports(vec![one, two]).len(), 2);

        // A subject with no number of its own cannot be matched against
        // anything, so it stands alone rather than being guessed at.
        let anonymous = notice(SubjectKind::Other, None, "subscribed");
        let other = notice(SubjectKind::Other, None, "subscribed");
        assert_eq!(merge_reports(vec![anonymous, other]).len(), 1);
    }

    /// A notice takes the kind of the thing it is about, so it lands in the
    /// category that thing belongs to (§FS-003-feed-categories.1) — and what
    /// ephor models no capability for is still a message to the reader.
    #[test]
    fn a_notice_takes_the_kind_of_its_subject() {
        let pull = notice(SubjectKind::PullRequest, Some("42"), "review_requested");
        assert_eq!(pull.kind, ItemKind::Pr);
        assert_eq!(pull.role, Some(ItemRole::Reviewer));
        assert_eq!(pull.id, "github-notifications:acme/widget#42");
        assert!(pull.needs_response);

        let release = notice(SubjectKind::Other, None, "subscribed");
        assert_eq!(release.kind, ItemKind::Message);
        assert_eq!(release.role, None);
        // Being kept informed is not being asked.
        assert!(!release.needs_response);

        // GitHub's `team_mention` is a mention; a reason ephor has never seen
        // still arrives, it simply does not flag.
        assert!(notice(SubjectKind::Issue, Some("7"), "team_mention").needs_response);
        assert!(!notice(SubjectKind::Issue, Some("7"), "state_change").needs_response);

        // The forge's own record of having read it settles the question, so
        // clearing a notification elsewhere clears the flag here.
        let read = notice_item(
            "github-notifications",
            "widget",
            &Notice {
                read: true,
                ..raw_notice(SubjectKind::PullRequest, Some("42"), "mention")
            },
        );
        assert!(!read.needs_response);
    }

    /// One fact, one word for it. Without this the row a merge produced showed
    /// `review-requested` and `review_requested` side by side and read as two
    /// separate things having been asked of the reader.
    #[test]
    fn one_fact_is_said_in_one_vocabulary() {
        assert_eq!(canonical_reason("review_requested"), "review-requested");
        assert_eq!(canonical_reason("assign"), "assigned");
        assert_eq!(canonical_reason("mention"), "mentioned");
        assert_eq!(canonical_reason("author"), "authored");
        // GitLab words its todo for the same fact differently again.
        assert_eq!(canonical_reason("directly_addressed"), "mentioned");
        // A reason ephor has no word for is quoted, not translated.
        assert_eq!(canonical_reason("team_mention"), "team_mention");
        assert_eq!(canonical_reason("security_alert"), "security_alert");

        let searched = pull_request_item(
            "github-prs",
            "widget",
            &PullRequest {
                reasons: vec![Reason::ReviewRequested],
                ..pr(Vec::new(), false, "open", Role::Reviewer)
            },
        );
        let told = notice(SubjectKind::PullRequest, Some("1"), "review_requested");
        let mut same = searched;
        same.id = "github-prs:acme/widget#1".to_string();
        same.raw["repo"] = json!("acme/widget");

        let merged = merge_reports(vec![same, told]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].raw["reasons"], json!(["review-requested"]));
    }

    fn raw_notice(subject: SubjectKind, number: Option<&str>, reason: &str) -> Notice {
        Notice {
            id: format!("thread-{reason}"),
            title: "Widen the retry window".to_string(),
            url: None,
            reason: reason.to_string(),
            subject,
            repo: Some("acme/widget".to_string()),
            number: number.map(String::from),
            updated_at: Utc::now(),
            read: false,
        }
    }

    fn notice(subject: SubjectKind, number: Option<&str>, reason: &str) -> Item {
        notice_item(
            "github-notifications",
            "widget",
            &raw_notice(subject, number, reason),
        )
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
                ..Thread::default()
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
            blocked: true,
            blockers: vec!["Requires approvals".to_string()],
        });

        let item = pull_request_item("demo-forge", "widget", &request);
        assert_eq!(item.id, "demo-forge:app/1");
        assert_eq!(item.raw["branch"], "you/ABC-42");
        assert_eq!(item.raw["repo"], "app");
        assert_eq!(item.raw["threads"][0]["messages"][0]["author"], "them");
        assert_eq!(item.raw["gate"]["repos"][0]["passed"], 3);
        // The verdict rides along with the counts: it is the half of the gate
        // the counts cannot express (§FS-001-forge-interface.1).
        assert_eq!(item.raw["gate"]["blocked"], true);
        assert_eq!(item.raw["gate"]["blockers"][0], "Requires approvals");
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
            assigned: None,
            messages: vec![message("them", false)],
        };
        let item = issue_item("tracker", "widget", &issue, Unclaimed::Ignored);
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
            assigned: None,
            messages: vec![message("me", true), message("them", false)],
        };
        let item = issue_item("github-issues", "widget", &issue, Unclaimed::Ignored);
        assert!(item.is_finished());
        assert!(!item.needs_response);

        // The same rule for a pull request whose review asked for changes.
        let mut merged = pr(Vec::new(), false, "merged:changes_requested", Role::Author);
        merged.state = Some("merged".to_string());
        assert!(!pull_request_item("forge", "widget", &merged).needs_response);
    }

    /// An issue nobody has taken awaits somebody however the conversation
    /// ended, and the conversation is exactly what cannot say so
    /// (§FS-003-feed-categories.4): an issue somebody filed and nobody picked
    /// up has its author's word last, so every other rule reads it as
    /// answered.
    #[test]
    fn an_unclaimed_issue_awaits_where_the_source_asked_for_it() {
        let unclaimed = Issue {
            key: "acme/widget#7".to_string(),
            title: "Retry window".to_string(),
            status: Some("open".to_string()),
            url: None,
            updated_at: Utc::now(),
            role: Role::Author,
            assigned: Some(false),
            // The reader's own issue, never answered: silent in the most
            // misleading way available.
            messages: vec![message("me", true)],
        };
        assert!(
            !issue_item("github-issues", "widget", &unclaimed, Unclaimed::Ignored).needs_response
        );
        assert!(
            issue_item("github-issues", "widget", &unclaimed, Unclaimed::Awaits).needs_response
        );

        // Somebody has it: it is being handled, and the conversation decides
        // as it always did.
        let taken = Issue {
            assigned: Some(true),
            ..unclaimed.clone()
        };
        assert!(!issue_item("github-issues", "widget", &taken, Unclaimed::Awaits).needs_response);

        // An implementation with no notion of assignment never invents work:
        // `None` is a claim nobody made, not "nobody has it"
        // (§FS-001-forge-interface.1).
        let unsaid = Issue {
            assigned: None,
            ..unclaimed.clone()
        };
        assert!(!issue_item("github-issues", "widget", &unsaid, Unclaimed::Awaits).needs_response);

        // And finished work asks nothing of anyone, claimed or not
        // (§FS-003-feed-categories.2).
        let closed = Issue {
            status: Some("closed".to_string()),
            ..unclaimed.clone()
        };
        assert!(!issue_item("github-issues", "widget", &closed, Unclaimed::Awaits).needs_response);
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
            assigned: None,
            messages: Vec::new(),
        };
        let item = issue_item("github-issues", "widget", &issue, Unclaimed::Ignored);
        assert_eq!(item.role, Some(crate::feed::model::ItemRole::Reviewer));
        // Closed, so it belongs under Recent rather than Participating.
        assert!(item.is_finished());
    }
}

#[cfg(test)]
mod dissolve_tests {
    use super::*;
    use crate::feed::model::{ItemKind, ItemRole};
    use chrono::Utc;

    fn report(source: &str, role: Option<ItemRole>, raw: Value) -> Item {
        Item {
            id: format!("{source}:acme/widget#42"),
            project: "widget".to_string(),
            source: source.to_string(),
            kind: ItemKind::Pr,
            role,
            title: "Retry window".to_string(),
            url: Some("https://github.com/acme/widget/pull/42".to_string()),
            state: role.map(|_| "open".to_string()),
            needs_response: false,
            updated_at: Utc::now(),
            raw,
        }
    }

    fn thread(author: &str, text: &str) -> Value {
        json!({ "messages": [{ "author": author, "text": text, "when": "2026-08-01T09:00:00Z" }] })
    }

    /// A source that only reads review threads reports the pull request they
    /// are on, and the conversation it found survives the merge even though
    /// its report is the thinner one (§FS-003-feed-categories.5,
    /// §FS-007-matters.3).
    #[test]
    fn a_thinner_report_still_hands_over_the_conversation_it_alone_saw() {
        let rich = report(
            "github-prs",
            Some(ItemRole::Author),
            json!({ "repo": "acme/widget", "branch": "you/ABC-42", "reasons": ["authored"] }),
        );
        let threads = report(
            "github-threads",
            None,
            json!({ "repo": "acme/widget", "threads": [thread("ada", "why this way?")] }),
        );

        let merged = merge_reports(vec![rich, threads]);
        assert_eq!(merged.len(), 1, "one subject is one row");
        let kept = &merged[0];
        // The richer report is the row…
        assert_eq!(kept.source, "github-prs");
        assert_eq!(kept.role, Some(ItemRole::Author));
        // …and the conversation only the other one saw came with it.
        let carried = kept.raw.get("threads").and_then(Value::as_array).unwrap();
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0]["messages"][0]["author"], "ada");
    }

    /// Both sources watch the same pull request, so they report the same
    /// review thread. The reader is shown it once.
    #[test]
    fn the_same_conversation_reported_twice_is_one_conversation() {
        let shared = thread("ada", "why this way?");
        let rich = report(
            "github-prs",
            Some(ItemRole::Author),
            json!({ "repo": "acme/widget", "threads": [shared.clone()] }),
        );
        let threads = report(
            "github-threads",
            None,
            json!({ "repo": "acme/widget", "threads": [shared, thread("bo", "and this?")] }),
        );

        let merged = merge_reports(vec![rich, threads]);
        let carried = merged[0]
            .raw
            .get("threads")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(carried.len(), 2, "the duplicate folded, the new one kept");
        assert_eq!(carried[0]["messages"][0]["author"], "ada");
        assert_eq!(carried[1]["messages"][0]["author"], "bo");
    }

    /// A gate is an observation of the pull request, so the report that read
    /// the checks lands its gate on the row whichever report wins.
    #[test]
    fn the_gate_reaches_the_row_from_whichever_report_read_it() {
        let rich = report(
            "github-prs",
            Some(ItemRole::Author),
            json!({ "repo": "acme/widget", "branch": "you/ABC-42" }),
        );
        let gate = json!({ "repos": [{ "repo": "acme/widget", "passed": 1, "failed": 2 }] });
        let ci = report(
            "github-ci",
            None,
            json!({ "repo": "acme/widget", "gate": gate }),
        );

        let merged = merge_reports(vec![rich, ci]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, "github-prs");
        let gate = crate::feed::gate::Gate::of(&merged[0]).expect("the gate came with it");
        assert!(gate.is_red());
        assert_eq!(gate.failed(), 2);
    }
}
