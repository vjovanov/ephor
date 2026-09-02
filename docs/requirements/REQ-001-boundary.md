# REQ-001-boundary: every capacity ephor lacks crosses a seam, and the seam has one anatomy

ephor observes and summons; it never governs ([§GRUND-001-overseer](../grund.md#grund-001-overseer-one-watch-over-every-project-and-none-of-the-governing)). Every
capacity it does not embody — a forge's answers, a project's checks, a gate's
restart, a runtime's execution — is reached across a **seam**, and every seam
is built the same way. This law is what keeps ephor publishable, keeps every
watched project clean, and keeps every default replaceable
([§GOAL-005-costless](../goals.md#goal-005-costless-watching-costs-the-watched-nothing)). It binds every feature, every adapter, and every future
seam; work near an edge cites it.

## 1. The anatomy

A seam has four parts, and a seam missing any of them is not done:

- **A contract in materials.** What crosses is files, commands, environment
  variables, and exit codes — never linked code. The other side of a seam can
  always be a shell script.
- **A configured binding.** Which implementation fills the seam is a choice,
  recorded somewhere a person can point to — a registry row, a site
  configuration field, a manifest entry — never an assumption compiled in.
- **A shipped default or worked example.** The seam works out of the box:
  the default implementation or a complete example ships *with* ephor. It is
  never fused *into* ephor — replacing it must require configuration, not a
  fork.
- **A degrade rule.** What happens when the seam is unbound is stated and
  visible: a narrower feed, a refused menu entry with the reason, tickets
  that wait on disk. Absence is never an error and never silent.

## 2. Three homes, one resolution order

Everything a seam needs to know lives in one of three places. **Description
and identity** live in the registry row — what a project is, where it is,
and the signals by which its matters are recognized. **Operational
bindings** live in site configuration — which command fills which verb,
which runtime runs work, what a person has chosen. **Conventions** are
probed in the checkout — well-known names a project carries for its own
sake. Resolution is always *site configuration over manifest over probe*:
probing is defaulting, a manifest is the project declaring what probes would
have guessed, and site configuration is the person overriding both.

## 3. Requirements on a project are capabilities, never artifacts

ephor may require a watched project to *be* things — reachable through a
source, a git forest on disk, checkable by command. It may never require a
project to *contain* ephor-specific things. The test for any proposed
requirement: if a file or behavior would only make sense because ephor
exists, it cannot be required of the project — it may only be offered by the
project or configured at the site. A project that offers nothing is fully
watchable; every rung it does hold buys features by the seam's degrade rule,
and no rung ever demands an artifact.

## 4. The footprint rule

Where ephor writes into a checkout, its presence is confined and disposable:
a work root the repository ignores, a generated file a template produced.
Deleting ephor's traces leaves a clean project; deleting a project's
registry row leaves a clean ephor. Neither side holds the other hostage.

## 5. No product literal outside its adapter

The name of a forge, vendor CLI, runtime, or task store appears only in
its own adapter, in shipped assets and examples, and in documentation —
never in core source. This is checked mechanically, not observed as a
convention: the check fails the build, the same way the site-word check
guards [§FS-001-forge-interface.5](../functional-spec/FS-001-forge-interface.md#5-no-site-specific-data-in-the-repository).
