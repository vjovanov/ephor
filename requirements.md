# Requirements

Declare each behavior or requirement inline as an H2: `## FS-NNN-slug: …`.

## FS-001-forge-interface: ephor reaches every forge and issue tracker through one provider interface

ephor aggregates work from places that host code review and places that track
issues. Which ones they are is a property of a person's employer, not of ephor.
No forge, tracker, or vendor CLI may therefore be named in ephor's core
(§REQ-001-boundary.5): every one of them is reached through a single interface
with a fixed capability set, and an implementation is selected per project by
configuration.

### 1. Capabilities

An implementation answers some or all of the following; it declares which, and
ephor degrades to what is answered rather than failing.

- **Pull requests by role** — the ones the user authored, and the ones they are
  on as a reviewer. Each carries a stable id, title, url, state, head branch,
  last-updated time, and the reasons it is theirs. A reason is one of: they
  opened it, they are engaged in a thread on it, they are cited in it, a review
  was asked of them, or it is assigned to them. Being *asked* and being
  *engaged* are not the same thing, and the quiet one is the one that matters:
  a review request nobody has answered leaves no trace in the conversation, so
  an implementation that reports only the pull requests the user has already
  spoken in reports the ones they have handled and drops the ones they have
  not. Where a forge can be searched as a whole, an implementation reports the
  user's pull requests wherever they live rather than only in a configured set
  of repositories, on the same terms as issues below. State is reported
  whatever it is, closed and merged included: a question asked of the user does
  not stop being asked when the branch lands.
- **Own review** — on a pull request the user is on as a reviewer, the review
  *they* gave: approved, changes requested, or reviewed with no verdict either
  way. Nothing else reported here implies it. A reviewer list says who was
  asked, a conversation says who spoke, and neither says who answered: an
  approval leaves no message behind, and a reviewer who commented at length has
  still not approved — so a reviewing row that shows only `open` cannot tell a
  change the reader has dealt with from one they have not, which is the same
  gap the reasons above exist to close, seen from the other side. Absent where
  the user has not reviewed, and absent for their own pull requests, since an
  author does not review their own change. A verdict this vocabulary has no
  word for — an approval the forge dismissed, a review still in draft — is
  reported as no review, which is what it means to the reader. What a verdict
  *retires* is policy's
  ([§3](#3-policy-lives-above-the-interface-never-in-an-implementation)): an
  implementation says what the user did, never what is left of it. It is what a
  reviewing row leads with, except where a review is being asked for again —
  a re-request is the forge saying the old verdict is no longer the answer.
- **Conversation** — a PR's threads as ordered messages (author, text, time),
  each with its reactions, and enough identity for a reaction to be posted back
  to it. A message the forge tracks as a task carries that too: its state, and
  enough identity to transition it.
- **Reactions** — post a reaction on a message, where the implementation
  supports writing; read-only implementations say so and their messages are
  display-only.
- **Tasks** — resolve a task the forge tracks on a message. Forges spell it
  differently — a checklist item, a blocker comment, a review task — but they
  agree on the shape: a message that is done or not done, and a person who
  says which. An implementation that can only read task state declares no
  tasks capability; its tasks still render with their state, since knowing a
  box is unticked is most of the value even where ephor cannot tick it.
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
  activity worth seeing. Each also reports **whether anyone has taken it**,
  where the forge tracks that: an issue nobody has picked up is the plainest
  case of work with no owner, and it is a fact only the forge holds — a
  conversation cannot say it, and an issue filed and never answered looks
  exactly like one that is finished
  ([§FS-003-feed-categories.4](#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)).
  An implementation with no notion of assignment omits it, and nothing it
  reports is ever counted as unclaimed.
- **Notices** — what the forge itself says is directed at the user: one entry
  per thing it decided to tell them, carrying the reason it gives, the subject
  it concerns, when it arrived, and whether the forge considers it read. This
  is the completeness capability, and the only one whose job is to be
  exhaustive rather than exact. Every other capability answers a question ephor
  composed, and so can only return what ephor knew to ask about: the
  repositories it was configured with, the roles it thought to search, the
  kinds of thing it models. A forge that keeps a notification list of its own —
  GitHub's notifications, GitLab's todos — has already made that judgement
  across everything it hosts, including the kinds ephor has no capability for
  at all. Without it, [§6](#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)
  is a promise ephor cannot keep: an empty feed would mean "nothing is waiting
  in what I was told to look at", which is a different and much weaker claim
  than the one an empty feed makes.

  A notice about something another capability already reported is the same work
  seen twice, not two pieces of work; reconciling them is policy's job, under
  [§3](#3-policy-lives-above-the-interface-never-in-an-implementation) and
  [§FS-003-feed-categories.5](#5-one-subject-is-one-row-however-many-sources-reported-it).

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
response, which reported reason puts the user on which side of a pull request,
which of two reports of the same subject the reader sees, how threads and gate
counts roll up, how items match registry branches, and what counts as unread
are ephor's, applied identically to every implementation — so the feed stays
coherent across forges, and an implementation stays small enough to be a shell
script.

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
[§FS-001-forge-interface.5](#5-no-site-specific-data-in-the-repository) or the
literal confinement of §REQ-001-boundary.5. The checks are mechanical and run
before anything is uploaded.

## FS-003-feed-categories: the feed sorts itself into categories, and finished work lands in Recent

A feed is read by scanning, not by searching, so items arrive already sorted
into the categories a person works in. The categories are ephor's, never a
provider's — a provider reports items and ephor places them
([§FS-001-forge-interface.3](#3-policy-lives-above-the-interface-never-in-an-implementation)),
policy staying on ephor's side of the seam (§REQ-001-boundary) — so every
forge lands in the same categories and a new implementation inherits them
without asking.

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
| Messages | anything addressed to the user that is not a pull request or an issue |
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

### 4. A conversation is answered in whatever form the forge recorded it

A thread awaits the user while the answer it is waiting for is missing, and an
answer takes more than one form. Having the last word is one. A reaction on the
last message is another — often it is the honest answer, and demanding prose
where a forge offers a button would leave the thread pending forever.

A task the forge tracks is a third, and it outranks the other two: a thread
holding an unresolved task awaits the user however the conversation ended, and
a thread whose last word is a resolved task is answered even though a robot
spoke it. Task state is the forge's own record of whether the thing was done,
which is exactly the question, and reading who spoke last instead leaves every
bot checklist counted as work forever — the reader's own inbox tells them the
box is unticked long after they ticked it.

**An issue nobody has taken is a fourth, and it is not a conversation at all.**
Where a source is configured to say so, an unclaimed issue
([§FS-001-forge-interface.1](#1-capabilities)) awaits somebody however the
conversation ended — including when it never started. The three forms above
all read the talk, and the talk is silent here in the most misleading way
available: an issue somebody filed and nobody picked up has its author's word
last, so the rule that serves every other case reports it as answered. It is
the same shape as a review asked for and never given
([§FS-001-forge-interface.1](#1-capabilities)) — being waited on leaves no
message behind.

Whether it applies is the source's to say, because *unclaimed* only means
*yours* where the reader is answerable for the backlog. On a project they run
it is the whole point; among issues they merely commented on somewhere it
would turn every stranger's open bug into their work. So it is configuration
on the source rather than a rule everywhere, and off unless asked for.

### 5. One subject is one row, however many sources reported it

Sources overlap on purpose. A source that searches by role and a source that
reads the forge's own notice list
([§FS-001-forge-interface.1](#1-capabilities)) are asking different questions
that land on the same pull request, and the overlap is the point: it is how
ephor can be exhaustive without being told in advance where to look. What the
reader must never see is the consequence — the same pull request twice, in two
rows, counted twice in the size of the pile that [§1](#1-the-categories) exists
to make readable.

So a subject reported by several sources is one item. Which report survives is
decided by how much it says, not by which source was configured first: the one
carrying the conversation, the gate, and the role outranks the one carrying a
title and a reason, because the reader can act on the first and can only click
through on the second. What the losing report knew and the winner did not — a
reason the forge gave for telling the user about it — is carried over rather
than discarded, since that reason is often the only thing that explains why the
row is there at all.

Merging is by subject identity as the forge states it, never by title text: two
pull requests may share a title, and a subject whose identity cannot be
established is left alone rather than guessed at. A duplicate shown is a small
insult to the reader; a distinct piece of work silently swallowed is the
failure [§FS-001-forge-interface.6](#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)
is about.

## FS-004-quick-actions: a problem ephor recognizes arrives with the action for it

The action menu starts empty: every entry in it is something the reader wrote
down first. Yet what a person wants on a problem is nearly always the same
thing — the failing log, the diff, the conversation — and making them
configure that is ephor knowing what is wrong and sending them to fetch it
anyway. The reader would have to leave, find the repository, remember the
tool's flags, and come back with an answer ephor could have handed them.

So ephor offers those itself. A **quick action** is a menu entry ephor has
without being told, on an item where it already knows what the problem is —
the most frequent response made the cheapest one (§GOAL-001-fewest-moves).

### 1. A quick action belongs to the source that found the problem

The source that produced an item is the one place that knows which forge it
came from and which tool reaches it — and naming a forge or a vendor CLI
anywhere above that is what
[§FS-001-forge-interface](#fs-001-forge-interface-ephor-reaches-every-forge-and-issue-tracker-through-one-provider-interface)
forbids. A quick action is therefore offered by the source; ephor's core only
merges it into the menu and runs it, exactly as it runs a configured action —
the same checkout resolution, the same `EPHOR_*` environment, the same
handover of the terminal while it runs, one crossing in the seam's materials
(§REQ-001-boundary.1). A quick action is an ordinary menu
entry that nobody had to write, and a source that offers none is complete.

### 2. Offered only where it would work

A quick action appears only when running it would do something: the item has
the problem the action addresses, the identifiers the command needs are
known, and the tool it runs is installed. A menu that lists an action which
cannot work is worse than one that lists nothing, because the reader believes
it and spends a keystroke and a screen of errors finding out.

A key a screen advertises is under the same rule, and it is measured against
what the key would act on rather than against the screen. Message keys are
offered on the message the reader has selected: the key to react appears where
the forge would accept a reaction, the key to tick a task where there is an
unresolved task to tick. A footer that offers the same keys everywhere teaches
the reader a key that does nothing on most of what they select, and the answer
they get for pressing it — a refusal in one line at the bottom of a full
screen — is the one place they were not looking.

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

### 5. A task is ticked where it is read

A task renders as what it is — a box, ticked or not — beside the message that
carries it, and the reader ticks it from there. The whole of that interaction
is reading the sentence and agreeing with it, so sending the reader to a
browser to click the same box is the trip this section exists to save.

Ticking goes back through the source that reported the task, like every other
write ([§1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)) —
ephor knows a task has a state and a way to transition it, and nothing about
how that forge spells either. A forge that reports task state without offering
to write it renders its boxes and offers no key, which is
[§2](#2-offered-only-where-it-would-work) and not a degraded mode.

A ticked box is an answer ([§FS-003-feed-categories.4](#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)),
so the thread stops awaiting the reader as soon as the forge accepts the
transition, without waiting for the next refresh to say so.

### 6. A branch that trails its main branch is offered the rebase

ephor already measures how far every checked-out branch has fallen behind its
project's main branch, and says so on the branch row — `3 behind`. Then it
leaves the reader to go elsewhere and do something about it. Knowing what is
wrong and not offering the move is the whole of what this section exists to
stop, and here ephor is not even relying on a forge to tell it: the fact is on
disk, in the reader's own checkout.

So a pull request whose branch workspace is on disk and trails its main branch
is offered the rebase, and no other item is
([§2](#2-offered-only-where-it-would-work)): an item linked to no branch has
nowhere to rebase, a branch that trails by nothing has nothing to replay, and a
workspace that is not there is a checkout question
([§7](#7-a-workspace-that-is-not-there-is-offered-the-checkout)) rather than a
rebase one.

It is git and nothing else. Fetch, replay the branch on the base, say what
moved — no forge, no vendor CLI, and no knowledge of what the project is built
with. A poly-repo workspace is several repositories sharing one branch name, so
every repository under the checkout is rebased and the answer is given per
repository, one already current being reported as current rather than silently
skipped.

Two things it will not do. It does not stash: a rebase that quietly pockets
uncommitted work and replays it is a good trick right up until it conflicts,
and the reader is then holding a conflict in a change they had not finished
writing — a repository with uncommitted work is reported and left alone. And it
does not decide. A rebase that stops in a conflict has arrived at a question
about the code, which is
[§FS-005-dispatch.12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model).

### 7. A workspace that is not there is offered the checkout

A branch ephor watches, on a project whose checkouts are one per branch, is
either on disk or it is not — and ephor is the thing that knows which, because
it computes the directory from the project's own template and looks. Where it
is not there, everything else stops: every action that needs a checkout is
refused, and no work can be dispatched at all, since a ticket about a change
has to run in the change.

Sending the reader to a configuration file for the command that fixes that is
the same mistake as sending them for the failing log. It is one operation, the
same on every project ephor watches, and ephor is already holding every input
it takes: which repositories the project has, where each goes under the
checkout, which branch, and what that branch is grown from. So **a missing
workspace is offered the checkout, and ephor supplies the command**. A project
that wants its own — a bare mirror, a filesystem snapshot, a `gh pr checkout` —
configures one and that wins ([§3](#3-quick-actions-come-first-and-configuration-adds-to-them)),
but nothing has to be configured for the offer to exist.

What it does is git and nothing else, and it has the rebase's shape. A
poly-repo workspace is several repositories sharing one branch name, so each
gets a working tree under the new directory: the branch itself where the forge
has it, and a new branch of that name grown from the main branch where it does
not — which is what a change touching one repository of a tree looks like on
disk. The answer is per repository, and one that was already there is reported
as already there rather than silently skipped.

Two things it will not do. It will not move a branch another working tree is
holding — git refuses that, and it is right to; the repository is reported and
left alone rather than worked around. And it will not decide where to put the
workspace: the directory is the project's template applied to the branch, the
same one every other part of ephor resolves, because a checkout that landed
somewhere else would be a checkout nothing else could find.

Like the rebase, it is one implementation for both callers
([§FS-005-dispatch.12](#12-work-an-algorithm-can-finish-does-not-start-with-a-model)):
the key the reader presses and the command a state machine runs are the same
operation, since two of them would eventually disagree about what a checked-out
workspace is.

## FS-005-dispatch: what ephor watches, it can hand to an agent runtime

A watch that only watches hands its reader a list. Nearly every row on that
list has an obvious next move — the gate is red so the failures need reading
and fixing, a reviewer asked a question so it needs answering, an issue was
filed so it needs doing — and every one of those moves is the same shape: read
a change in a checkout, do something small, say what was done. That is work an
agent can be asked to do, and asking it is the boring half of the day.

ephor does not do that work. It **dispatches** it: it turns an item into a
ticket in an agent runtime, hands over what it already knows, and then keeps
the ledger — which items have work under way, what that work reached, and
whether the item has moved since. Watching and working are one loop, ephor is
the half that remembers, and the routine moves leave the reader's hands
(§GOAL-004-handover).

The runtime is a binding with [rhei](https://github.com/vjovanov/rhei) as the
shipped default (§REQ-001-boundary.1, decided with its tradeoff recorded in
§DA-001-runtime-bound-default). What ephor writes is a plan file in a
documented plain-text language, and that language — together with the runner
command configured to execute it and the verdict read back from its results —
is the entire coupling: a contract in files, never a linked process. Choosing
a runtime remains a property of how a person works, which is why one ships
wired and ready; requiring it would be something else. Nothing in ephor's
core names the default runner, and with no runner installed every part of
dispatch except the running still holds — tickets are written, read, and
reopened, staying readable, diffable, and hand-editable on disk — while
running refuses with the configured runner named.

### 1. A recipe decides which items deserve work, and what to ask for

A **recipe** is a named piece of configuration with a selector and a brief: the
selector says which items it applies to — kind, role, whether the gate is red,
whether a response is owed, which source reported it — and the brief is what
the ticket asks for, in the reader's own words.

Recipes are how the same watch serves different projects: what to do about a
red gate in one repository is not what to do about it in another, and neither
is ephor's to decide. A recipe is therefore configuration first. ephor ships
the few that are true everywhere — the red gate, the unanswered conversation,
the review, the issue, the branch that has fallen behind — for the same reason
it ships quick actions
([§FS-004-quick-actions](#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it)):
a problem ephor already recognizes should not need to be described to it
before anything can be done about it. Configuration adds recipes, and a
configured recipe that reuses a shipped one's name replaces it.

### 2. The ticket carries what ephor knows, not a link to it

A ticket that says "look at pull request 42" has handed back the whole job. The
watch already holds what the work needs: the title and state, the branch and
the checkout it lives in, the gate's counts per repository and the forge's own
reasons for refusing the merge, and the conversation as messages with their
authors and times. All of it was fetched already, and it is on disk.

So the ticket carries it — a **dossier** written into the plan, and the ask
written under it. Two things follow. The work starts from what a person would
have read first, instead of spending its opening move re-fetching what ephor
had. And the dossier is a record: it says what the item looked like when the
work was asked for, which is the only way to read the result of that work
later.

A dossier is bounded. A conversation of two hundred messages is not evidence,
it is a transcript; what is quoted is bounded per thread and in total, and
where anything was dropped the ticket says so and links to the whole.

### 3. One rhei per item, one ticket per dispatch

An item's work lives in one plan named after the item — so its whole history is
one file, and dispatching a second recipe on the same item adds a ticket to it
rather than starting a rival copy of the same work somewhere else.

The plan is created in the project the item belongs to, in the checkout the
item's branch resolves to — the same resolution actions already use
([§FS-004-quick-actions.1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)).
Work about a branch belongs in that branch's working tree: it is where the
change is, where the tools run, and where the runtime will put the agent. Where
the branch is not checked out, dispatch says so and offers the checkout,
because writing a ticket about code that is not on the machine only moves the
problem.

### 4. The ledger is ephor's record, and never the truth about the work

ephor keeps a ledger of what it dispatched: the item, the recipe, the plan, and
what the item looked like at that moment. The ledger is what makes the second
question answerable — has this already been handed over? — and it is written
where ephor's other state lives, not in the reader's repositories.

But the work's state belongs to the runtime and is read from the plan, never
cached in the ledger. A ledger that remembers "running" when the plan says
"done" is worse than no ledger: it is a watch reporting on itself instead of on
the world, which is the one thing this tool must never do. A ledger entry whose
plan has been deleted is reported as missing rather than repaired.

### 5. An item that moved reopens its work

Work asked about a pull request is answered against the pull request as it was.
New comments arrive; the gate turns red again; the state changes. The ticket
that was finished is now finished about something that no longer exists.

So ephor **fingerprints** the item at dispatch — its last activity, its state,
its gate, how much conversation it had — and a change to any of those makes the
work **stale**. Stale work is reopened by appending a ticket to the same plan
that says what changed since the last one and asks for the difference, ordered
after it. Not by opening a second plan: the point of the record is that one
item's work reads in one place, in order.

What is asked for is chosen against the item as it now is, preferring what was
asked last while that still applies. A change moves between categories as it
goes: the pull request whose gate was red is, two hours later, one whose jobs
pass and whose reviewer has asked a question. Reopening it under the recipe it
was first dispatched with would hand the work a ticket about a problem that is
no longer there. Where nothing applies any more — it merged, it closed — the
work is not reopened at all, and the ledger goes on saying that the item moved
past it.

Reopening is a decision, not a reflex. It is offered where it applies and
performed when asked for — by a person or by whatever runs the sync — and never
as a side effect of merely looking at the feed.

### 6. Dispatch is offered where it would work, and refuses where it would not

The rules of [§FS-004-quick-actions.2](#2-offered-only-where-it-would-work)
hold here and cost more when broken, because a ticket that cannot run is not a
wasted keystroke but a piece of work that looks scheduled and never happens. A
recipe is offered only when it matches the item, the item's project has a root,
and the checkout can be resolved — and where the work edits the change rather
than reading it, only when that change is actually on the machine. Where the
runtime's setup in that checkout cannot run what ephor would write — a state
machine already there that does not declare the state a recipe starts in —
dispatch refuses and names both, rather than writing a ticket that will sit
there unrunnable. It refuses the mirror image too: where the reader's own plans
are already in that directory under no declared machine, ephor does not install
one, because a state machine governs every plan in a project and theirs were
there first.

Matching is on what a gate is doing, not on how red it looks. Jobs that failed
are work for a checkout; a forge that refuses to merge an otherwise green
change is usually waiting on a person, and dispatching an agent at it spends a
pass to be told so.

Finished work is never dispatched. An item under Recent
([§FS-003-feed-categories.2](#2-recent)) is news, and asking an agent to fix a
merged pull request is asking it to invent something to do.

### 7. Handing over work is the reader's move, and stays inside the machine

Dispatch writes files and nothing else. It opens no pull request, posts no
comment, and pushes no branch — those are the runtime's to do, if a recipe asks
for them, and a recipe that does asks in the ticket's own words where a reader
can see it. What ships asks for none of them: the shipped recipes end at a
local change, and closing the loop out to the forge is one line of
configuration that a person turns on deliberately.

Bulk dispatch — every matching item in a project, in one command — is the same
guarantee at scale: it writes tickets, reports each one, and can be asked what
it would do without doing it.

### 8. The ticket carries the item as data, not only as prose

The dossier is written for a reader — a person or an agent — and a program
cannot read it. Yet the useful thing to put in front of an agent working a
failing gate is usually what a *script* can fetch: the log, the failing job,
the forge's analysis. A state machine can run that script before the agent, but
only if the script is told which item it is about, and prose is not an input.

So every ticket also carries the item's identifiers as **structured metadata**,
under the same names its context takes in a shell action
([§FS-004-quick-actions.1](#1-a-quick-action-belongs-to-the-source-that-found-the-problem)):
project and source, kind and item id, repository and number, branch and ticket,
url and state, and the checkout the work belongs to. One vocabulary, whether
the thing reading it is a shell command in a menu or a program in a state
machine.

Two consequences. A ticket that is appended to a plan **adds** its metadata
rather than replacing what is there, because the runtime keeps its own
bookkeeping in the same place and a ticket writing over it would break the
plan. And what is written is identifiers only — the prose stays in the dossier,
which is where a reader is looking.

### 9. Work that stops for a person says so where the person is looking

Work handed to a runtime is autonomous until it meets a question that is not
its to answer: a product decision, a trade-off between two things it cannot
weigh, an instruction it cannot read. The honest move then is to stop and ask —
and a machine that can only finish or fail will instead guess, because guessing
is the only move it has.

A runtime that can park work pending a person is therefore something ephor
reads, not something it invents: where the state a ticket sits in is one the
runtime will not leave on its own, that ticket is **waiting on the reader**, and
it is shown that way — ahead of anything else its work is doing, since it is
the one part of it nobody else will move.

The question and its answer stay in the plan. A ticket that asked something
carries the question in the artifact it wrote, and the answer belongs beside it
rather than in a chat window, a comment, or somebody's memory: the plan is the
record of what was decided about this item, and a decision taken anywhere else
is one the next round cannot read.

### 10. What ephor offers is not a limit on what can be asked

Recipes are for the work that repeats. Most work does not: a reader looks at a
change and knows the one thing they want done to it, and that thing has never
come up before and will not come up again. A tool where every ask must first be
written down as a rule, in a configuration file, in another window, has made
the common case the expensive one — and the reader will do it by hand instead,
which is what they were trying to stop doing.

So an item can be asked for **anything, in the reader's own words, where they
are standing**. What that produces is an ordinary ticket: the same dossier, the
same plan, the same place in the order, the same runtime. Only the brief is
different, in that nobody wrote it in advance.

Two things follow from its being asked for rather than offered:

1. **It is never refused for not matching.** Selectors say what ephor
   *volunteers*; they say nothing about what a person may ask for. Finished
   work, an item no recipe covers, a second ask on work already under way — each
   is somebody's deliberate request, and ephor's job is to write it down
   accurately rather than to have an opinion about it.
2. **The same holds for a command.** The action menu
   ([§FS-004-quick-actions](#fs-004-quick-actions-a-problem-ephor-recognizes-arrives-with-the-action-for-it))
   is configuration plus what a source offers; a reader who wants to run
   something once should not have to add it to a file first. A command typed
   into the menu runs exactly as a configured one does — the same checkout, the
   same `EPHOR_*` environment, the same handover of the terminal — because the
   only difference between the two is whether anyone expects to want it again.

### 11. A failure that is not the change's fault is restarted, not fixed

Not every red gate is a broken change. A runner dies, a mirror is unreachable,
a dependency ships a bad artifact, the same flake lands on the same job for the
third day running — and what the item needs then is not a fix but another run.
A loop that cannot tell the two apart pays for the difference twice: it spends
a model on diagnosing something that was never wrong, and then it lands a
commit whose only purpose was to make the gate start again.

So **restarting is a move the loop has**, and it is a different move from
fixing. Four things follow from its being different.

It is **decided by a program, not by prose**. Recognizing infrastructure is a
judgement, and a judgement nobody acts on is the same as no judgement at all:
what the model concluded has to reach a transition. So the verdict is a marker
on a line of its own, the way a question for a person is
([§FS-005-dispatch.9](#9-work-that-stops-for-a-person-says-so-where-the-person-is-looking)),
and a program reads that line and picks the state. An agent that is asked to
notice something, and given nowhere to put the answer, has been asked for
nothing.

It **opens a ticket of its own**. A restart is work done to the item, and the
plan is the record of that work — what was hit, when, and whether the round
after it came back green. A restart that happened as a side effect of some
other state would leave the next round unable to see that this failure has been
retried already, which is the one fact that separates a flake from a gate that
is simply broken.

It **restarts the gate and every gate downstream of it**. A gate that spans
several repositories fails downward: the repository whose job died takes the
rest of the tree with it, and the gates below never ran at all. Re-running only
what failed leaves every one of them exactly as red, so what is restarted is
the failing gate and everything under it that is not green. Nothing is
committed — the change was never the problem.

And it is **bounded**. An unhealthy runner pool answers a restart with the same
failure, and a loop that restarts every round is one that never stops and never
says why. Past a small number of restarts on one item the work stops for a
person, because at that point the infrastructure is the thing that is wrong and
no amount of retrying is going to be the fix.

### 12. Work an algorithm can finish does not start with a model

Not everything the watch turns up is a judgment call. Replaying a branch onto
its main branch is a fetch and a rebase: it either applies or it stops at a
conflict, and it does the same thing every time. Handing that to a model is
paying a pass to have two commands typed, slower and less predictably than the
commands would have run themselves — and the pass that matters is the one after
it, on the part no algorithm can do.

So the deterministic move runs first, and the work starts where it stopped.
Where it finished, nothing is dispatched at all: a clean rebase is a done
thing, not a ticket. Where it stopped, that is the ticket, and what is handed
over is the situation rather than the request to reproduce it — the repository
is left where the algorithm left it, mid-rebase with the conflict in the
working tree, because that is the state resolving it needs, and the ticket says
which repository, which files, and which two sides
([§2](#2-the-ticket-carries-what-ephor-knows-not-a-link-to-it)).

The rebase is the first of these, not the shape of the only one. Any recipe
whose opening move is deterministic makes that move before it costs a model,
and dispatches what is left over — which is also why the move has to be
runnable on its own ([§FS-004-quick-actions.6](#6-a-branch-that-trails-its-main-branch-is-offered-the-rebase)):
the same rebase the reader presses a key for is the one a state machine runs,
and two implementations of it would eventually disagree about what a clean
rebase is.

### 13. A communication is work too, and its answer comes back as a proposal

Not every matter's next move is a change in a checkout; often it is a reply.
A matter owing a response — a question in a review thread, a mail asking for
a decision, a mention carrying a request
(§FS-003-feed-categories.4) — is dispatched like any other: the ticket
carries the discussions as its dossier, and asks the runtime for a
**proposed answer**, drafted in the matter's context. The shipped answer
recipe is this shape, and an ask in the reader's own words (§10) may request
one for anything.

Three things distinguish it. **It needs no checkout**: the work is about the
conversation, so the plan is written where the matter resolves — the branch
workspace where one exists, because sitting in the change makes a better
answer, and the project root otherwise — and the checkout-able rung is not
required (§FS-006-project-interface.10). **The proposal is a file, never a
post**: §7 holds — the runtime writes the proposed reply into the plan's
results, ephor reads it back and surfaces it beside the discussion it
answers, and nothing reaches the channel by itself. **Posting is one
deliberate move, where the channel can carry it**: on a channel that
declares reply (§FS-007-matters.4) the surfaced proposal is offered for
posting, edited or as it stands, exactly as a reaction is posted today; on
a channel that does not, the proposal is what the person copies — a stated
degrade (§REQ-001-boundary.1), not a failure.

## FS-006-project-interface: a project and ephor meet over one interface, in three homes

ephor requires capabilities of a project, never artifacts in it
(§REQ-001-boundary.3): a project is fully watchable exactly as it stands, and
everything beyond watching is something the project *can do* or *chooses to
offer*. This section is the whole of how a project and ephor speak — where
each fact lives, how a command is invoked, what it may answer, and what its
absence degrades to — so that tracking a new project costs minutes and
touches nothing in it (§GOAL-005-costless). Whatever crosses this interface
in structure is validated against a published schema; nothing crosses it as
linked code.

### 1. The three homes

Every fact the interface carries lives in exactly one of three places.
**Description and identity** live in the registry row: where the forest is,
how a branch becomes a workspace, and the signals by which the project's
matters are recognized (§FS-008-attribution.1). **Operational bindings**
live in site configuration: which command fills which verb, which runtime
runs work, the person's own actions and recipes. **Conventions** are probed
in the checkout: well-known names a project carries for its own sake. One
precedence resolves them all — *site configuration over manifest over
probe* (§REQ-001-boundary.2) — so a probe is a default, a manifest is a
declaration, and the person always has the last word.

### 2. The manifest is offered, never required

A project that chooses to speak places one file, `ephor.json`, at its forest
root (decided in §DF-001-manifest-offered). It may declare identity hints,
the forest's own layout, check verbs, gate verbs, ticket stores, and
offers — every field optional, an empty
manifest valid, and nothing in it able to gate a capability that probing or
site configuration could not establish alone. Identity fields are hints the
registry row adopts unless it overrides: **the row is authoritative, because
attribution keys must not be forgeable by a checkout.** Manifest commands
run with exactly the trust a person extends to running the project's own
build, and the row can narrow it — honoring the manifest fully, reading only
its descriptions, or ignoring it — for checkouts trusted less. Offers are
menu entries a person invokes; what spends agent time on its own match (a
recipe) is site configuration only, because a repository does not get to
spend it.

### 3. A summons: environment in, exit code and answer out

Every command the interface names — a check verb, a gate verb, a checkout, a
ticket store's CLI, an offer — is invoked one way. It runs in the resolved
place: the item's branch workspace where one resolves, the forest root
otherwise, a manifest-designated repository of the forest where the entry
says so. It receives the dossier as `EPHOR_*` environment — one vocabulary,
identical to what a shell action and a state-machine script receive
(§FS-005-dispatch.8). It answers first with its exit code — `0` for done,
non-zero for failed, `75` for *parked*: not applicable now, ask again later —
and optionally in structure, written to the file named by `$EPHOR_ANSWER`.
Standard output and error remain the command's own, streamed to the person
or the log; a contract that parsed them would make every honest build log a
protocol violation.

**Paths in that environment are spelled so the shell can read them.** The
command is invoked through a shell, so `$EPHOR_ANSWER` and the rest are
strings that shell parses before anything opens them — and where a platform's
native spelling separates directories with the shell's own escape character, a
path handed over verbatim stops being a path: the redirect lands somewhere
else, or nowhere, and the answer comes back empty with nothing saying why,
which is the silence §REQ-001-boundary.1 refuses. Path-valued variables use
`/` between their segments on every platform. This costs nothing where the two
spellings already agree, and a command that only passes a path on to another
program sees no difference either way; the place the command runs in is set by
ephor rather than spelled to the shell, so it is not involved.

### 4. The answer envelope

Structured answers share one envelope, speaking the model's nouns
(§FS-007-matters): `matters`, `discussions`, and `events` for sources with
something to report; `summary`, `url`, and `needs_response` for the common
one-line cases; `failures` and `features` as verb-level conveniences that
ephor normalizes into events and facts; `data` as free passthrough that
returns wherever the dossier's metadata goes. Each verb's contract names the
fields it reads and ignores the rest, and unknown fields are ignored
everywhere — the envelope evolves by addition, and an incompatible change is
a version bump with a changelog entry (§11). Paths in an answer resolve
against the summons's working directory.

### 5. Checks are verbs, and every script is self-contained

A project's checks are three well-known names probed at the forest root —
`./check.sh` the aggregate, `./check-style.sh` the fast style pass,
`./smoke-test.sh` the smoke — or the same three declared in the manifest
under whatever paths the project prefers. Each is self-contained: a smoke
test that needs a build performs its build, because how a project builds is
the project's knowledge and stays there. Smoke may enumerate **features** —
`--list` printing one id per line, or a static list in the manifest — and a
feature id given as an argument runs that feature's smoke alone; without
enumeration, smoke is one opaque verb and that is a complete
implementation. Which verbs run, and in what order, is policy above the
interface: a verify step sequences them from site configuration, one
summons each.

### 6. The gate is the project's, in three verbs

How to ask a project's CI what it is doing is project truth — the same for
every person who works on it — so its home is the manifest, with site
configuration overriding where credentials or variants demand. Three verbs:
**status** answers the gate's counts per repository of the forest;
**failures** answers what actually failed, as the failing job, its log, and
the error where it can be had — the expensive question, asked on demand
(§FS-001-forge-interface.1); **restart** re-runs the failing gate and every
gate downstream of it, committing nothing, under the semantics of
§FS-005-dispatch.11. A forge-hosted gate needs no manifest at all: the
provider's own gate capability is the shipped default binding. A project
with an internal gate binds three commands, and nothing above the seam can
tell the difference.

### 7. Local ticket stores are read where they live

A project may keep tickets in its checkout — a plan directory, a
git-backed issue store — and a store ephor recognizes is read through the
store's own files and CLI, as matters with their discussions
(§FS-007-matters), into the same feed under the same rules as anything a
forge reported. Recognition is by probed convention or manifest
declaration; attribution is the checkout's own project; and the stores are
project-native things that exist without ephor — a store's presence is a
capability rung, never an obligation.

**Where they live is per branch, where a project has branch workspaces.**
Work about a change belongs in that change's working tree
(§FS-005-dispatch.3), so a branch-addressable project keeps a store per
workspace rather than one at the forest root. Both places are read: the root,
for a project whose checkout is its root, and every branch workspace on disk
that holds one, for a project whose branches have trees of their own. Reading
only the root leaves such a project writing its work into a place it never
looks again — work dispatched, plans on disk, and a feed that shows none of
it.

Verified on disk rather than derived from the registry row. The row names the
branches somebody wrote down; the stores are wherever branches were actually
checked out, and the two are not the same list. The rung this buys is
*local-issues* rather than *ticketed*, because a **ticket** is what a remote
tracker keys and these are the project's own — one name for one thing
(§FS-001-forge-interface.3).

**A workspace ephor makes gets a store.** Where ephor creates a branch
workspace (§FS-004-quick-actions.7) it initializes one there, so the first
dispatch into that branch has somewhere to land and what is under way is
visible from the moment the tree exists. This is not an artifact required of
the project and does not bend §REQ-001-boundary.3: the store ignores itself,
so what it holds is ephor's own planning state that happens to live in a
checkout, never content the project carries. A project cloned without ephor
is byte-for-byte what it was.

### 8. The checkout contract

A project may bind one **checkout** command; its contract is to make
`$EPHOR_WORKSPACE` exist, and ephor verifies the directory afterwards
rather than trusting the exit code alone. Where none is bound, ephor
supplies the git checkout itself (§FS-004-quick-actions.7). Everything that
needs a workspace — offers marked as requiring one, work whose recipe edits
the change — is gated on this contract and degrades by naming it.

### 9. Offers: the project's actions

A manifest may offer actions: entries for the same menu configured actions
occupy, in the same shape, selected by the same `when` language recipes use,
and gated by the same capability requirements. Provenance orders the menu —
what ephor itself recognized first (§FS-004-quick-actions.3), then the
project's offers, then the person's own — and where two entries share an id,
the person's beats the project's beats the shipped one. An offer is invoked
by a person, runs as a summons (§3), and is refused with its reason where
its requirements do not hold (§FS-004-quick-actions.2).

### 10. Capability, rung by rung

What a project can do is resolved into a ladder, and every feature names
the rungs it needs: *observable* (a registry row and at least one source
answering) buys the watch; *placed* (the forest root on disk) buys actions
and update; *branch-addressable* (a workspace template) buys resolution of
matters to workspaces; *checkout-able* (§8) buys work that edits;
*checkable* (§5) buys verification that means something; *gated* (§6) buys
failure dossiers and the restart; *local-issues* (§7) buys the project's
own issues as matters;
*workable* (a bound runtime, §FS-005-dispatch) buys the loop. A missing
rung degrades exactly the features that named it, with the reason stated
where the feature would have appeared — never an error, never silence
(§REQ-001-boundary.1).

### 11. The interface is versioned

The manifest, the envelope, and the registry schema are published schemas,
embedded in the binary and printable on demand, so a project can validate
what it says without ephor present. They evolve by addition: an optional
field costs nothing, unknown fields are ignored, and any incompatible
change bumps the schema version with a changelog entry per
§FS-002-release.1. The schemas are the interface's stability surface — what
a release may change is answerable by diffing them.

## FS-007-matters: the feed is made of matters, and a matter knows why it is there

The unit of the watch is the **matter**: the subject under discussion or
observation — a pull request, an issue, a ticket in a local store, a
periodic build, a status subject, or a bare topic. What the spec has so far
called an item is a matter seen through one source's report. A matter is
the feed's row, the unit of attribution, of state, of fingerprinting, and
of dispatch — the dossier is the dossier of a matter — and the reason for
the noun is that the same matter is discussed in more than one place: the
pull request's review threads, a mail thread about it, a chat fragment
naming it. One subject, several venues, one row (§GOAL-002-glance).

### 1. A matter is a subject with a stated identity

A matter's identity is the subject key its source stated — the pull request
the forge names, the ticket by its key, the store's own id — or, where a
conversation matched a project but no known subject, an identity synthesized
for it as a topic. Identity is never guessed from resemblance: two pull
requests may share a title, and a subject whose identity cannot be
established is left alone (§FS-003-feed-categories.5).

### 2. Same subject, one matter; related subjects, linked matters

Reports of the same subject key merge into one matter, however many sources
made them, under the survival rules of §FS-003-feed-categories.5. Matters
that *reference* each other — the pull request implementing a ticket, the
local ticket tracking a gate — stay distinct and are **linked**, presented
together under the branch that relates them. Merging what is one thing and
linking what is related is the difference between a readable pile and a
lossy one.

### 3. A discussion is messages grouped in a channel

A matter's conversation arrives as **discussions**: ordered messages with
authors, times, reactions, and task boxes, grouped within one channel.
Whether a discussion awaits the reader is decided per discussion, by the
calculus of §FS-003-feed-categories.4, identically in every channel. A
matter awaits its reader while any of its discussions does.

### 4. A channel says what it can do

The venue a discussion lives in — review threads, an issue's comments, a
mail thread, a chat thread — declares its capabilities in the pattern of
§FS-001-forge-interface.1: whether a reaction can be posted, a task ticked,
a reply sent. What grouping means is the channel's own policy; what the
reader can do about a message is offered only where the channel declared it
(§FS-004-quick-actions.2) — an undeclared capability narrows the offer by
the degrade rule of §REQ-001-boundary.1, never silently.

### 5. An event moves state, and resurfacing names its reason

Everything about a matter that is not conversation arrives as **events**:
the gate's counts changed, the state closed, a check finished, a ticket
resolved. Events fold into the matter's state, and the matter's fingerprint
digests state, discussions, and the event tail — so when a done matter
moves and resurfaces (§FS-005-dispatch.5), the row can say *what* moved:
resurfacing is always accompanied by its reason, because a row that
reappears without one sends the reader to re-read everything, which is the
sweep this tool exists to retire (§GOAL-003-nothing-lost).

## FS-008-attribution: every conversation finds its project, or says that it could not

Conversations arrive from places that know nothing of the registry: a
mailbox serves every project a person has, a discussion sits on an adjacent
repository, a notice names a subject nobody configured. Attribution is
ephor's own move — deciding whose business a conversation is — and it is
data matching, never code: evidence the conversation carries against
identity the registry declares (§GOAL-003-nothing-lost).

### 1. Identity is declared, and the row has the last word

A project's identity is the set of signals by which its matters are
recognized: ticket patterns, the forest's repositories, the wider
**territory** the project claims — repositories and organizations that are
its business without being in its forest — names and aliases, addresses. It
lives in the registry row; a manifest may hint it
(§FS-006-project-interface.2), and the row adopts or overrides — a checkout
must not be able to claim another project's conversations. Territory is what
places the general case: a mention of the person on some repository of the
project's ecosystem, an issue filed there, a discussion opened there —
none of it in any forest, all of it the project's business
(§GOAL-003-nothing-lost).

### 2. Two stages, one engine

Attribution runs discussion → matter, then matter → project and branch. It
is one matching engine at two scopes: the branch matching that already
places items under registry branches is this engine confined to one
project, and it is promoted, not duplicated.

### 3. Venue beats reference beats resemblance

A discussion *on* a subject belongs to that subject's matter. A discussion
*naming* a subject — a ticket key in a mail's text, a pull request's URL in
a chat message — belongs to the named matter, linked onward
(§FS-007-matters.2). Only where neither holds may declared aliases place a
conversation, and then as a topic matter, never onto an existing subject:
resemblance may start a new row, it may not amend one. At the second stage
the venue itself is the explicit signal: a matter whose subject sits on a
repository of a project's forest or declared territory
(§FS-008-attribution.1) is that project's before any reference or alias is
consulted.

### 4. Unattributed is a place, not a fate

A conversation that matched nothing lands in a visible unattributed bucket,
in the interactive view and on demand — never dropped. The bucket is the
attribution seam's degrade rule (§REQ-001-boundary.1): mapping failures are
seen where they can be fixed, by adding the signal the identity was
missing.

## FS-009-shipped-actions: what ephor ships for CI runs from the repository alone

ephor ships continuous-integration entry points — workflow steps a project
wires into its own CI — and every one of them obeys the rule that selects
them: **a shipped step runs from repository-committed material and workflow
inputs alone**, never from a personal site. A step that would need a
registry, credentials for a person's sources, or a person's bindings does
not ship as CI; the watch-and-work loop stays on machines that have a site,
and shipping it hosted would mean shipping someone's configuration
(§REQ-001-boundary.2).

### 1. The set

Three steps ship. **Validate** checks a repository's `ephor.json` — and a
committed registry, where a repository carries one — against the published
schemas (§FS-006-project-interface.11). **Check** reads the manifest's
declared check verbs and runs them, per-feature where features are
enumerated (§FS-006-project-interface.5) — the project's own gate derived
from the project's own declaration, with nothing project-specific in the
workflow. **Setup** installs a pinned ephor release, and is the building
block the other two and anyone's own composition stand on.

### 2. Versioned and released with ephor

The steps live in ephor's repository and version with it: a release that
changes a schema or a verb ships the steps that understand the change, per
§FS-002-release. A repository pins the version it consumes, as it pins any
dependency (§GOAL-005-costless).

## FS-010-doctor: ephor can be asked whether it still works, and answers in one screen

The watch is only worth having if it is believed when it says there is
nothing to do (§GOAL-003-nothing-lost). Everything that makes that claim
false is quiet: a credential that expired, a forge whose extension left
`PATH`, a checkout somebody deleted, a runner that a system upgrade removed.
None of them announces itself — each one simply makes a section of the feed
empty, which is the one thing an empty section must never mean
(§FS-001-forge-interface.6).

So ephor can be **asked**. `doctor` answers "is this still working" for the
whole site in one run, and it is built to be run on a timer by whoever wants
one: it needs no argument, it prints what is wrong and what would fix it,
and it says so in its exit code.

### 1. It reports what is already judged, and judges nothing itself

What a project can do is the ladder of §FS-006-project-interface.10, and why
a rung is missing is a sentence that already exists. What a source did is
§FS-001-forge-interface.6's answer, with its own split between a
configuration to go and fix and a network to wait out. `doctor` composes
those two and adds no opinion of its own.

This is the whole of the design rule. A second opinion about whether a
project is checkout-able would be a diagnosis that drifts from the one the
menu refuses with, and a reader holding two answers has none: the sentence
`doctor` prints and the sentence a greyed entry shows are the same sentence.

### 2. The ladder is answerable on its own

The same table is worth having without a sweep. **`capabilities`** prints one
project's rungs — held, and missing with their reasons — so that "why is this
action not offered here" is a question with a cheap answer. `doctor`'s first
pass is this for every configured project.

### 3. Two passes: the site, and ephor itself

**The site pass** asks the world: the registry parses, every project's row
resolves, each ladder is computed, and every configured source is asked —
refreshed rather than read from cache, since a cached answer cannot say
whether a source still answers. What it reports is per project, and a
project that is entirely well says so in one line.

**The self pass** asks the binary. It builds a throwaway project — its own
state directory, registry, configuration and checkout, in a temporary place —
and walks the seams end to end against it: a forge reached out of process
(§FS-001-forge-interface.2), a refresh that categorizes what came back, a
summons answering by exit code and by envelope (§FS-006-project-interface.3),
a check verb bound by manifest and by probe (§FS-006-project-interface.5),
the git operations both a key and a state machine run
(§FS-004-quick-actions.6, §FS-004-quick-actions.7), a dispatch that writes a
plan and reads its ledger back (§FS-005-dispatch.4), and a local ticket store
read where it lives (§FS-006-project-interface.7).

It touches nothing of the person's, reads no registry of theirs, and reaches
no forge: it is hermetic, or it is not a diagnosis. This is what a test suite
cannot answer — `cargo test` speaks for the tree it was run in, and the
question here is whether the binary on *this* machine still works.

**Both passes say what they are doing while they do it.** Asking every source
of every project takes as long as the slowest forge, and a diagnostic that
prints nothing until it is finished is one a reader kills half way through
and reports as hung — which is the same failure the tool exists to name, with
ephor as the source that did not answer. So each step is announced as it is
reached and each answer is given as it arrives, on the error stream, where it
narrates the run without becoming part of it: what a program reads is the
report, and a progress line that reached a parser would be ephor writing to
its own contract.

### 4. Nothing of the reader's is written

The site pass refreshes and reads; the self pass writes only inside the
temporary place it made and removes it afterwards. `doctor` posts nothing,
dispatches nothing, and changes no checkout of the reader's — a diagnostic
that repaired things as it went would be a second thing to debug, and one
that could not be run while unsure is one nobody runs.

### 5. The answer is in the exit code

A timer reads exit codes, not screens. `0` is well. `4` is degraded — a rung
missing or a source lost, which is the condition a person acts on. `3` is
nothing on the site reachable at all. `1` is the self pass failing, which is
ephor itself being wrong rather than the world being away, and is the one
answer that does not improve by waiting.
