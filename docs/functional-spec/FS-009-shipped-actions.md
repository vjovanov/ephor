# FS-009-shipped-actions: what ephor ships for CI runs from the repository alone

ephor ships continuous-integration entry points — workflow steps a project
wires into its own CI — and every one of them obeys the rule that selects
them: **a shipped step runs from repository-committed material and workflow
inputs alone**, never from a personal site. A step that would need a
registry, credentials for a person's sources, or a person's bindings does
not ship as CI; the watch-and-work loop stays on machines that have a site,
and shipping it hosted would mean shipping someone's configuration
([§REQ-001-boundary.2](../requirements/REQ-001-boundary.md#2-three-homes-one-resolution-order)).

## 1. The set

Three steps ship. **Validate** checks a repository's `ephor.json` — and a
committed registry, where a repository carries one — against the published
schemas ([§FS-006-project-interface.11](FS-006-project-interface.md#11-the-interface-is-versioned)). **Check** reads the manifest's
declared check verbs and runs them, per-feature where features are
enumerated ([§FS-006-project-interface.5](FS-006-project-interface.md#5-checks-are-verbs-and-every-script-is-self-contained)) — the project's own gate derived
from the project's own declaration, with nothing project-specific in the
workflow. **Setup** installs a pinned ephor release, and is the building
block the other two and anyone's own composition stand on.

## 2. Versioned and released with ephor

The steps live in ephor's repository and version with it: a release that
changes a schema or a verb ships the steps that understand the change, per
[§FS-002-release](FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change). A repository pins the version it consumes, as it pins any
dependency ([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)).

