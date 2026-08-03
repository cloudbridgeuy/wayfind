# Behavior: Search and export

Requirements for the two commands that read many **Tickets** at once.

## Behavior

### Requirement: Search
`wayfind search` answers full-text queries over the **Initiative** in play, in
relevance order, with a marked snippet per hit.

#### Scenario: A hit
- **WHEN** a query matches a Ticket's title, question, or **Decision**
- **THEN** the hit reports the Ticket's identifier, title, status, and a snippet
  of its question with the matched terms marked

#### Scenario: No hits
- **WHEN** nothing matches
- **THEN** the answer is an empty page, not a failure

#### Scenario: Another initiative
- **WHEN** a matching Ticket belongs to a different Initiative
- **THEN** it is not returned

#### Scenario: A ticket written a moment ago
- **WHEN** a Ticket was created or resolved in the same run
- **THEN** it is already searchable

#### Scenario: Query syntax
- **WHEN** the query uses FTS5 syntax such as `near/3`, `title:backend`, or a
  quoted phrase
- **THEN** it is passed through and answered
- **AND** a query FTS5 cannot parse is reported as a query problem, told apart
  from a broken backend

#### Scenario: Paging
- **WHEN** `--limit N --offset M` are given
- **THEN** at most N hits are returned, starting after M
- **AND** paging forward walks the whole result set once, without wrapping
- **AND** a limit of zero, or above 500, is refused

#### Scenario: Ranking
- **WHEN** two hits score the same
- **THEN** they come back in ascending Ticket identifier order

### Requirement: Export
`wayfind dump --csv` writes the **Initiative**'s **Tickets** as comma-separated
records. `--csv` is required and is the only format.

#### Scenario: Records
- **WHEN** `wayfind dump --csv` runs
- **THEN** the header is `id,title,type,status,question,resolution` and one
  record follows per Ticket

#### Scenario: Awkward text
- **WHEN** a title, question, or Decision holds a comma, a quotation mark, or a
  line break
- **THEN** the record is still readable as a record, with the text preserved

#### Scenario: No tickets
- **WHEN** the Initiative holds no Tickets
- **THEN** the output is a header row on its own

#### Scenario: Paging
- **WHEN** `--limit N --offset M` are given
- **THEN** at most N Tickets are written, starting after M
