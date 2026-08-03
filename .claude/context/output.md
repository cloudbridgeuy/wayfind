# Behavior: Output shape

Requirements for the document every command answers with, and how it differs
from the Bash script's.

## Behavior

### Requirement: The document contract
Most commands answer with a TOML front-matter block, then Markdown. The front
matter names the kind of document and carries the identifiers a caller needs.

#### Scenario: Front matter parses
- **WHEN** any Map, Ticket, Session, sessions, attachment, attachments, search,
  or handoff document is produced
- **THEN** the block between the `+++` fences parses as TOML
- **AND** its keys keep the order they were built in

#### Scenario: An absent optional value
- **WHEN** an optional key has no value — an unamended Ticket's amendment time,
  for example
- **THEN** the key is left out entirely rather than emptied

#### Scenario: Outside the contract
- **WHEN** the command is `init`, `dump`, `attach show --raw`, or a usage
  message
- **THEN** the output is not wrapped in front matter

### Requirement: Text stays inside its value
Text an operator wrote cannot break the document that carries it.

#### Scenario: A control character in a front-matter value
- **WHEN** a title or note holds a control character
- **THEN** it is escaped, and the value parses back to what went in

#### Scenario: A value on several lines
- **WHEN** a front-matter value holds line breaks
- **THEN** it becomes a value on one line

#### Scenario: A pipe in a table cell
- **WHEN** a title holds a `|`
- **THEN** it is escaped, and the cell does not end its row early

### Requirement: Gists are clamped by character
Where a **Decision** would be too long to list, its **Gist** is shown instead.

#### Scenario: A short decision
- **WHEN** the Decision is at or under the limit
- **THEN** it is shown unchanged, with no ellipsis

#### Scenario: A long decision
- **WHEN** the Decision is over the limit
- **THEN** it is cut at a character boundary, never mid-character, and ends with
  an ellipsis

#### Scenario: Whitespace
- **WHEN** the Decision holds line breaks, tabs, or repeated spaces
- **THEN** the Gist collapses them

### Requirement: Sizes are reported in the script's units
A stored document's size is reported in bytes below a kilobyte, and in scaled
units above it, truncated rather than rounded up.

#### Scenario: A small document
- **WHEN** a document is under 1024 bytes
- **THEN** its size is reported in bytes

#### Scenario: A larger document
- **WHEN** a document is at or over 1024 bytes
- **THEN** its size is reported in the scaled unit, with the fraction truncated

### Requirement: Deliberate differences from the Bash script
Output is compatible with the Bash script's by meaning, not by byte. Three
things differ on purpose.

#### Scenario: The dump header
- **WHEN** `wayfind dump --csv` writes its header
- **THEN** the fifth column is named `question`, where the script leaked
  `"replace(t.question, char(10), ' ')"`
- **AND** the records themselves are identical to the script's

#### Scenario: Argument style
- **WHEN** a command takes several values
- **THEN** they are named Clap options — `initiative create --name --destination`,
  `ticket block ID --by N`, `attach ref TICKET --attachment N`,
  `fog add --note`, `scope exclude --note` — where the script used positions

#### Scenario: Escaping
- **WHEN** text holds a TOML control character or a Markdown table pipe
- **THEN** it is escaped, where the script wrote it raw

#### Scenario: Everything else
- **WHEN** `map`, `tree`, `handoff`, `next`, `ticket`, `sessions`, `search`,
  `attach list`, `attach show`, or `attach show --raw` runs against the same
  database as the script
- **THEN** the output is byte-identical
