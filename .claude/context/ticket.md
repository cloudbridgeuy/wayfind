# Behavior: Tickets

Requirements for the life of a **Ticket**: creating it, claiming it, settling
it, repairing its **Decision**, and the **Dependency** graph it sits in.

## Behavior

### Requirement: Creating a ticket
A **Ticket** needs a title, a type, and a question. It is created open, in the
**Initiative** in play, and moves that Initiative out of charting.

#### Scenario: A new ticket
- **WHEN** `wayfind ticket create --title T --type research --question Q` runs
- **THEN** a Ticket is created with status `open`, no **Claim**, and no
  **Decision**
- **AND** the Ticket is returned

#### Scenario: An unknown type
- **WHEN** the type is not one of grilling, research, prototype, or task
- **THEN** the command refuses before anything is opened

### Requirement: Claiming a ticket
A **Session** takes a **Ticket** with a **Claim** before it may settle it. A
Session holds at most one Ticket at a time.

#### Scenario: An open ticket
- **WHEN** a Session claims an open Ticket
- **THEN** the Ticket's status becomes `claimed`
- **AND** the Session is bound to it

#### Scenario: The same ticket twice
- **WHEN** a Session claims a Ticket it already holds
- **THEN** nothing changes and the command succeeds

#### Scenario: Someone else's ticket
- **WHEN** a Session claims a Ticket another Session holds
- **THEN** the command refuses

#### Scenario: Already holding something
- **WHEN** a Session holding one Ticket claims another
- **THEN** the command refuses, and says so before it checks the budget

#### Scenario: A settled ticket
- **WHEN** a Session claims a resolved Ticket
- **THEN** the command refuses

### Requirement: The non-research budget
A **Session** may settle exactly one non-research **Ticket** in its whole life.
Research Tickets cost it nothing. The limit applies at claim and at resolution
alike, and never lifts.

#### Scenario: A spent session takes research
- **WHEN** a Session that has settled a non-research Ticket claims a research
  Ticket
- **THEN** the claim is accepted

#### Scenario: A spent session takes anything else
- **WHEN** that Session claims a grilling, prototype, or task Ticket
- **THEN** the command refuses

### Requirement: Resolving a ticket
Only the **Session** holding a **Ticket** may settle it, and settling records a
**Decision**, frees the Session, and closes the Ticket for good.

#### Scenario: The holder settles it
- **WHEN** the holding Session resolves the Ticket with text
- **THEN** the Ticket's status becomes `resolved` and carries the Decision and
  the time
- **AND** the Session holds nothing again

#### Scenario: Nobody claimed it
- **WHEN** an unclaimed Ticket is resolved
- **THEN** the command refuses

#### Scenario: Another session
- **WHEN** a Session that does not hold the Ticket resolves it
- **THEN** the command refuses

#### Scenario: Already settled
- **WHEN** a resolved Ticket is resolved again
- **THEN** the command refuses

#### Scenario: Where the text comes from
- **WHEN** the Decision is given as `--resolution TEXT`, as
  `--resolution-file PATH`, or as `--resolution-file -` with text piped in
- **THEN** the Decision is recorded the same way in each case
- **AND** giving both, or neither, is refused

### Requirement: Amending a decision
A **Decision** already recorded may have its text repaired. Nothing else about
the **Ticket** changes.

#### Scenario: Repairing a decision
- **WHEN** `wayfind ticket amend ID --resolution TEXT` runs on a resolved Ticket
- **THEN** the Decision and its **Gist** are replaced
- **AND** the amendment time is recorded alongside the original resolution time

#### Scenario: An unresolved ticket
- **WHEN** an open or claimed Ticket is amended
- **THEN** the command refuses

### Requirement: Dependencies
A **Dependency** says one **Ticket** waits for another. Both ends must be in the
**Initiative** in play, and an edge that would close a loop is refused.

#### Scenario: An ordinary edge
- **WHEN** `wayfind ticket block ID --by BLOCKER` runs with both on this
  Initiative
- **THEN** the edge is recorded and the Ticket reports the **Blocker**

#### Scenario: The same edge again
- **WHEN** the edge is already recorded
- **THEN** the command succeeds and nothing changes

#### Scenario: A self-edge
- **WHEN** a Ticket is made to wait on itself
- **THEN** the command refuses

#### Scenario: A cycle
- **WHEN** the edge would close a loop
- **THEN** the command refuses and names the whole loop

#### Scenario: A ticket off this map
- **WHEN** either end is not in this Initiative
- **THEN** the command refuses

### Requirement: The frontier
A **Ticket** is on the **Frontier** when it is open and every one of its
**Blockers** carries a **Decision**. The Frontier is ordered by ascending
identifier.

#### Scenario: An unblocked open ticket
- **WHEN** a Ticket is open with no Blockers
- **THEN** it is on the Frontier

#### Scenario: A blocker that is claimed but not resolved
- **WHEN** a Ticket's Blocker is claimed
- **THEN** the Ticket is not on the Frontier

#### Scenario: An excluded blocker
- **WHEN** a Ticket's Blocker is excluded
- **THEN** the Ticket is still blocked

#### Scenario: A blocker that is not in the initiative
- **WHEN** a Dependency names a Ticket that is not there
- **THEN** it does not block, matching the Bash script's join

### Requirement: Next
`wayfind next` hands out the first **Ticket** on the **Frontier**. An empty
Frontier is an answer, not a failure.

#### Scenario: Work is available
- **WHEN** the Frontier holds Tickets
- **THEN** the lowest-identifier one is returned

#### Scenario: Nothing is available
- **WHEN** the Frontier is empty
- **THEN** a document reports the Initiative's state and what to do about it

### Requirement: Showing a ticket
`wayfind ticket ID` reports one **Ticket** with its blockers, its counts of
owned and referenced **Attachments**, its question, and its **Decision** if it
has one.

#### Scenario: An open ticket
- **WHEN** the Ticket is open
- **THEN** the document has no resolution section

#### Scenario: A resolved ticket
- **WHEN** the Ticket is resolved
- **THEN** the Decision follows the question

#### Scenario: No blockers
- **WHEN** the Ticket waits on nothing
- **THEN** the blocker key is present as an empty list rather than absent
