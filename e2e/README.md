# e2e

One executable scenario per seam. A case is a story about a person and a
project — a forge nobody built in, a repository checking itself, a plan
directory full of tickets — run against the real binary in a world built for it
and thrown away afterwards.

Each case is a file under `cases/`, and each file **is** the `E2E-NNN-…`
declaration in the grund tree: the module doc-comment states the scenario and
cites the `§FS` point it holds ephor to (`grund list --kind E2E`). A case that
cites nothing is a test; a case that cites its spec point is a scenario, and the
spec point can be traced to it with `grund refs`.

## Running them

They are ordinary cargo test targets, declared by path in `Cargo.toml`, so they
run with everything else:

```sh
just check                      # the whole gate, e2e included
just e2e                        # only the scenarios
cargo test --test e2e_004_ticket_store   # one of them
```

## What a case may use

The world (`support.rs`) is a temporary directory holding the forest, the
registry, the site configuration, the feed cache, and a `PATH` of stubs.
Nothing reads the machine the case runs on, which is what makes it a scenario
rather than a test of one laptop.

The bindings are stubs on purpose. A forge, a check verb, a gate and an agent
runtime are all commands ephor summons, so a shell script standing in for one
exercises the whole seam with no forge, no CI system, and no agent anywhere
near it — and if a twenty-line script is a complete implementation, the
interface is the thing it claims to be.

A case drives `ephor` the way a person does. Where a seam has no command line of
its own yet, it drives the seam directly from the library and says so in the
declaration, rather than pretending a surface exists.

## Adding one

1. Write `cases/E2E-NNN-<slug>.rs`, opening with the declaration and its `§FS`
   citation.
2. Add the matching `[[test]]` entry to `Cargo.toml` (`name` is the file name,
   lowercased, dashes as underscores).
3. `just check` — `grund check` holds the new declaration to the E2E→FS rule.
