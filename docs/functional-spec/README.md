# Functional specification

Behavior and requirements, one declaration per file. Each H1 declares an
`FS-NNN-<slug>` ID and the body is its contract. Citations
(`§FS-NNN-<slug>.<section>`) resolve into these files.

| ID | Subject |
|---|---|
| [FS-001-forge-interface](FS-001-forge-interface.md) | ephor reaches every forge and issue tracker through one provider interface |
| [FS-002-release](FS-002-release.md) | ephor releases from a tag, with a changelog entry per change |
| [FS-003-feed-categories](FS-003-feed-categories.md) | the feed sorts itself into categories, and finished work lands in Recent |
| [FS-004-quick-actions](FS-004-quick-actions.md) | a problem ephor recognizes arrives with the action for it |
| [FS-005-dispatch](FS-005-dispatch.md) | what ephor watches, it can hand to an agent runtime |
| [FS-006-project-interface](FS-006-project-interface.md) | a project and ephor meet over one interface, in three homes |
| [FS-007-matters](FS-007-matters.md) | the feed is made of matters, and a matter knows why it is there |
| [FS-008-attribution](FS-008-attribution.md) | every conversation finds its project, or says that it could not |
| [FS-009-shipped-actions](FS-009-shipped-actions.md) | what ephor ships for CI runs from the repository alone |
| [FS-010-doctor](FS-010-doctor.md) | ephor can be asked whether it still works, and answers in one screen |
| [FS-011-command-line](FS-011-command-line.md) | every ability is a command, and every answer has a JSON form |
| [FS-012-file-size](FS-012-file-size.md) | every file is measured against a budget set by how it is read |
| [FS-013-burn](FS-013-burn.md) | what this machine spends on agents is a reading like any other |
| [FS-014-work-root-scopes](FS-014-work-root-scopes.md) | a plan lives in the smallest scope that can see everything it touches |
| [FS-015-spend-ceiling](FS-015-spend-ceiling.md) | what unattended work may spend is the person's number, and the sweep stops at it |
| [§FS-001-forge-interface](FS-001-forge-interface.md#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface) | ephor reaches every forge and issue tracker through one provider interface |
| [§FS-002-release](FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change) | ephor releases from a tag, with a changelog entry per change |
| [§FS-003-feed-categories](FS-003-feed-categories.md#fs-003-feed-categories-the-feed-sorts-itself-into-categories-and-finished-work-lands-in-recent) | the feed sorts itself into categories, and finished work lands in Recent |
| [§FS-004-quick-actions](FS-004-quick-actions.md#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it) | a problem ephor recognizes arrives with the action for it |
| [§FS-005-dispatch](FS-005-dispatch.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime) | what ephor watches, it can hand to an agent runtime |
| [§FS-006-project-interface](FS-006-project-interface.md#fs-006-project-interface-a-project-and-ephor-meet-over-one-interface-in-three-homes) | a project and ephor meet over one interface, in three homes |
| [§FS-007-matters](FS-007-matters.md#fs-007-matters-the-feed-is-made-of-matters-and-a-matter-knows-why-it-is-there) | the feed is made of matters, and a matter knows why it is there |
| [§FS-008-attribution](FS-008-attribution.md#fs-008-attribution-every-conversation-finds-its-project-or-says-that-it-could-not) | every conversation finds its project, or says that it could not |
| [§FS-009-shipped-actions](FS-009-shipped-actions.md#fs-009-shipped-actions-what-ephor-ships-for-ci-runs-from-the-repository-alone) | what ephor ships for CI runs from the repository alone |
| [§FS-010-doctor](FS-010-doctor.md#fs-010-doctor-ephor-can-be-asked-whether-it-still-works-and-answers-in-one-screen) | ephor can be asked whether it still works, and answers in one screen |
| [§FS-011-command-line](FS-011-command-line.md#fs-011-command-line-every-ability-is-a-command-and-every-answer-has-a-json-form) | every ability is a command, and every answer has a JSON form |
| [§FS-012-file-size](FS-012-file-size.md#fs-012-file-size-every-file-is-measured-against-a-budget-set-by-how-it-is-read) | every file is measured against a budget set by how it is read |
| [§FS-013-burn](FS-013-burn.md#fs-013-burn-what-this-machine-spends-on-agents-is-a-reading-like-any-other) | what this machine spends on agents is a reading like any other |
| [§FS-014-work-root-scopes](FS-014-work-root-scopes.md#fs-014-work-root-scopes-a-plan-lives-in-the-smallest-scope-that-can-see-everything-it-touches) | a plan lives in the smallest scope that can see everything it touches |
| [§FS-015-spend-ceiling](FS-015-spend-ceiling.md#fs-015-spend-ceiling-what-unattended-work-may-spend-is-the-persons-number-and-the-sweep-stops-at-it) | what unattended work may spend is the person's number, and the sweep stops at it |

This index is navigational — citations should target the declaration's ID
directly, never this file.
