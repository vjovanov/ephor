# FS-012-file-size: every file is measured against a budget set by how it is read

A file costs whoever opens it, and what it costs depends on how it is opened. A
`§`-declared document is reached by an ID and fetched a section at a time, so its
length is charged to no single read. `CLAUDE.md` is loaded whole into every
session, so every line of it is charged to every session. A source file opened to
change one function is charged whole, once. Those are three different costs, and
one line budget over the whole tree would be wrong for all three.

So this tree is measured, mechanically, and the budget follows the reader rather
than the file extension. Nothing here asks for a smaller repository. A file over
its budget is usually a file that has taken on a second subject, and what is owed
is the seam it is missing — never a trim to fit.

## 1. The budget follows the reader

- A **citable spec** — a `§`-declared document, this file among them — is reached
  by an ID and read a lead, a section, or a map at a time, so its length is not
  charged to every read the way a source file's is. What still costs is one
  document covering two subjects, and that is what its budget names.
- An **entrypoint** — `README.md`, `CLAUDE.md`, a skill's `SKILL.md` — is
  addressed by nothing and therefore read whole, into every session that loads
  it. Its budget is a fraction of the citable tree's, for exactly that reason.
- **The manual** is neither. Nothing reaches into it by `§`, so a reader who
  wants one field pays for the file; but it is deliberately one document,
  rendered as one page, and its unit of growth is the chapter rather than the
  line. It gets a rule of its own.
- **The changelog** is append-only and is not line-measured at all. A line budget
  on one only asks it to cut a release in half, and it already carries the
  bounding that means something: each release rotates out to its own file when
  the next ships ([§FS-002-release.1](FS-002-release.md#1-changelog)).
- **Code** — sources, tests, scripts, workflows — is opened whole by whoever
  changes it, and is budgeted per tree from what that tree measures.
- **Shipped artifacts** — the published schemas and the worked example
  configurations — are content, not composition. They are bounded in bytes and
  never hand-trimmed: an example somebody copies whole loses the thing it
  demonstrates when it is cut to fit a line count.

## 2. The limits

Two tiers per rule. **Soft** warns and lets the commit through; **hard** refuses
it. Line counts exclude blank and comment lines, so a citation or a rationale in
a doc comment is free — a budget that taxed comments would pay an agent to delete
the grounding this tree exists to keep.

| what | soft | hard |
| --- | --- | --- |
| citable spec | 750 lines | 2000 lines |
| entrypoint | 250 lines | 500 lines |
| the manual | 2000 lines | 3200 lines |
| source under `src/` | 800 lines | 2000 lines |
| tests outside a source file | 700 lines | 2000 lines |
| the Python suite | 450 lines | 700 lines |
| a script | 300 lines | 500 lines |
| a CI workflow | 300 lines | 600 lines |
| the changelog, schemas, examples | bytes only | bytes only |

`.agents/fissile.toml` is where each of these is configured, and it carries the
derivation for every one — what this tree measures, and which number it is taken
from. Read it before changing a limit: a limit moved to clear a finding is a
budget that has stopped meaning anything.

## 3. The gate

`fissile check` runs in the commit hook against the files a commit touches, and
in CI against the whole tree. It is not advisory: a hard overflow fails the
build, and the tree passes or a recorded exception says why it does not.

## 4. An overflow is recorded with the boundary it is missing

A file that cannot meet its budget is written down, not argued in a commit
message. The record says which of two things is true, and it is refused if it
says neither: **structural**, that splitting is illegal, naming the constraint;
or **deferred**, that a boundary is missing, naming the boundary and the
condition that retires the entry. Soft entries are an agent's to add; hard
entries are a human's.

"This file is long" is not a reason, and neither is "splitting it would be work".
The entry has to name the seam — which is why writing one is usually more
expensive than taking it.
