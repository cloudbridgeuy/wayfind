# Behavior: Configuration and storage

Requirements for deciding what to open, which **Project**, **Session**, and
**Initiative** a command belongs to, and how the port shares a database with the
Bash script.

## Behavior

### Requirement: Configuration layers
Four layers decide what a command opens: defaults, the configuration file, the
environment, and the command line. Each setting is decided on its own, and a
later layer wins.

#### Scenario: A later layer wins
- **WHEN** the file names one database and the command line names another
- **THEN** the command line's is opened

#### Scenario: A layer that says nothing
- **WHEN** a layer leaves a setting unset
- **THEN** the layer below it decides that setting, and the others are unchanged

#### Scenario: An empty environment variable
- **WHEN** a `WAYFIND_`-prefixed variable is set to the empty string
- **THEN** it says nothing, rather than forcing an empty value

#### Scenario: Environment variable names
- **WHEN** a setting is written `[sqlite-fts5] table` in the file
- **THEN** its variable is `WAYFIND_SQLITE_FTS5__TABLE` — `__` for the dot, `_`
  for the dash

#### Scenario: A misspelled key
- **WHEN** the configuration file holds a key Wayfind does not know
- **THEN** the command refuses rather than ignoring it

#### Scenario: An unknown backend
- **WHEN** a selected backend is not one this build offers
- **THEN** the command refuses and says what this build offers

#### Scenario: A setting for a backend that was not selected
- **WHEN** settings are given for a backend other than the selected one
- **THEN** they are ignored

### Requirement: The configuration file
The file named by `--config` must exist and must parse. The default file may be
missing, but a default file that exists and does not parse is an error.

#### Scenario: A named file that is not there
- **WHEN** `--config PATH` names a file that does not exist
- **THEN** the command refuses

#### Scenario: No default file
- **WHEN** no configuration file exists at the default place
- **THEN** the command runs on the layers below it

#### Scenario: Malformed text
- **WHEN** a configuration file that exists cannot be parsed
- **THEN** the command refuses and gives the reason

### Requirement: The configuration home and the default database
The configuration home is `$XDG_CONFIG_HOME/wayfind` when that variable is set,
`$HOME/.config/wayfind` otherwise. The default database is `wayfind.sqlite`
inside it — the file the Bash script created, in the place it created it.

#### Scenario: Neither variable is set
- **WHEN** neither `XDG_CONFIG_HOME` nor `HOME` is set, and no layer names a
  database
- **THEN** the command refuses and says why, rather than guessing a path

#### Scenario: Paths are used as written
- **WHEN** a configured path holds `~` or `$VAR`
- **THEN** it is not expanded, because the shell already expands what is typed

### Requirement: Database creation is restricted
Only `wayfind init` and `wayfind initiative create` may bring a database into
being.

#### Scenario: A mistyped path
- **WHEN** any other command names a database file that is not there
- **THEN** the command refuses, rather than leaving an empty database behind it

### Requirement: Choosing the project
A command belongs to the git checkout root of the directory it was run in, or to
that directory itself when it is not in a checkout.

#### Scenario: Inside a checkout
- **WHEN** the command runs anywhere inside a git checkout
- **THEN** the Project is the checkout root

#### Scenario: Outside a checkout
- **WHEN** the directory is not in a checkout
- **THEN** the Project is the directory itself

#### Scenario: An explicit project
- **WHEN** `--project PATH` is given
- **THEN** it wins over the checkout around it

### Requirement: Choosing the session
The **Session** is named by `--session`, else `WAYFIND_SESSION_ID`, else
`CLAUDE_SESSION_ID`. Four commands act as a Session and refuse without one:
`session resume`, `ticket claim`, `ticket resolve`, and `attach add`. Every
other command, `ticket create` included, runs without a Session.

#### Scenario: The flag wins
- **WHEN** both the flag and the variables are set
- **THEN** the flag is used, and `WAYFIND_SESSION_ID` beats `CLAUDE_SESSION_ID`

#### Scenario: Nothing names a session
- **WHEN** none is set and the command acts as a Session
- **THEN** the command refuses and says how to name one

#### Scenario: A command that does not act as a session
- **WHEN** `ticket create` runs with no Session named
- **THEN** the Ticket is created, because it records no Session

#### Scenario: A session stays where it started
- **WHEN** a Session that appeared in one Initiative is used in another
- **THEN** the command refuses

### Requirement: Choosing the initiative
A command acts on the **Project**'s newest unfinished **Initiative**, unless
`--initiative ID` names another.

#### Scenario: The active initiative
- **WHEN** no `--initiative` is given
- **THEN** the newest Initiative that is not clear is used

#### Scenario: A cleared initiative
- **WHEN** an Initiative is cleared
- **THEN** it stops being the active one, but `--initiative ID` still reads it

### Requirement: Compatibility with the Bash script's database
Wayfind opens the database the Bash script created and writes rows the script
can read. Schema changes are additive.

#### Scenario: A script database
- **WHEN** Wayfind opens a database the script created
- **THEN** every Ticket, Decision, Session, Dependency, and Attachment parses,
  and the write-ahead journal mode is left as it is

#### Scenario: The amended_at column
- **WHEN** the database predates amendments
- **THEN** the `amended_at` column is added; when it is already there, nothing
  changes

#### Scenario: Identifiers
- **WHEN** Wayfind allocates an identifier
- **THEN** it starts above the rows the script wrote and never repeats one

#### Scenario: Reading back what was written
- **WHEN** Wayfind creates an Initiative, Tickets, Dependencies, and
  Attachments in a script database
- **THEN** the script reads the same Map, Tree, and Handoff that Wayfind does
- **AND** the database reports no foreign-key violation and no FTS5 corruption

### Requirement: Impossible stored data is refused
A stored record whose columns cannot describe a real state is reported as
corrupt rather than guessed at.

#### Scenario: A claimed ticket with no live claim
- **WHEN** a Ticket's status is claimed and no claim row holds it
- **THEN** reading it reports corrupt data

#### Scenario: A resolved ticket with no decision
- **WHEN** a Ticket's status is resolved and its Decision is missing
- **THEN** reading it reports corrupt data

#### Scenario: A closed session still holding a ticket
- **WHEN** a Session's status is closed and it still names a current Ticket
- **THEN** reading it reports corrupt data

#### Scenario: An unknown status
- **WHEN** a Ticket or Session carries a status word outside the check
  constraint
- **THEN** reading it reports corrupt data
