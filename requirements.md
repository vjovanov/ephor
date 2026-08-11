# Requirements

Declare each behavior or requirement inline as an H2: `## FS-NNN-slug: …`.

## FS-001-forge-interface: ephor reaches every forge and issue tracker through one provider interface

ephor aggregates work from places that host code review and places that track
issues. Which ones they are is a property of a person's employer, not of ephor.
No forge, tracker, or vendor CLI may therefore be named in ephor's core: every
one of them is reached through a single interface with a fixed capability set,
and an implementation is selected per project by configuration.

### 1. Capabilities

An implementation answers some or all of the following; it declares which, and
ephor degrades to what is answered rather than failing.

- **Pull requests by role** — the ones the user authored, and the ones they are
  engaged in as a reviewer. Each carries a stable id, title, url, state, head
  branch, and last-updated time.
- **Conversation** — a PR's threads as ordered messages (author, text, time),
  each with its reactions, and enough identity for a reaction to be posted back
  to it.
- **Reactions** — post a reaction on a message, where the implementation
  supports writing; read-only implementations say so and their messages are
  display-only.
- **Gate status** — the job counts (passed, failed, running) for a pull
  request, per repository the gate covers, since one change may gate across
  several repositories at once.
- **Issues by role** — the ones the user opened, and the ones they are engaged
  in without having opened them. Each is a ticket by key, with title, url,
  state as the forge spells it, last-updated time, and its comments in the same
  message shape as a PR conversation. Where a forge can be searched as a whole,
  an implementation reports the user's issues wherever they live rather than
  only in a configured set of repositories: an issue filed against someone
  else's project is theirs to follow just as much as one on their own. State is
  reported whatever it is, closed included — an issue's closing is often the
  activity worth seeing.

### 2. Two transports, one interface

An implementation is reached either **in process**, as Rust implementing the
interface, or **out of process**, as an executable ephor runs. The two are the
same interface: the capability set of §1 defines a set of data types, the
in-process form passes them directly and the out-of-process form passes their
JSON serialization, and both are produced from one definition so they cannot
drift. Which transport an implementation uses is invisible above the interface —
the policy that turns forge data into feed items runs identically over both.

Out of process is the low-ceremony path and needs no Rust: an executable named
for the forge, resolved on `PATH`, answering a fixed set of subcommands with
JSON on stdout and receiving its configuration on stdin. A shell script with
`jq` is a complete implementation. In process is for implementations that want
Rust's typing; because Rust has no stable plugin ABI, an in-process
implementation outside this repository means depending on ephor as a library
and building a binary that registers it — not a dynamically loaded object.

### 3. Policy lives above the interface, never in an implementation

An implementation answers questions about a forge. It does not decide what the
answers mean. Whether a citation was answered, whether an item needs a
response, how threads and gate counts roll up, how items match registry
branches, and what counts as unread are ephor's, applied identically to every
implementation — so the feed stays coherent across forges, and an
implementation stays small enough to be a shell script.

### 4. Site-specific implementations ship separately

An implementation for a private forge — such as one reaching an internal
Bitbucket Server, Jira, and Buildbot through a vendor CLI — lives outside the
default build and is neither a build-time nor a run-time dependency of it. Its
vendor CLI name, host names, project keys, and repository names are
configuration it reads, never identifiers in ephor's source.

### 5. No site-specific data in the repository

The registry and feed configuration a person runs ephor with is their own: it
names their employer's repositories, hosts, and accounts. The repository
carries example configuration only, and a published artifact must contain
nothing else.

*Not satisfied today* — see
[§RM-001-forge-interface](docs/roadmap.md#rm-001-forge-interface-put-every-forge-behind-the-interface).

## FS-002-release: ephor releases from a tag, with a changelog entry per change

Versions are semver, and a version exists exactly when a `vX.Y.Z` tag does. The
version in `Cargo.toml` and the tag agree or the release refuses to run.

### 1. Changelog

[docs/changelog.md](docs/changelog.md) holds `## Unreleased` and the most
recent release inline; older releases move one-per-file under
`docs/changelog/` with a one-line pointer, so the common question — what
changed lately — is one file deep. Sections per release are the
Keep-a-Changelog set. Every pull request adds a bullet under `## Unreleased`
naming its own number, checked in CI.

### 2. Cutting a release

Promoting `## Unreleased` into a numbered release, bumping the manifest, and
tagging is done by workflow, not by hand: a patch release on a schedule when
main has shipped observable changes and its CI is green, and a minor release on
demand. Each first runs the whole release on a candidate branch with publishing
disabled, and only fast-forwards main if that dry run passed.

### 3. Artifacts

A release publishes the crate and a self-checked binary per supported target,
each built profile-guided, archived with its `sha256`, and attached to a GitHub
release whose notes are the changelog section for that version. Re-running a
partially-failed release skips what already exists rather than failing.

### 4. Publication is gated on carrying nothing site-specific

No artifact is published while the tree still violates
[§FS-001-forge-interface.5](#5-no-site-specific-data-in-the-repository). The
check is mechanical and runs before anything is uploaded.

## FS-003-feed-categories: the feed sorts itself into categories, and finished work lands in Recent

A feed is read by scanning, not by searching, so items arrive already sorted
into the categories a person works in. The categories are ephor's, never a
provider's — a provider reports items and ephor places them
([§FS-001-forge-interface.3](#3-policy-lives-above-the-interface-never-in-an-implementation))
— so every forge lands in the same categories and a new implementation
inherits them without asking.

### 1. The categories

An item belongs to exactly one category, chosen by its kind and by the user's
role on it:

| Category | Holds |
| --- | --- |
| Status | project status lines |
| My Pull Requests | pull requests the user authored |
| Reviewing | pull requests the user is on as a reviewer |
| CI | gate and build results |
| My Issues | issues the user opened |
| Participating | issues the user is in but did not open |
| Messages | conversations that are not attached to a pull request or issue |
| Recent | finished items — see [§2](#2-recent) |

Exactly one, so that the size of a category is the size of that pile of work
and not a double count.

### 2. Recent

Work does not stop mattering the moment it is finished. An item whose state is
terminal — closed, merged, done, resolved, declined, however its forge spells
it — leaves its category and appears under **Recent** for as long as its last
activity falls inside the recency window; past that it leaves the feed
entirely. Being closed is itself activity: an issue closed with no reply shows
up under Recent precisely because closing it was the answer.

Finished work never awaits a response. Whatever its conversation looks like —
someone else had the last word, the user was named and never answered — a
finished item is news and not a task, and nothing that counts work left to do
counts it.

### 3. The recency window is configured

How long finished work stays interesting is a property of a person, not of
ephor: the window is `defaults.recent_days` in the feed configuration, in
days, defaulting to 7. Zero drops an item from the feed the moment it is
finished, which is the behavior for someone who never looks back.
