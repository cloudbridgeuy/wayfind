# Wayfind

Wayfind is a small issue tracker for long-running wayfinding work. It keeps one
initiative at a time: the destination, the tickets that stand between you and
it, the decision each ticket settled on, the dependency graph, the fog, and the
handoff you write when the work moves to someone else.

Every command answers with TOML front matter followed by Markdown, so an agent
can parse the answer and a person can read it.

This workspace is the Rust port of the original Bash script. It reads and writes
the same SQLite database.

## Install

```sh
cargo build --release
cp target/release/wayfind ~/.local/bin/wayfind
```

The binary carries its own SQLite build, so it needs nothing installed beside it.

## Start

```sh
wayfind init
wayfind initiative create \
  --name "Port the tracker" \
  --destination "The Rust binary replaces the script with no data migration." \
  --notes "The database is shared, so schema changes must stay additive."
```

`init` registers the directory you are in as a project and creates the database
if it is not there yet. Every later command belongs to that project.

## Work an initiative

```sh
wayfind ticket create --title "Storage trait shape" --type research \
  --question "What trait shape supports both SQLite and DynamoDB?"
wayfind ticket block 3 --by 2          # ticket 3 waits for ticket 2
wayfind next                           # the one ticket to work on now
wayfind ticket claim 2                 # take it for this session
wayfind ticket resolve 2 --resolution "Typed capability traits, plus explicit atomic workflows."
wayfind ticket amend 2 --resolution "Corrected text."
```

A ticket is `open`, `claimed`, or `resolved`. Only the session that claimed a
ticket may resolve it. A session may settle **one** non-research ticket, ever;
research tickets are free. A dependency that would close a loop is refused and
the loop is named.

Read the map, the graph, and the digest:

```sh
wayfind map                            # frontier, decisions, fog, exclusions
wayfind tree                           # the dependency graph
wayfind handoff                        # every decision in full
wayfind ticket 2                       # one ticket
wayfind sessions list                  # who is working on what
wayfind session resume                 # where this session left off
```

Record what you do not yet know, and what you will not do:

```sh
wayfind fog add --note "Cutover and distribution are undecided."
wayfind scope exclude --note "The DynamoDB backend is out of scope."
```

Close it when nothing is outstanding:

```sh
wayfind initiative clear
```

## Attach documents

An attachment is a body of text — a transcript, a benchmark, a specification —
filed against one ticket and readable from any other.

```sh
wayfind attach add 2 --file notes.md --description "Research notes"
wayfind attach add 2 --file - --description "Piped notes" --name piped.md
wayfind attach add 2 --file draft.md --description "Draft" --move
wayfind attach ref 3 --attachment 5    # ticket 3 points at ticket 2's document
wayfind attach unref 3 --attachment 5
wayfind attach list                    # every document on the initiative
wayfind attach list 2                  # only the ones on ticket 2
wayfind attach show 5                  # heading, then the document
wayfind attach show 5 --raw > notes.md # the stored bytes alone
wayfind attach rm 5
```

Documents are text only, at most one mebibyte. A document read from a pipe has
no name to lend, so `--name` is required. `--move` deletes the source only after
the store confirms what it holds.

## Search and export

```sh
wayfind search "sqlite OR fts5"        # FTS5 syntax, passed through
wayfind search rusqlite --limit 5 --offset 5
wayfind dump --csv                     # id,title,type,status,question,resolution
wayfind dump --csv --limit 500 > tickets.csv
```

Both are scoped to the initiative in play.

## Configuration

Four layers. Each setting is decided on its own, and a later layer wins:

1. **Defaults** — the database under the configuration home.
2. **The configuration file** — `$XDG_CONFIG_HOME/wayfind/config.toml`, or the
   file named by `--config`. The default file may be missing; a file that exists
   and does not parse is an error, and so is a `--config` file that is missing.
3. **The environment** — `WAYFIND_`-prefixed variables. The name follows the
   setting, with `__` where the file has a dot and `_` where it has a dash:
   `[sqlite-fts5] table` is `WAYFIND_SQLITE_FTS5__TABLE`. An empty variable says
   nothing.
4. **The command line** — `--backend`, `--search-backend`, `--sqlite.database`,
   `--sqlite-fts5.database`, `--sqlite-fts5.table`.

The configuration home is `$XDG_CONFIG_HOME/wayfind` when that variable is set,
`$HOME/.config/wayfind` otherwise. If neither variable is set and no layer names
a database, the command refuses instead of guessing.

```toml
# ~/.config/wayfind/config.toml
backend = "sqlite"
search-backend = "sqlite-fts5"

[sqlite]
database = "/Users/you/.config/wayfind/wayfind.sqlite"

[sqlite-fts5]
database = "/Users/you/.config/wayfind/wayfind.sqlite"
table = "ticket_search"
```

Paths are used exactly as written. A `~` or a `$VAR` inside a configured path is
not expanded — the shell already does that for anything you type.

### The existing database is the default

With no configuration at all, Wayfind opens
`$XDG_CONFIG_HOME/wayfind/wayfind.sqlite` — the file the Bash script created, in
the place it created it. There is no import step and no conversion: the port
reads the script's rows, writes rows the script can read, and adds the
`amended_at` column only if it is absent.

Only `init` and `initiative create` may bring a database into being. Every other
command opens what is there, so a mistyped path says so instead of leaving an
empty database behind it.

### Which project, which session, which initiative

- **Project** — the git checkout root of the current directory, or the current
  directory itself when it is not in a checkout. `--project PATH` overrides it.
- **Session** — `--session ID`, else `WAYFIND_SESSION_ID`, else
  `CLAUDE_SESSION_ID`. A command that writes refuses without one.
- **Initiative** — the project's newest unfinished one. `--initiative ID`
  overrides it.

## Architecture

Two crates, split by the Functional Core – Imperative Shell pattern.

- `crates/core` (`wayfind_core`) holds strict domain values, classification and
  graph logic, capability traits, typed commands and outcomes, and every render
  function. It performs no I/O: no clock, no filesystem, no environment, no
  database.
- `crates/cli` (`wayfind_cli`, binary `wayfind`) owns Clap, layered
  configuration, standard input, the filesystem, `now`, and the Rusqlite
  adapters. It contains no business rule.

The storage boundary is a set of small, object-safe capability traits —
`EntityReader`, `EntityWriter`, `IdAllocator`, `AtomicWorkflows`,
`AttachmentStore` — with `SearchBackend` separate beside them. The SQLite
adapter implements them today; another backend can implement the same traits
without SQL-shaped assumptions leaking into the core.

## Develop

```sh
cargo run -p xtask -- lint             # fmt, check, clippy, test, file length, arg ban
cargo run -p xtask -- lint --fix       # apply what fmt and clippy can fix
cargo run -p xtask -- lint --install-hooks
cargo test --workspace --all-targets
bash tests/smoke.sh target/debug/wayfind
```

`xtask lint` writes each check's full output to `target/xtask-lint.log`.

### Never develop against the live database

`~/.config/wayfind/wayfind.sqlite` holds real work. Copy it first:

```sh
WORK=$(mktemp -d)
cp ~/.config/wayfind/wayfind.sqlite "$WORK/wayfind.sqlite"
target/debug/wayfind \
  --sqlite.database "$WORK/wayfind.sqlite" \
  --sqlite-fts5.database "$WORK/wayfind.sqlite" \
  map
```

`tests/smoke.sh` sets its own `XDG_CONFIG_HOME` for the same reason, so it never
reads or writes the real file.

## Compatibility with the Bash script

Output is compared **semantically**, not byte for byte: front-matter key/value
pairs, Markdown heading order, CSV records. Raw attachment bytes are the one
exact comparison.

Verified against a copy of the live database, `map`, `tree`, `handoff`,
`sessions`, `next`, `search`, `ticket`, `attach list`, `attach show` and
`attach show --raw` are byte-identical to the script. Three things differ on
purpose:

- **`dump --csv` header.** The script leaked its own SQL — `"replace(t.question,
  char(10), ' ')"` — where the column name belongs. Wayfind writes `question`.
  The records themselves are identical.
- **Argument style.** Wayfind uses idiomatic Clap options where the script used
  positions: `initiative create --name … --destination …`, `ticket block ID --by
  N`, `attach ref TICKET --attachment N`, `fog add --note …`.
- **Escaping.** TOML control characters and Markdown table pipes are escaped, so
  a title holding either stays inside its own value and its own cell.

The script can still read everything the port writes.

## Non-goals

- **Alternating or concurrent use of both implementations.** The port opens the
  script's database so no data moves. It does not promise a parallel-run period,
  forward compatibility for later script writes, or rollback safeguards.
- **Byte-for-byte output parity.** See above.
- **Bash-compatible help and argument errors.** Clap's are used instead.
- **A DynamoDB backend.** The trait shape admits one; nothing implements it.
- **Cutover.** How the built binary replaces the `~/.local/bin/wayfind` symlink
  is undecided, and stays undecided until a separate decision selects it.
