# Behavior: Initiatives

Requirements for creating, classifying, and closing an **Initiative**, and for
the documents that report one.

## Behavior

### Requirement: Project registration
`wayfind init` records the **Project** the command was run in and brings the
database into being if it is not there yet. Running it again changes nothing.

#### Scenario: A fresh machine
- **WHEN** `wayfind init` runs in a directory with no database
- **THEN** the database file and its schema are created
- **AND** the Project is recorded with the time it was first seen

#### Scenario: Already registered
- **WHEN** `wayfind init` runs again in the same Project
- **THEN** the first recorded time is kept

### Requirement: Creating an initiative
Creating an **Initiative** requires a name and a **Destination**, and makes it
the Project's active one. Notes are optional.

#### Scenario: A new initiative
- **WHEN** `wayfind initiative create --name N --destination D` runs
- **THEN** an Initiative named N is created with state `charting`
- **AND** later commands in that Project act on it unless `--initiative` names
  another

### Requirement: Initiative state
An **Initiative** is `clear`, `charting`, `complete`, `blocked`, or `ready`, and
the checks run in that order.

#### Scenario: Cleared
- **WHEN** the Initiative's stored status is clear
- **THEN** its state is `clear`, whatever its Tickets say

#### Scenario: No tickets
- **WHEN** the Initiative holds no Tickets
- **THEN** its state is `charting`

#### Scenario: Everything settled
- **WHEN** every Ticket is resolved or excluded
- **THEN** its state is `complete`

#### Scenario: Work is outstanding and the frontier is empty
- **WHEN** open Tickets remain but none is on the **Frontier**
- **THEN** its state is `blocked`
- **AND** the reason names either the count of claims holding the Frontier or
  that every open Ticket is blocked

#### Scenario: Work is available
- **WHEN** at least one Ticket is on the Frontier
- **THEN** its state is `ready` and the whole Frontier is reported

### Requirement: Clearing an initiative
An **Initiative** may be cleared only when no work is outstanding.

#### Scenario: Unresolved tickets remain
- **WHEN** `wayfind initiative clear` runs with open or claimed Tickets
- **THEN** the command refuses and reports how many are unresolved
- **AND** the Initiative's status is unchanged

#### Scenario: Nothing outstanding
- **WHEN** every Ticket is resolved or excluded
- **THEN** the Initiative's status becomes clear
- **AND** it is no longer the Project's active Initiative, but stays readable

### Requirement: Fog and exclusions
**Fog** and **Exclusions** are recorded against the **Initiative** in play and
are reported as two separate lists.

#### Scenario: Recording fog
- **WHEN** `wayfind fog add --note T` runs
- **THEN** T is recorded as Fog on the current Initiative
- **AND** the Map is returned

#### Scenario: Recording an exclusion
- **WHEN** `wayfind scope exclude --note T` runs
- **THEN** T is recorded as an Exclusion on the current Initiative
- **AND** the Map is returned

### Requirement: The map
`wayfind map` reports the **Initiative** in a fixed section order: destination,
notes, frontier, decisions so far, not yet specified, out of scope.

#### Scenario: A clear initiative
- **WHEN** the Initiative is clear
- **THEN** the Frontier section says so and names the next step instead of
  listing nothing

#### Scenario: No notes
- **WHEN** the Initiative carries no notes
- **THEN** the notes section is absent, and every other section keeps its place

#### Scenario: A long decision
- **WHEN** a Decision is longer than the clamp
- **THEN** the Map shows its **Gist**, ending in an ellipsis

### Requirement: The tree
`wayfind tree` draws one row per **Ticket**, ordered by depth and then by
descending identifier, with a mark for its status and its **Blockers** listed in
ascending identifier order.

#### Scenario: An empty initiative
- **WHEN** the Initiative holds no Tickets
- **THEN** the output is the heading and nothing else

#### Scenario: An edge naming a ticket that is not drawn
- **WHEN** a Dependency names a Ticket outside this Initiative
- **THEN** that edge is left out of the drawing rather than breaking it

#### Scenario: A title on two lines
- **WHEN** a Ticket's title holds a line break
- **THEN** its row stays on one line

### Requirement: The handoff
`wayfind handoff` prints every **Decision** in full, in decision order, one
heading per Decision, and counts what it carries.

#### Scenario: A clear initiative
- **WHEN** every Ticket is resolved
- **THEN** the unresolved section is absent

#### Scenario: Open work remains
- **WHEN** unresolved Tickets remain
- **THEN** the front matter counts them and the document warns about them

#### Scenario: A long decision
- **WHEN** a Decision is longer than the Map's clamp
- **THEN** the Handoff prints it whole rather than as a Gist
