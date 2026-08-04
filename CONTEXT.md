# Wayfind

Wayfind charts one piece of long-running exploratory work at a time: where it is
going, what stands between here and there, what each obstacle was settled on,
and what is still unknown. It answers in TOML front matter plus Markdown so an
agent can parse the answer and a person can read it.

## Language

### The work

**Initiative**:
One named piece of work with a destination, from charting to clear.
_Avoid_: Project, epic, milestone

**Destination**:
The sentence that says what finishing the **Initiative** looks like.
_Avoid_: Goal, definition of done

**Ticket**:
One question or piece of work inside an **Initiative**.
_Avoid_: Issue, task, card

**Decision**:
The text that settles a **Ticket** and closes it.
_Avoid_: Answer, resolution note, outcome

**Gist**:
The clamped one-line form of a **Decision**, shown wherever the full text would
be too long.
_Avoid_: Summary, excerpt

**Fog**:
A note recording something the **Initiative** has not yet specified.
_Avoid_: Unknown, open question, TODO

**Exclusion**:
A note recording something the **Initiative** will deliberately not do.
_Avoid_: Out of scope item, non-goal

**Handoff**:
The document that carries an **Initiative** to whoever picks it up next: every
**Decision** in full, rather than as a **Gist**.
_Avoid_: Summary, report

### The graph

**Dependency**:
An edge saying one **Ticket** must wait until another is resolved.
_Avoid_: Link, relation, parent

**Blocker**:
The **Ticket** on the waited-for end of a **Dependency**.
_Avoid_: Parent, prerequisite

**Frontier**:
Every **Ticket** that is open and whose **Blockers** are all resolved — the work
that can be picked up right now.
_Avoid_: Backlog, ready queue, available set

**Map**:
The document that reports an **Initiative**'s **Frontier**, **Decisions**,
**Fog**, and **Exclusions**.
_Avoid_: Overview, dashboard

**Tree**:
The drawing of an **Initiative**'s **Dependency** graph, one row per **Ticket**.
_Avoid_: Graph view, chart

### Who is working

**Session**:
One agent's or person's run of work, bound to the **Initiative** it first
appeared in.
_Avoid_: Run, worker, user

**Claim**:
A **Session**'s hold on one **Ticket**, taken before it may be resolved.
_Avoid_: Assignment, lock, lease

**Non-research budget**:
The single non-research **Ticket** a **Session** may settle in its whole life.
_Avoid_: Quota, allowance

**Project**:
The directory a command belongs to — a git checkout root, or the directory
itself when it is not in one.
_Avoid_: Repository, workspace, folder

### Documents

**Attachment**:
A body of text filed against one **Ticket** and readable from any other.
_Avoid_: File, upload, artifact

**Reference**:
A pointer from a **Ticket** to an **Attachment** another **Ticket** owns.
_Avoid_: Link, alias, shortcut

## Relationships

- A **Project** holds zero or more **Initiatives**; exactly one is active.
- An **Initiative** holds zero or more **Tickets**, **Fog** notes, and
  **Exclusions**.
- A **Ticket** waits on zero or more **Blockers** through **Dependencies**.
- A resolved **Ticket** carries exactly one **Decision**.
- A **Ticket** owns zero or more **Attachments**; any other **Ticket** of the
  same **Initiative** may hold a **Reference** to one.
- A **Session** is bound to exactly one **Initiative**, holds at most one
  **Claim**, and spends its **Non-research budget** at most once.

## Example dialogue

> **Dev:** "If I claim a **Ticket** and the **Session** dies, is the **Ticket**
> stuck?"
> **Domain expert:** "It stays claimed. The **Claim** belongs to that
> **Session**, and nobody else may resolve it — that is the point. Start a new
> **Session** and claim a different **Ticket**."
>
> **Dev:** "Then why may a **Session** only settle one non-research **Ticket**?"
> **Domain expert:** "Because a **Session** that settles two has stopped
> exploring and started grinding. Research is free — you can read all day. But
> a **Decision** on a grilling, prototype, or task **Ticket** spends the
> **Non-research budget**, and after that the **Session** may only research."
>
> **Dev:** "What decides which **Ticket** `next` hands me?"
> **Domain expert:** "The **Frontier**, lowest identifier first. A **Ticket**
> whose **Blocker** is claimed but not resolved is not on the **Frontier** —
> only a **Decision** clears a **Blocker**."

## Flagged ambiguities

- "resolution" was used for both the act of closing a **Ticket** and the text
  that closes it — resolved: the text is the **Decision**; the act is
  "resolving".
- "attachment" was used for both an owned document and a **Reference** to
  someone else's — resolved: these are distinct, and a **Ticket** reports them
  in separate counts.
- "session" was used for both a terminal session and a Wayfind **Session** —
  resolved: a Wayfind **Session** is named by `--session`,
  `WAYFIND_SESSION_ID`, or `CLAUDE_SESSION_ID`, and outlives any terminal.

## Behavior

- [Initiatives](./.claude/context/initiative.md) — charting, clearing, fog,
  scope, and the map, tree, and handoff documents
- [Tickets](./.claude/context/ticket.md) — creating, claiming, resolving,
  amending, dependencies, and the frontier
- [Attachments](./.claude/context/attachment.md) — filing, referencing,
  reading, and deleting documents
- [Search and export](./.claude/context/query.md) — full-text search and CSV
  records
- [Configuration and storage](./.claude/context/configuration.md) — the
  configuration layers, project and session selection, and compatibility with
  the Bash script's database
- [Output shape](./.claude/context/output.md) — the document contract every
  command answers with
- [v2 store lifecycle](./.claude/context/v2-store.md) — creating and opening
  the `wayfind2` store, and how an unimplemented v2 command answers
