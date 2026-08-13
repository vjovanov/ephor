//! Tasks on thread messages (§FS-004-quick-actions.5): reading the box out of
//! a message's `task` descriptor, and ticking it back through the source that
//! reported it.
//!
//! A forge that tracks tasks — a checklist item, a blocker comment, a review
//! task — puts a `task` descriptor on the message carrying one, e.g.
//! `{"state": "open", "repo": "app", "pull_request": "101", "comment": "c-7"}`.
//! Everything but `state` is that implementation's own and travels back to it
//! untouched (§FS-001-forge-interface.1); `state` is the one field ephor
//! reads, because whether the box is ticked is what the reader sees and what
//! decides whether the thread still awaits them (§FS-003-feed-categories.4).

use serde_json::Value;

use crate::error::{EphorError, Result};
use crate::feed::config::Defaults;
use crate::feed::providers::forge_call;
use crate::forge::task_resolved;

/// A task on a message: its state, and who to send a transition to.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub resolved: bool,
    pub source: String,
    pub target: Value,
}

impl Task {
    /// The box as it renders beside the message. The two glyphs are the same
    /// width, so ticking one does not reflow the text beside it.
    pub fn box_glyph(&self) -> &'static str {
        if self.resolved {
            "☑"
        } else {
            "☐"
        }
    }
}

/// Parse a message's `task` descriptor, if it carries one. `source` is the
/// item's source — the implementation that reported the task is the one that
/// can transition it.
pub fn parse(message: &Value, source: &str) -> Option<Task> {
    let task = message.get("task").filter(|task| !task.is_null())?;
    Some(Task {
        resolved: task_resolved(task),
        source: source.to_string(),
        target: task.clone(),
    })
}

/// Tick a task, through the forge that reported it.
pub fn resolve(task: &Task, blocks: &[Value], project: &str, defaults: &Defaults) -> Result<()> {
    let (forge, request) = forge_call(blocks, &task.source, project, defaults)
        .map_err(|err| EphorError::Command(err.to_string()))?;
    forge
        .resolve_task(&request, &task.target)
        .map_err(|err| EphorError::Command(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_message_without_a_task_has_no_box() {
        assert_eq!(parse(&json!({ "author": "a" }), "gdev"), None);
        assert_eq!(parse(&json!({ "task": null }), "gdev"), None);
    }

    /// Only `resolved` is ticked. A forge reporting a state ephor has not seen
    /// is reporting work left to do, which is the answer that shows the reader
    /// something rather than quietly clearing it.
    #[test]
    fn only_resolved_ticks_the_box() {
        let open = parse(
            &json!({ "task": { "state": "open", "comment": 7 } }),
            "gdev",
        )
        .unwrap();
        assert!(!open.resolved);
        assert_eq!(open.box_glyph(), "☐");
        assert_eq!(open.target, json!({ "state": "open", "comment": 7 }));

        let done = parse(&json!({ "task": { "state": "RESOLVED" } }), "gdev").unwrap();
        assert!(done.resolved);
        assert_eq!(done.box_glyph(), "☑");

        let odd = parse(&json!({ "task": { "state": "pending-review" } }), "gdev").unwrap();
        assert!(!odd.resolved);

        let stateless = parse(&json!({ "task": { "comment": 7 } }), "gdev").unwrap();
        assert!(!stateless.resolved);
    }
}
