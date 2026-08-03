# Wayfind — development guidance

Wayfind is a small, agent-oriented issue tracker for long-running wayfinding
work. This workspace is the Rust port of the original Bash script. It reads and
writes the same SQLite database that the Bash script created.

## Architecture

Two crates, split by the Functional Core – Imperative Shell pattern
(`~/.claude/patterns/functional-core-imperative-shell.md`).

- `crates/core` (`wayfind_core`) is the **functional core**. It holds strict
  domain values, classification and graph logic, capability traits, typed
  commands and outcomes, and every render function. It performs no I/O: no
  clock, no filesystem, no environment, no database.
- `crates/cli` (`wayfind_cli`, binary `wayfind`) is the **imperative shell**. It
  owns Clap, layered configuration, standard input, the filesystem, `now`, and
  the Rusqlite adapters. It contains no business rule.

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
- Business-rule tests are unit tests inside `crates/core`. `crates/cli` carries
  only the focused SQLite/FTS5 compatibility integration test.
- Output is compared **semantically**: front-matter key/value pairs, Markdown
  heading order, CSV records. Raw attachment bytes are the one exact
  comparison. Do not write byte snapshots for prose output.
- `cargo run -p xtask -- lint` is the gate. It runs fmt, check, clippy, test, a
  1000-line file-length cap, and a ban on
  `#[allow(clippy::too_many_arguments)]`. Split a long file into a module
  directory rather than raising the cap; group parameters into a struct rather
  than allowing the lint.

## Deliberate differences from the Bash script

Verified against a copy of the live database: `map`, `tree`, `handoff`,
`sessions`, `next`, `search`, `ticket`, `attach list`, `attach show` and
`attach show --raw` are byte-identical. Three things differ on purpose.

- **`dump --csv` header.** The script leaked its own SQL —
  `"replace(t.question, char(10), ' ')"` — where the column name belongs. The
  port writes `question`. The records are identical.
- **Argument style.** Idiomatic Clap options replace positions:
  `initiative create --name … --destination …`, `ticket block ID --by N`,
  `attach ref TICKET --attachment N`, `fog add --note …`,
  `scope exclude --note …`.
- **Escaping.** TOML control characters and Markdown table pipes are escaped.

Also narrower than the script: a document piped on standard input must carry
`--name` (the script's basename-of-`-` branch was dead code), and
`attachments.byte_size` records the stored length, one byte less than the
script's for a newline-terminated file.

Changing any of these needs an explicit decision and a focused compatibility
test.

## Common commands

```sh
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt -- --check
cargo run -p xtask -- lint
bash tests/smoke.sh target/debug/wayfind
```

## Reference

The behavioral reference is the Bash script at commit
`0120d614c778e7e865f54ae2709e84f1d9b22d03` in the `Personal/scripts`
repository. When behavior is unclear, read that script — it wins over prose.
