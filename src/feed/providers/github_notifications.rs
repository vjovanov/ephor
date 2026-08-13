//! GitHub's own notification list as notices (§FS-001-forge-interface.1) — the
//! source whose job is to be exhaustive.
//!
//! Every other GitHub provider asks a question ephor composed: these
//! repositories, these roles, these kinds of thing. Each of those questions can
//! be wrong in a way nobody notices, because the answer to a question never
//! asked looks exactly like an empty feed. This provider asks nothing: it reads
//! what GitHub decided to tell the user, whatever it is about — a team named in
//! a review comment, a discussion, a release, an advisory, a repository they
//! were invited to — and reports it.
//!
//! It is meant to overlap. A pull request another provider already found comes
//! back here too, and the two reports merge into one row carrying both reasons
//! (§FS-003-feed-categories.5), so the overlap costs the reader nothing and
//! buys the guarantee that the feed is not quietly missing a whole class of
//! work.

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::feed::provider::{
    command_exists, run_json, Provider, ProviderContext, ProviderError, ProviderResult,
};
use crate::feed::providers::{gh_command, parse_config, parse_github_time};
use crate::forge::{policy, Notice, SubjectKind};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[allow(dead_code)]
    provider: String,
    /// Repositories to keep, as `owner/name`. Empty — the default — keeps
    /// everything GitHub says, which is what makes this a net: the repository
    /// worth catching is the one nobody thought to configure. Naming
    /// repositories here is for a person who runs a provider per project and
    /// wants each project's notices in its own feed.
    #[serde(default)]
    repos: Vec<String>,
    /// Include notices GitHub considers already read. Off by default: reading
    /// a notification on GitHub is the reader saying they have dealt with it,
    /// and a feed that argues with that is a feed they have to clear twice.
    #[serde(default)]
    read: bool,
    /// Which of GitHub's reasons to keep. The default is the set that means
    /// somebody is waiting on *this reader in particular*.
    ///
    /// `assign` and `ci_activity` are deliberately not in it. Both are real
    /// reasons and both can be added, but on a busy account assignment is a
    /// bulk mechanism rather than a request — a repository that assigns every
    /// incoming contribution to its maintainers produces hundreds of them, and
    /// a net that catches all of those catches nothing, because nobody reads
    /// it. What is genuinely assigned to the reader is already reported by
    /// `github-prs` and `github-issues` under its own role, where it arrives
    /// with its conversation and its gate.
    ///
    /// Empty keeps every reason, which is the exhaustive reading of the
    /// capability and the right setting for an account quiet enough to afford
    /// it (§FS-001-forge-interface.1).
    #[serde(default = "default_reasons")]
    reasons: Vec<String>,
    /// How far back to look. Zero removes the bound.
    #[serde(default = "default_window_days")]
    updated_within_days: u64,
    /// The most notices this provider will report. Not a page size and not a
    /// trimming rule — it is a guard against a runaway list, and reaching it is
    /// a failure rather than a shorter answer
    /// (§FS-001-forge-interface.6).
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    host: Option<String>,
}

fn default_window_days() -> u64 {
    30
}

fn default_limit() -> usize {
    500
}

/// Somebody is waiting on this reader by name, or on a group they are in, or
/// the forge has something urgent to say about their code.
fn default_reasons() -> Vec<String> {
    [
        "mention",
        "team_mention",
        "review_requested",
        "security_alert",
    ]
    .map(str::to_string)
    .to_vec()
}

pub struct GithubNotifications {
    config: Config,
}

impl GithubNotifications {
    pub fn from_config(config: &Value) -> Result<Self, ProviderError> {
        Ok(GithubNotifications {
            config: parse_config(config)?,
        })
    }

    /// Where this host's pages live, as opposed to its API.
    fn web_base(&self) -> String {
        match self.config.host.as_deref() {
            Some(host) => format!("https://{host}"),
            None => "https://github.com".to_string(),
        }
    }

    fn fetch_threads(&self, ctx: &ProviderContext) -> Result<Vec<Value>, ProviderError> {
        let mut path = format!("notifications?per_page=100&all={}", self.config.read);
        if self.config.updated_within_days > 0 {
            let since = Utc::now() - Duration::days(self.config.updated_within_days as i64);
            path.push_str(&format!("&since={}", since.format("%Y-%m-%dT%H:%M:%SZ")));
        }
        let mut command = gh_command(self.config.host.as_deref());
        command.args(["api", &path, "--paginate"]);
        let response = run_json(command, ctx.timeout, false)?;
        Ok(response.as_array().cloned().unwrap_or_default())
    }
}

/// GitHub's subject types, mapped to the kinds ephor models. Everything else
/// is `Other` on purpose — a release, an advisory, an invitation has no
/// capability that would ever have asked for it, which is precisely why the
/// notice is the only way the reader hears about it.
fn subject_kind(subject_type: &str) -> SubjectKind {
    match subject_type {
        "PullRequest" => SubjectKind::PullRequest,
        "Issue" => SubjectKind::Issue,
        _ => SubjectKind::Other,
    }
}

/// The subject's number, read out of the API url GitHub gives for it
/// (`https://api.github.com/repos/acme/widget/pulls/42`). A subject with no
/// number — a release, a discussion — has none, and then the notice stands on
/// its own rather than being merged into somebody else's row
/// (§FS-003-feed-categories.5).
fn subject_number(subject: &Value) -> Option<String> {
    let url = subject.get("url").and_then(Value::as_str)?;
    let last = url.rsplit('/').next()?;
    (!last.is_empty() && last.bytes().all(|byte| byte.is_ascii_digit())).then(|| last.to_string())
}

/// Where the reader goes to deal with it. Built from the repository and the
/// subject rather than taken from the notification, which carries only API
/// urls; a subject ephor cannot place still gets its repository, which beats
/// a row that cannot be opened at all.
fn web_url(base: &str, repo: &str, kind: SubjectKind, number: Option<&str>) -> Option<String> {
    match (kind, number) {
        (SubjectKind::PullRequest, Some(number)) => Some(format!("{base}/{repo}/pull/{number}")),
        (SubjectKind::Issue, Some(number)) => Some(format!("{base}/{repo}/issues/{number}")),
        _ if !repo.is_empty() => Some(format!("{base}/{repo}")),
        _ => None,
    }
}

impl Provider for GithubNotifications {
    fn name(&self) -> &'static str {
        "github-notifications"
    }

    fn available(&self, _ctx: &ProviderContext) -> bool {
        command_exists("gh")
    }

    fn fetch(&self, ctx: &ProviderContext) -> ProviderResult {
        let threads = self.fetch_threads(ctx)?;
        let base = self.web_base();
        let mut items = Vec::new();

        for thread in threads {
            let reason = thread
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !self.config.reasons.is_empty() && !self.config.reasons.contains(&reason) {
                continue;
            }
            let repo = thread
                .pointer("/repository/full_name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if !self.config.repos.is_empty() && !self.config.repos.contains(&repo) {
                continue;
            }
            let Some(id) = thread.get("id").and_then(Value::as_str) else {
                continue;
            };
            let subject = thread.get("subject").cloned().unwrap_or(Value::Null);
            let kind = subject_kind(
                subject
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            );
            let number = subject_number(&subject);

            items.push(policy::notice_item(
                "github-notifications",
                &ctx.project_id,
                &Notice {
                    id: id.to_string(),
                    title: subject
                        .get("title")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    url: web_url(&base, &repo, kind, number.as_deref()),
                    reason,
                    subject: kind,
                    repo: (!repo.is_empty()).then(|| repo.clone()),
                    number,
                    updated_at: parse_github_time(thread.get("updated_at").unwrap_or(&Value::Null)),
                    read: !thread
                        .get("unread")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                },
            ));
        }

        // Every page is fetched, so nothing here is ever a partial answer —
        // and it must not become one. Past `limit` the source says so and
        // stops, rather than trimming an unknown fraction off a list whose
        // whole value is that the reader can believe it
        // (§FS-001-forge-interface.6).
        if items.len() >= self.config.limit {
            return Err(ProviderError(format!(
                "{} notices match, which is this source's `limit` — narrow `reasons` or \
                 `updated_within_days`, or raise `limit`, rather than be shown an unknown \
                 fraction of them",
                items.len()
            )));
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Everything GitHub says, by default: the repository worth catching is
    /// the one that was never configured.
    #[test]
    fn the_default_scope_is_every_repository_and_only_what_is_unread() {
        let provider = GithubNotifications::from_config(&json!({
            "provider": "github-notifications"
        }))
        .unwrap();
        assert!(provider.config.repos.is_empty());
        assert!(!provider.config.read);
        assert_eq!(provider.config.updated_within_days, 30);
        assert_eq!(provider.web_base(), "https://github.com");

        // Waiting on this reader by name, or on a group they are in. Not
        // `assign`: a repository that assigns every contribution to its
        // maintainers turns the net into a list nobody reads, and what is
        // genuinely assigned already arrives through `github-prs` and
        // `github-issues` with its conversation attached.
        assert_eq!(
            provider.config.reasons,
            [
                "mention",
                "team_mention",
                "review_requested",
                "security_alert"
            ]
        );
        assert!(!provider.config.reasons.iter().any(|r| r == "assign"));

        // Empty is the exhaustive reading, for an account quiet enough to
        // afford it.
        let everything = GithubNotifications::from_config(&json!({
            "provider": "github-notifications", "reasons": []
        }))
        .unwrap();
        assert!(everything.config.reasons.is_empty());

        let enterprise = GithubNotifications::from_config(&json!({
            "provider": "github-notifications", "host": "ghe.example.com"
        }))
        .unwrap();
        assert_eq!(enterprise.web_base(), "https://ghe.example.com");
    }

    /// A kind ephor models is placed as itself; everything else is still worth
    /// reporting, and is exactly what no other provider would ever return.
    #[test]
    fn a_subject_ephor_does_not_model_is_still_a_notice() {
        assert_eq!(subject_kind("PullRequest"), SubjectKind::PullRequest);
        assert_eq!(subject_kind("Issue"), SubjectKind::Issue);
        for exotic in ["Discussion", "Release", "RepositoryVulnerabilityAlert", ""] {
            assert_eq!(subject_kind(exotic), SubjectKind::Other, "{exotic}");
        }
    }

    #[test]
    fn the_number_and_the_page_come_out_of_the_api_url() {
        let pull = json!({ "url": "https://api.github.com/repos/acme/widget/pulls/42" });
        assert_eq!(subject_number(&pull).as_deref(), Some("42"));
        assert_eq!(
            web_url(
                "https://github.com",
                "acme/widget",
                SubjectKind::PullRequest,
                Some("42")
            )
            .as_deref(),
            Some("https://github.com/acme/widget/pull/42")
        );
        assert_eq!(
            web_url(
                "https://github.com",
                "acme/widget",
                SubjectKind::Issue,
                Some("7")
            )
            .as_deref(),
            Some("https://github.com/acme/widget/issues/7")
        );

        // A release has no number in its url, and a discussion has no url at
        // all — both still open somewhere the reader can act.
        let release = json!({ "url": "https://api.github.com/repos/acme/widget/releases/tag/v1" });
        assert_eq!(subject_number(&release), None);
        assert_eq!(subject_number(&json!({})), None);
        assert_eq!(
            web_url(
                "https://github.com",
                "acme/widget",
                SubjectKind::Other,
                None
            )
            .as_deref(),
            Some("https://github.com/acme/widget")
        );
        assert_eq!(
            web_url("https://github.com", "", SubjectKind::Other, None),
            None
        );
    }
}
