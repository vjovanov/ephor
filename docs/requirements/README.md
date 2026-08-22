# Laws

Standing requirements — *laws* every change observes, distinct from the
behavior specs in [requirements.md](../../requirements.md): an FS says what a
feature does, a REQ constrains every feature there is. One file per law; each
H1 declares a `REQ-NNN-<slug>` ID and the body is its contract. Citations
(`§REQ-NNN-<slug>.<section>`) resolve into these files.

Laws are few by design. A candidate that applies to one feature is an FS
point; only a rule that must hold at every edge of the system earns a file
here.

| ID | Subject |
|---|---|
| [REQ-001-boundary](REQ-001-boundary.md) | every capacity ephor lacks crosses a seam |
| [REQ-002-parity](REQ-002-parity.md) | every ability is reachable without the screen |

This index is navigational — citations should target the law's ID directly,
never this file.
