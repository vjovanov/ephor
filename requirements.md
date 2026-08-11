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
  several repositories at once; and, where the forge reaches a verdict of its
  own, whether the gate blocks the merge and the reasons it gives for that.
  Counts alone cannot say it: a gate whose jobs are all green may still be
  blocked on an approval, on a downstream repository, or on jobs it has not
  started, and a row showing only what passed reads as finished work.
- **Failures** — for a pull request whose gate is red, what actually failed:
  each failure as the job that produced it, a link to its log, and the error
  text itself where the forge can extract it. Asked on demand rather than
  during a refresh — it is the expensive question, and nobody asks it of a
  green gate.
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

### 6. A source that did not answer says so, and says which kind of not

ephor's whole purpose is to be believed when it says there is nothing to do.
An empty section therefore has to mean "nothing is waiting", never "this
source could not be read" — the two are indistinguishable on screen, and the
second one silently costs exactly the work the reader opened ephor to find.

So a provider that cannot deliver **fails explicitly**, and never substitutes
an empty or partial answer for a failure:

1. **No silent degradation, in either transport.** An implementation that
   cannot complete an answer — a fetch that failed, output it cannot parse, a
   shape it does not recognise — reports the failure. Returning the part it
   managed is not allowed where the missing part changes meaning: a pull
   request whose conversation was dropped reads as one that needs no reply.
2. **A failed capability probe is a failure, not an empty declaration.** §1
   lets an implementation decline a capability, and ephor degrades to what is
   declared. That applies to an implementation that *answered*; one that could
   not be asked has declared nothing, and is reported as broken rather than as
   a forge that does very little.
3. **Failures are visible without being looked for.** Every failed provider is
   named — with its project — on stderr and in the interactive header, and a
   run that lost any provider exits non-zero. A partial refresh reported as
   success is how a source stays dark indefinitely: whatever runs the refresh
   on a timer sees exit 0 and no one is told.
4. **An unreachable destination is its own condition.** A host that could not
   be reached at all — DNS, refused connection, no route, a VPN that is down —
   is reported as unreachable rather than as a generic failure. It asks
   nothing of the reader but a working network, where every other failure asks
   them to go and change something, and the distinction also says whether the
   items still on screen are last-good values waiting out an outage.
5. **Last-good items are labelled, never passed off as current.** Keeping the
   previous answer when a provider fails is right — one flaky source must not
   blank the feed — but those items are marked stale wherever they appear, and
   the provider that failed is reported alongside them.
6. **What is quoted is the diagnosis, not the narration.** An out-of-process
   implementation writes its diagnostics to stderr, and the tools it wraps
   narrate their progress there too — "Requesting …", "Waiting for …". ephor
   reports one line of that stream, so taking the first line by position
   reports the narration and drops the error underneath it: a message that
   says only that something was attempted, and that no reader can act on.
   The line reported is the one that reads as a diagnosis.

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

## FS-004-quick-actions: a problem ephor recognizes arrives with the action for it

The action menu starts empty: every entry in it is something the reader wrote
down first. Yet what a person wants on a problem is nearly always the same
thing — the failing log, the diff, the conversation — and making them
configure that is ephor knowing what is wrong and sending them to fetch it
anyway. The reader would have to leave, find the repository, remember the
tool's flags, and come back with an answer ephor could have handed them.

So ephor offers those itself. A **quick action** is a menu entry ephor has
without being told, on an item where it already knows what the problem is.

### 1. A quick action belongs to the source that found the problem

The source that produced an item is the one place that knows which forge it
came from and which tool reaches it — and naming a forge or a vendor CLI
anywhere above that is what
[§FS-001-forge-interface](#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
forbids. A quick action is therefore offered by the source; ephor's core only
merges it into the menu and runs it, exactly as it runs a configured action —
the same checkout resolution, the same `EPHOR_*` environment, the same
handover of the terminal while it runs. A quick action is an ordinary menu
entry that nobody had to write, and a source that offers none is complete.

### 2. Offered only where it would work

A quick action appears only when running it would do something: the item has
the problem the action addresses, the identifiers the command needs are
known, and the tool it runs is installed. A menu that lists an action which
cannot work is worse than one that lists nothing, because the reader believes
it and spends a keystroke and a screen of errors finding out.

### 3. Quick actions come first, and configuration adds to them

They are listed above the configured actions, so that the obvious thing is
the first key. Configuration never replaces them: a reader whose own action
does the same job gets both, because ephor cannot tell that two commands mean
the same thing and silently dropping either one is the failure that matters.

### 4. Failing CI answers what failed and why

The quick action on a pull request whose gate is red shows what failed: the
check list as the forge reports it — which failed, which passed, which are
still running — and then the failures themselves, each with its log, paged.
That is the whole question a red gate asks, and reaching it by hand is several
commands and a browser tab.

The condition is the red gate, not the source that reported it. Every item
carrying a failing gate is offered the action, whichever source produced it,
and a source that cannot say what failed offers nothing. Hanging the action off
one kind of item instead leaves the reader looking at a number with nothing
behind it on every forge that reports its gate on the pull request itself —
which is most of them, since a gate is a property of the change, not a separate
piece of work.

Where ephor renders the failures itself rather than handing over a log,
identical ones are reported once, with the number of jobs that hit them. A gate
fans one error across every job that compiled the same file, and six copies of
one compile error is a worse answer than one copy that says six.
