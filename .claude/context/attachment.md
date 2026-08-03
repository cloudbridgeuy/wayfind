# Behavior: Attachments

Requirements for filing a document against a **Ticket**, pointing other Tickets
at it, reading it back, and deleting it.

## Behavior

### Requirement: Filing a document
An **Attachment** is filed against one **Ticket** of the **Initiative** in play,
with a description and a name.

#### Scenario: From a file
- **WHEN** `wayfind attach add TICKET --file PATH --description D` runs
- **THEN** the file's contents are stored under the file's own base name
- **AND** the Ticket is returned with its Attachment count raised

#### Scenario: A chosen name
- **WHEN** `--name N` is given
- **THEN** the document is filed as N whatever the source was called

#### Scenario: From a pipe
- **WHEN** the source is `--file -`
- **THEN** `--name` is required, because a pipe has no name to lend

#### Scenario: A ticket of another initiative
- **WHEN** the Ticket is not in the Initiative in play
- **THEN** the command refuses

### Requirement: Content limits
A stored document is text, at most one mebibyte, and is stored without its
terminating line feed.

#### Scenario: An empty document
- **WHEN** the source holds no bytes
- **THEN** the command refuses

#### Scenario: A document above the cap
- **WHEN** the source is larger than 1 048 576 bytes
- **THEN** the command refuses and names the cap

#### Scenario: A document holding a NUL
- **WHEN** the source holds a NUL byte
- **THEN** the command refuses, because Attachments are text only

#### Scenario: Trailing line feeds
- **WHEN** the source ends in line feeds
- **THEN** exactly one is removed, and any others are kept

### Requirement: Moving the source
`--move` deletes the source file, but only once the store has confirmed what it
holds.

#### Scenario: A matching store
- **WHEN** the stored size equals the size that was read
- **THEN** the source file is deleted

#### Scenario: A mismatch
- **WHEN** the stored size differs
- **THEN** the command refuses and the source file is kept

### Requirement: References
A **Reference** points a **Ticket** at an **Attachment** another Ticket owns.
Both ends must be in the **Initiative** in play.

#### Scenario: Referencing another ticket's document
- **WHEN** `wayfind attach ref TICKET --attachment ID` runs
- **THEN** the Reference is recorded, and the Ticket counts it separately from
  the documents it owns

#### Scenario: The same reference again
- **WHEN** the Reference is already recorded
- **THEN** the command succeeds and nothing changes

#### Scenario: Referencing your own document
- **WHEN** the Ticket already owns the Attachment
- **THEN** the command refuses

#### Scenario: Dropping a reference
- **WHEN** `wayfind attach unref TICKET --attachment ID` runs
- **THEN** the Reference is dropped and the Attachment itself stays

### Requirement: Listing documents
`wayfind attach list` reports the **Initiative**'s documents as one row each,
with the owning **Ticket**, the name, the size, and the description.

#### Scenario: Every document
- **WHEN** no Ticket is named
- **THEN** every Attachment of the Initiative is listed

#### Scenario: One ticket's documents
- **WHEN** a Ticket is named
- **THEN** only that Ticket's Attachments are listed

#### Scenario: None at all
- **WHEN** the Initiative holds no Attachments
- **THEN** the table is a header row on its own

### Requirement: Reading a document
`wayfind attach show` prints a heading and then the stored bytes. `--raw` prints
the bytes alone. Reading resolves through the **Project**, not the current
**Initiative**.

#### Scenario: With a heading
- **WHEN** `wayfind attach show ID` runs
- **THEN** the front matter and heading end at a rule, and the document follows
  untouched

#### Scenario: Raw
- **WHEN** `--raw` is given
- **THEN** the stored bytes are written with no heading, followed by one line
  feed
- **AND** `wayfind attach show ID --raw > file` reproduces the file that was
  filed

#### Scenario: A document of a closed initiative
- **WHEN** the owning Ticket is in an Initiative that is no longer active
- **THEN** the document is still readable, because it belongs to the Project

### Requirement: Deleting a document
Deleting an **Attachment** takes every **Reference** to it as well.

#### Scenario: Deleting
- **WHEN** `wayfind attach rm ID` runs
- **THEN** the Attachment and every Reference to it are gone
- **AND** the owning Ticket is returned
