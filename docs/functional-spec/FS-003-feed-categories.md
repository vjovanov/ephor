# FS-003-feed-categories: the feed sorts itself into categories, and finished work lands in Recent

A feed is read by scanning, not by searching, so items arrive already sorted
into the categories a person works in. The categories are ephor's, never a
provider's — a provider reports items and ephor places them
([§FS-001-forge-interface.3](FS-001-forge-interface.md#3-policy-lives-above-the-interface-never-in-an-implementation)),
policy staying on ephor's side of the seam ([§REQ-001-boundary](../requirements/REQ-001-boundary.md#req-001-boundary-every-capacity-ephor-lacks-crosses-a-seam-and-the-seam-has-one-anatomy)) — so every
forge lands in the same categories and a new implementation inherits them
without asking.

## 1. The categories

An item belongs to exactly one category, chosen by its kind and by the user's
role on it:

| Category | Holds |
| --- | --- |
| Status | project status lines |
| My Pull Requests | pull requests the user authored |
| Reviewing | pull requests the user is on as a reviewer |
| CI | gate and build results |
| My Issues | issues the user opened |
| Participating | issues the user is in but did not open, or follows by a label the source names ([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)) |
| Tasks | the project's own tasks, from a store in its checkout ([§FS-006-project-interface.7](FS-006-project-interface.md#7-the-projects-own-tasks-are-read-where-they-live)) |
| Messages | anything addressed to the user that is not a pull request or an issue |
| Recent | finished work that still leaves something to do — see [§2](#2-recent) |

Exactly one, so that the size of a category is the size of that pile of work
and not a double count.

## 2. Recent

Work does not stop mattering the moment it is finished, but most of it stops
asking for anything. An item whose state is terminal — closed, merged, done,
resolved, declined, however its forge spells it — leaves its category, and it
appears under **Recent** only while it still leaves the reader something to do.
There are three such things, and they are the whole list:

- **An answer is missing** — whatever would have made the subject await one
  while it was still open
  ([§4](#4-a-conversation-is-answered-in-whatever-form-the-forge-recorded-it)):
  somebody else had the last word, a task box on it is unticked, a notice named
  the reader. The comment that lands on the way out, or after it, is the case
  this exists for.
- **The gate is red** — the run that went the other way after the merge. A red
  gate is a thing to look at whatever the state beside it says, and the action
  that reads its log is offered on the row it is on
  ([§FS-004-quick-actions.4](FS-004-quick-actions.md#4-failing-ci-answers-what-failed-and-why)).
- **Work is still open on it** — the runtime holds a ticket about this matter.
  Work stands on rows beneath the matter ([§FS-005-dispatch.23](FS-005-dispatch.md#23-work-stands-on-rows-of-its-own-beneath-the-row-it-is-about)), so a matter
  that leaves the feed takes the work rows with it, and a run nobody can see is
  a run nobody can take back.

A finished item with none of the three leaves the feed the moment it finishes,
whatever the recency window would still allow. Nothing is lost by that: it
merged, which is what it was for, and a merge that went as asked is not news
anybody has to clear. Listing it anyway makes Recent a list of things nobody
will do anything about — read every day, at the cost of the rows that need
doing ([§GOAL-002-glance](../goals.md#goal-002-glance-one-glance-answers-what-needs-me-now)).

The recency window bounds the two the report knows: a missing answer and a red
gate are worth showing only while the item's own last activity falls inside it
([§3](#3-the-recency-window-is-configured)). Open work is not bounded by it. A
run in flight is not history, however long ago the matter it was asked about
last moved, and the ledger — not the forge — is what says it is still going.

**A loose end is not a response owed.** Finished work never awaits one:
whatever its conversation looks like — someone else had the last word, the user
was named and never answered — a finished item is news and not a task, and
nothing that counts work left to do counts it. What the loose end decides is
whether the news is worth putting in front of anybody, never that somebody owes
an answer.

## 3. The recency window is configured

How long finished work stays interesting is a property of a person, not of
ephor: the window is `defaults.recent_days` in the feed configuration, in
days, defaulting to 7. Zero drops an item from the feed the moment it is
finished, which is the behavior for someone who never looks back.

## 4. A conversation is answered in whatever form the forge recorded it

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
([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)) awaits somebody however the
conversation ended — including when it never started. The three forms above
all read the talk, and the talk is silent here in the most misleading way
available: an issue somebody filed and nobody picked up has its author's word
last, so the rule that serves every other case reports it as answered. It is
the same shape as a review asked for and never given
([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)) — being waited on leaves no
message behind.

Whether it applies is the source's to say, because *unclaimed* only means
*yours* where the reader is answerable for the backlog. On a project they run
it is the whole point; among issues they merely commented on somewhere it
would turn every stranger's open bug into their work. So it is configuration
on the source rather than a rule everywhere, and off unless asked for.

## 5. One subject is one row, however many sources reported it

Sources overlap on purpose. A source that searches by role and a source that
reads the forge's own notice list
([§FS-001-forge-interface.1](FS-001-forge-interface.md#1-capabilities)) are asking different questions
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
failure [§FS-001-forge-interface.6](FS-001-forge-interface.md#6-a-source-that-did-not-answer-says-so-and-says-which-kind-of-not)
is about.

