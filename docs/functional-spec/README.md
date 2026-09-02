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

This index is navigational — citations should target the declaration's ID
directly, never this file.
