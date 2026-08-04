# Wayfind — development guidance

Wayfind is a small, agent-oriented issue tracker for long-running wayfinding
work.

## Architecture

Two product lines share this workspace while v2 is built out: the daily v1
tool, and the v2 rewrite growing behind its own binary until it can replace
v1. Each line is a pair of crates, split by the Functional Core – Imperative
Shell pattern (`~/.claude/patterns/functional-core-imperative-shell.md`).

- **v1** (`crates/legacy-core`, package `wayfind_v1_core`; `crates/legacy-cli`,
  package `wayfind_v1_cli`, binary `wayfind`) is the tool described by the rest
  of this file's `## Behavior` section. It is not touched by v2 work.
- **v2** (`crates/core`, package `wayfind_core`; `crates/cli`, package
  `wayfind_cli`, binary `wayfind2`) is the rewrite. `crates/core` is the
  **functional core**: strict domain values, classification and graph logic,
  capability traits, typed commands and outcomes, and every render function.
  It performs no I/O: no clock, no filesystem, no environment, no database.
  `crates/cli` is the **imperative shell**: it owns Clap, layered
  configuration, standard input, the filesystem, `now`, and the Rusqlite
  adapters. It contains no business rule. v2's current behavior is tracked in
  [v2 store lifecycle](.claude/context/v2-store.md).

Neither crate in either line may name a type from the other line — a file
under `crates/core` or `crates/cli` that contains the text `wayfind_v1_` fails
the lint gate's `no-legacy-dependency` check.

The storage boundary is a set of small, object-safe capability traits. The
SQLite adapter implements them today; another backend can implement the same
traits without SQL-shaped assumptions leaking into the core.

## Rules

- **Never point a development command at `~/.config/wayfind/wayfind.sqlite`.**
  Copy the database to a temporary directory first. `tests/smoke.sh` sets its
  own `XDG_CONFIG_HOME` for exactly this reason. To work against real data:

  ```sh
  WORK=$(mktemp -d)
  cp ~/.config/wayfind/wayfind.sqlite "$WORK/wayfind.sqlite"
  target/debug/wayfind \
    --sqlite.database "$WORK/wayfind.sqlite" \
    --sqlite-fts5.database "$WORK/wayfind.sqlite" \
    map
  ```

  Fingerprint the live file with `shasum -a 256` before and after, and check it
  did not move.
- The database is shared with the Bash script. Schema changes must be additive
  and must not change the meaning of an existing column or table.
- `#![deny(clippy::unwrap_used, clippy::expect_used)]` is set in both crate
  roots. Test modules may still use `.unwrap()`.
- Business-rule tests are unit tests inside each line's core crate
  (`crates/legacy-core` for v1, `crates/core` for v2). Each line's `cli` crate
  carries only the focused SQLite/FTS5 compatibility integration test.
- Output is compared **semantically**: front-matter key/value pairs, Markdown
  heading order, CSV records. Raw attachment bytes are the one exact
  comparison. Do not write byte snapshots for prose output.
- `cargo run -p xtask -- lint` is the gate. It runs fmt, check, clippy, test, a
  1000-line file-length cap, a ban on `#[allow(clippy::too_many_arguments)]`,
  and a ban on `crates/core` or `crates/cli` naming a v1 type. Split a long
  file into a module directory rather than raising the cap; group parameters
  into a struct rather than allowing the lint.

## Behavior

`CONTEXT.md` holds the language and the relationships. What the system does
lives in topic files, one requirement per behavior:

- [Initiatives](.claude/context/initiative.md) — charting, clearing, fog,
  scope, and the map, tree, and handoff documents
- [Tickets](.claude/context/ticket.md) — creating, claiming, resolving,
  amending, dependencies, and the frontier
- [Attachments](.claude/context/attachment.md) — filing, referencing, reading,
  and deleting documents
- [Search and export](.claude/context/query.md) — full-text search and CSV
  records
- [Configuration and storage](.claude/context/configuration.md) — the
  configuration layers, project and session selection, and compatibility with
  the Bash script's database
- [Output shape](.claude/context/output.md) — the document contract, and the
  three deliberate differences from the script
- [v2 store lifecycle](.claude/context/v2-store.md) — creating and opening the
  `wayfind2` store, and how an unimplemented v2 command answers

Changing a deliberate difference needs an explicit decision and a focused
compatibility test.

## Common commands

```sh
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt -- --check
cargo run -p xtask -- lint
bash tests/smoke.sh target/debug/wayfind
```
