# Architectural spec

Internals — *how* this project is built. One file per spec; each H1 declares an `AR-NNN-<slug>` ID and the body is its contract. Citations from elsewhere in the tree (`§AR-NNN-<slug>.<section>`) resolve into these files.

An architectural spec may live inline in the class- or module-level doc-comment of the file it describes. A one-line stub here whose H1 is `# AR-NNN-<slug>: [<path>](<path>)` is **optional** — add it when you want the inline spec to appear in the index below; omit it when the doc-comment alone is enough. `grund <ID>` resolves the ID either way.

By convention every file under this directory — each file-form spec and each stub you chose to write — is linked from this README. Inline specs without a stub are not listed here and that is fine. Extra prose, recommended reading order, and conceptual groupings are welcome around the link set.

| ID | Subject |
|---|---|
| [AR-001-layers](AR-001-layers.md) | five layers, one binary, core depends on nothing |
| [AR-002-summons](AR-002-summons.md) | one executor runs everything ephor asks of the world |
| [AR-003-attribution](AR-003-attribution.md) | one matching engine, evidence against identity |
| [AR-004-forest](AR-004-forest.md) | git as substrate, a project as a forest folded over |
| [AR-005-capabilities](AR-005-capabilities.md) | availability computed once, consulted everywhere |
| [AR-006-matters](AR-006-matters.md) | the core types of the watch |
| [AR-007-runtime](AR-007-runtime.md) | the runtime adapter, rhei as its shipped binding |
| [AR-008-pipeline](AR-008-pipeline.md) | the engine as seven stages |

This index is navigational — citations should target the spec ID directly, never this file.
