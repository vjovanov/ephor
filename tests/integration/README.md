# integration

Integration tests prove the How: that the parts fit as designed. This is the
home of the non-citable `integration` kind — no file here carries an ID, and
`[citations.integration]` in `grund.toml` says the home should cite
`AR`. A test belongs here when its subject spans more than one part: a command
end to end against a real checkout and registry, or a repository-hygiene
script that holds the tree itself to a rule. A claim about one module is a
unit test beside it; black-box proof of a spec point is an e2e case under
`tests/e2e/`.

## Rust

`cargo test --all-targets`, declared by path in `Cargo.toml` because Cargo does
not auto-discover tests in a subdirectory. `common/` is the shared harness
(`mod common;`); `golden/` holds recorded provider responses fixtures compare
against.

- `check_test.rs` — `ephor check` end to end ([§FS-006-project-interface.5](../../docs/functional-spec/FS-006-project-interface.md#5-checks-are-verbs-and-every-script-is-self-contained)).
- `checkout_test.rs` — `ephor checkout` end to end ([§FS-004-quick-actions.7](../../docs/functional-spec/FS-004-quick-actions.md#7-a-workspace-that-is-not-there-is-offered-the-checkout)).
- `doctor_test.rs` — `ephor doctor` and `ephor capabilities` ([§FS-010-doctor](../../docs/functional-spec/FS-010-doctor.md#fs-010-doctor-ephor-can-be-asked-whether-it-still-works-and-answers-in-one-screen)).
- `rebase_test.rs` — `ephor rebase` end to end ([§FS-004-quick-actions.6](../../docs/functional-spec/FS-004-quick-actions.md#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)).
- `work_test.rs` — `ephor work` end to end ([§FS-005-dispatch](../../docs/functional-spec/FS-005-dispatch.md#fs-005-dispatch-what-ephor-watches-it-can-hand-to-an-agent-runtime)).
- `scope_test.rs` — the scope selectors across the registry and the site's
  watch list, over a world with two organizations
  ([§FS-011-command-line.9](../../docs/functional-spec/FS-011-command-line.md#9-a-scope-selector-is-honoured-or-refused), [§AR-009-surfaces.2](../../docs/architecture/AR-009-surfaces.md#2-the-session-is-built-once-and-shared)).
- `forge_extension_test.rs` — an out-of-process forge extension, a real shell
  script and nothing else ([§FS-001-forge-interface.2](../../docs/functional-spec/FS-001-forge-interface.md#2-two-transports-one-interface)).
- `agents_test.rs`, `feed_test.rs`, `registry_test.rs`, `update_test.rs` — the
  registry and feed engine driven through the CLI: project registration,
  fetch/refresh, and the AGENTS.md rendering path.

## Python

`python -m unittest discover -s tests/integration -p 'test_*.py'`, the same
line CI and the pre-commit hook run.

- `test_check_boundary.py` — the boundary check itself ([§REQ-001-boundary.5](../../docs/requirements/REQ-001-boundary.md#5-no-product-literal-outside-its-adapter)).
- `test_check_changelog_pr_entry.py`, `test_prepare_changelog_release.py` —
  the pull-request changelog gate and the release script
  ([§FS-002-release.1](../../docs/functional-spec/FS-002-release.md#1-changelog), [§FS-002-release](../../docs/functional-spec/FS-002-release.md#fs-002-release-ephor-releases-from-a-tag-with-a-changelog-entry-per-change)).

Unit tests stay beside the code under `code`'s rule; there is no third kind
for them.
