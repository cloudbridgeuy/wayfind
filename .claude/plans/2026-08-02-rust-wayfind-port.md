# Rust Wayfind Port Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use the executing skill chosen at handoff (subagent-driven-development, executing-plans, or executing-plans-solo) to implement this plan task-by-task.

**Goal:** Replace the Bash Wayfind script with a Clap-driven Rust workspace that preserves command semantics and existing SQLite data while keeping domain logic pure and storage portable.

**Architecture:** `wayfind_core` is the functional core: strict domain types, pure classification and graph logic, typed storage/search capabilities, commands, outcomes, and render models. `wayfind_cli` is the imperative shell: Clap, layered configuration, filesystem/stdin/clock access, Rusqlite adapters, orchestration, and output. The SQLite adapter opens the Bash database in place; a future DynamoDB adapter can implement the same explicit capabilities without SQL-shaped assumptions.

**Tech Stack:** Rust 1.91.1, Cargo workspace, Clap 4.5, thiserror 2, color-eyre 0.6, serde/toml/serde_json, chrono, rusqlite 0.40 with `bundled`, csv, tempfile, xtask, GitHub Actions.

---

## Settled constraints and references

- Follow Functional Core–Imperative Shell for every implementation decision: `~/.claude/patterns/functional-core-imperative-shell.md`. Keep all business rules in `crates/core`; keep I/O in `crates/cli` [13, 19].
- Apply Parse, Don't Validate and type-driven design at CLI, configuration, and persisted-record boundaries. Invalid persisted combinations return `StorageError::CorruptData` [19].
- Use the current Bash script at commit `0120d614c778e7e865f54ae2709e84f1d9b22d03` as the behavioral reference. It supersedes stale lifecycle text in attachment 5 [19].
- Open and modify the existing Bash-created database directly. Do not add an import or conversion step [14].
- Keep output structure and meaning stable. Compare TOML values and Markdown structure, not bytes. Raw attachment output stays exact, and CSV stays record-compatible [15].
- Use `rusqlite = { version = "0.40", features = ["bundled"] }`; enable foreign keys per connection and preserve WAL. Do not use `bundled-full` or loadable extensions [17].
- Search is a separate required `SearchBackend`; it is not a `Storage` supertrait and core does not scan as fallback [16].
- Use synchronous, object-safe capability traits, explicit ID allocation, named bounded atomic workflows, explicit consistency, opaque attachment bytes, and revision-stabilized reads [18, 19].
- Initial selectors are only `sqlite` and `sqlite-fts5`. Parse inactive reserved sections but reject unavailable selections [20].
- Business-rule tests are unit tests in `crates/core`. The only CLI integration test required by this plan is the focused Bash-database/FTS5 compatibility test required by [17].
- Do not implement DynamoDB, cutover/distribution, parallel Bash/Rust use, rollback safeguards, or a future migration framework.

## Task 1: Bootstrap the workspace and convention files [13]

**Files:**
- Create: `.gitignore`, `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `clippy.toml`, `deny.toml`, `cliff.toml`, `bacon.toml`, `CLAUDE.md`
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`, `xtask/Cargo.toml`, `xtask/src/main.rs`

**Interfaces:**
- Produces: workspace packages `wayfind_core`, `wayfind_cli` (binary name `wayfind`), and `xtask`.

1. Run `git init -b main` because the target directory is empty and not a repository. Expected: `Initialized empty Git repository`.
2. Write the workspace manifest with members `crates/core`, `crates/cli`, and `xtask`; edition 2021; Rust 1.91.1; `publish = false`; the exact lint and profile tables from attachment 3.
3. Add only used workspace dependencies: `chrono`, `clap`, `color-eyre`, `csv`, `rusqlite`, `serde`, `serde_json`, `tempfile`, `thiserror`, and `toml`. Do not copy Forgeguard's full dependency list.
4. Add `#![deny(clippy::unwrap_used, clippy::expect_used)]` to both crate roots, outside `#[cfg(test)]` modules.
5. Copy the pinned toolchain and Clippy thresholds from attachment 3. Add minimal deny, changelog, Bacon, ignore, and Wayfind-specific development guidance files.
6. Run `cargo check --workspace --all-targets`. Expected: all three packages compile.

## Task 2: Define strict domain values and errors [19]

**Files:**
- Create: `crates/core/src/error.rs`, `crates/core/src/id.rs`, `crates/core/src/time.rs`, `crates/core/src/model.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces: `ProjectKey`, `InitiativeId`, `TicketId`, `AttachmentId`, `DecisionId`, `SessionId`, `Timestamp`, `TicketType`, `PersistedInitiativeStatus`, `TicketState`, `InitiativeState`, `SessionState`, `ActiveSessionState`, `Error`, and `Result<T>`.

1. Write failing unit tests for ID parsing, session IDs, timestamps, closed enums, and corrupt ticket-state combinations.
2. Run `cargo test -p wayfind_core model`. Expected: compilation or assertion failures show the missing types.
3. Implement private-field newtypes with `TryFrom`/`FromStr`, accessors, `Display`, and serde support. Use these state shapes:

```rust
pub enum TicketState {
    Open,
    Claimed { claimant: SessionId, claimed_at: Timestamp },
    Resolved { resolution: String, resolved_at: Timestamp, amended_at: Option<Timestamp> },
    Excluded,
}
pub enum InitiativeState {
    Charting,
    Ready { frontier: NonEmptyVec<FrontierTicket> },
    Blocked(BlockedReason),
    Complete,
    Clear,
}
pub enum SessionState { Active(ActiveSessionState), Closed }
pub enum ActiveSessionState { Ready, Holding { ticket_id: TicketId } }
```

4. Implement `NonEmptyVec<T>::try_from(Vec<T>) -> Result<NonEmptyVec<T>>`; do not permit an empty `Ready` state.
5. Keep `Error` structured with `InvalidValue`, `InvalidTransition`, and `CorruptData`; add Display tests.
6. Run `cargo test -p wayfind_core`. Expected: all domain tests pass.

## Task 3: Define portable storage and search capabilities [16, 18, 19]

**Files:**
- Create: `crates/core/src/storage.rs`, `crates/core/src/search.rs`, `crates/core/src/command.rs`, `crates/core/src/outcome.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: domain values from Task 2.
- Produces:
  - `EntityReader`, `EntityWriter`, `IdAllocator`, `AtomicWorkflows`, `AttachmentStore`, `Storage`, `SearchBackend`.
  - `ClaimTicket`, `ResolveTicket`, `InsertDependency`, `ClaimOutcome`, `ResolveOutcome`, `InsertDependencyOutcome`.

1. Add compile-time object-safety tests with functions that accept `&dyn Storage` and `&dyn SearchBackend`.
2. Define the exact capability boundary:

```rust
pub enum Consistency { Strong, Relaxed }
pub trait Storage: EntityReader + EntityWriter + IdAllocator + AtomicWorkflows + AttachmentStore {}
pub trait EntityReader {
    fn initiative_revision(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<InitiativeRevision>;
    fn initiative(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Option<Initiative>>;
    fn tickets(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<Ticket>>;
    fn dependencies(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<Dependency>>;
    fn sessions(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<Session>>;
    fn decisions(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<Decision>>;
    fn fog_notes(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<FogNote>>;
    fn scope_exclusions(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<ScopeExclusion>>;
    fn attachment_index(&self, id: InitiativeId, consistency: Consistency) -> StorageResult<Vec<AttachmentMetadata>>;
}
pub trait IdAllocator { fn allocate(&self, scope: IdScope) -> StorageResult<AllocatedId>; }
pub trait AtomicWorkflows {
    fn claim_ticket(&self, command: ClaimTicket) -> StorageResult<ClaimOutcome>;
    fn resolve_ticket(&self, command: ResolveTicket) -> StorageResult<ResolveOutcome>;
    fn insert_dependency(&self, command: InsertDependency) -> StorageResult<InsertDependencyOutcome>;
}
pub trait AttachmentStore {
    fn store_attachment(&self, command: StoreAttachment, bytes: &[u8]) -> StorageResult<AttachmentMetadata>;
    fn read_attachment(&self, id: AttachmentId) -> StorageResult<Option<Vec<u8>>>;
    fn remove_attachment(&self, id: AttachmentId) -> StorageResult<RemoveAttachmentOutcome>;
    fn add_reference(&self, command: AddAttachmentReference) -> StorageResult<ReferenceOutcome>;
    fn remove_reference(&self, command: RemoveAttachmentReference) -> StorageResult<ReferenceOutcome>;
}
```

3. Make every atomic command name all participants, expected entity revisions, expected initiative revision, and shell-supplied timestamp. Document the future backend limit of 100 items/4 MB.
4. Define exhaustive conflict enums. Reserve `StorageError` for infrastructure, corrupt data, and capacity limits; expected conflicts are values.
5. Define `SearchRequest { query, initiative_id, limit, offset }`, relevance-ordered `SearchHit`, namespaced metadata as `BTreeMap<String, serde_json::Value>`, and deterministic pagination.
6. Run `cargo test -p wayfind_core storage search`. Expected: object-safety and type tests pass.

## Task 4: Implement pure initiative, graph, session, and transition logic [12, 19]

**Files:**
- Create: `crates/core/src/initiative.rs`, `crates/core/src/graph.rs`, `crates/core/src/session.rs`, `crates/core/src/transition.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Initiative`, `Ticket`, `Dependency`, `Session`, and typed commands from Tasks 2–3.
- Produces:
  - `classify_initiative(view: &InitiativeView) -> InitiativeState`
  - `frontier(tickets: &[Ticket], dependencies: &[Dependency]) -> Vec<FrontierTicket>`
  - `would_create_cycle(edges: &[Dependency], candidate: Dependency) -> bool`
  - `next_ticket(state: &InitiativeState) -> Option<&FrontierTicket>`
  - `prepare_claim(input: ClaimInput, view: &InitiativeView) -> Result<ClaimTicket, ClaimConflict>`
  - `prepare_resolution(input: ResolveInput, view: &InitiativeView) -> Result<ResolveTicket, ResolveConflict>`

1. Write table-driven failing tests for `Charting`, full ordered `Ready`, `Blocked`, `Complete`, and persisted `Clear`, including a successful empty-frontier classification [19].
2. Add cycle tests for self edges, duplicates, deep cycles, diamonds, and disjoint graphs.
3. Add transition tests for claim ownership, idempotent same-session claim, other-session conflict, one active ticket, and the permanent one-non-research limit at both claim and resolve [12, 19].
4. Implement pure functions. Frontier means `Open` plus all blockers `Resolved`, ordered by ticket ID. Graph traversal stays in core.
5. Add revision-stabilized read orchestration with a bounded retry policy supplied as data: read revision, strong-read records, read revision, retry on change. Do not read time or sleep in core.
6. Run `cargo test -p wayfind_core initiative graph transition`. Expected: all branches pass.

## Task 5: Implement pure formatting and render models [12, 15]

**Files:**
- Create: `crates/core/src/format.rs`, `crates/core/src/render.rs`, `crates/core/src/tree.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces:
  - `clamp_gist(text: &str) -> String`
  - `format_size(bytes: u64) -> String`
  - `strip_one_trailing_newline(bytes: &[u8]) -> &[u8]`
  - `render_map(model: &MapView) -> String`
  - `render_ticket(model: &TicketView) -> String`
  - `render_handoff(model: &HandoffView) -> String`
  - `render_tree(model: &TreeView) -> String`
  - render functions for session lists, attachments, search, and CSV rows.

1. Write failing tests for Unicode 200-character gist clamping, whitespace collapse, byte-size truncation, one-newline stripping, TOML control escaping, optional sections, and tree lanes.
2. Implement semantic TOML-plus-Markdown output with stable key and section order. Use a TOML serializer for valid strings; keep deliberate output changes narrow [15].
3. Preserve `init`, `initiative clear`, `tree`, CSV, raw attachment, help, and error output as non-front-matter formats.
4. Use the `csv` crate for `id,title,type,status,question,resolution` records and flatten LF in question/resolution. Add record-based tests, not byte snapshots.
5. Keep tree ordering and lane behavior from commit `0120d61`; use a focused snapshot only for this layout-sensitive output.
6. Run `cargo test -p wayfind_core format render tree`. Expected: all formatting tests pass.

## Task 6: Implement layered typed configuration and Clap [20]

**Files:**
- Create: `crates/core/src/config.rs`, `crates/cli/src/args.rs`, `crates/cli/src/config.rs`, `crates/cli/src/error.rs`
- Modify: `crates/core/src/lib.rs`
- Modify: `crates/cli/src/main.rs`, `crates/cli/Cargo.toml`

**Interfaces:**
- Consumes: all command input types from core.
- Produces:
  - `Cli::parse_from(...) -> Cli`
  - `resolve_config(input: ConfigInput) -> Result<ResolvedConfig>`
  - `ResolvedConfig { storage: StorageConfig, search: SearchConfig }`
  - complete typed command enum for all Bash commands, including `handoff`.

1. Put pure configuration source merging and selected-backend refinement in `crates/core/src/config.rs`. Write its unit tests in core for precedence, unknown/reserved sections, inactive settings, and required selected fields. Keep `crates/cli/src/config.rs` as untested I/O glue that reads files/environment and converts them to core configuration sources.
2. Define global Clap arguments with `global = true`, so they work before or after subcommands: `--project`, `--session`, `--initiative`, `--config`, `--backend`, `--search-backend`, and flattened dotted backend options.
3. Define every subcommand from commit `0120d61`: `init`; initiative create/clear; map/tree/next/handoff; ticket show/create/claim/resolve/amend/block; attach add/ref/unref/rm/list/show; session resume/list and sessions list; fog add; scope exclude; search; dump.
4. Parse config into `RawConfig` with `#[serde(deny_unknown_fields)]`, then convert to selected typed configs. Merge defaults < TOML < `WAYFIND_` env (`__` for nesting) < CLI.
5. Default to `$XDG_CONFIG_HOME/wayfind/{config.toml,wayfind.sqlite}` or `$HOME/.config/wayfind/...`; do not expand variables or `~` in supplied paths.
6. Ensure `--help` and `--version` exit before config or database I/O. Missing default config is valid; missing explicit or malformed config is an error.
7. Run `cargo test -p wayfind_core config`, `cargo check -p wayfind_cli --all-targets`, and `cargo run -p wayfind_cli -- --help`. Expected: core configuration tests pass, CLI wiring compiles, and help exits 0 without a database.

## Task 7: Implement the SQLite connection, schema compatibility, and record parsing [14, 17, 19]

**Files:**
- Create: `crates/cli/src/sqlite/mod.rs`, `crates/cli/src/sqlite/schema.rs`, `crates/cli/src/sqlite/row.rs`
- Create: `crates/cli/tests/fixtures/create-bash-fixture.sql`, `crates/cli/tests/sqlite_compat.rs`

**Interfaces:**
- Consumes: core domain and storage types.
- Produces: `SqliteStorage::open(path: &Path) -> Result<SqliteStorage>` and strict row parsers such as `parse_ticket(row: &Row<'_>) -> StorageResult<Ticket>`.

1. Create a Bash-schema fixture from attachment 5, including all 10 tables, FTS5 table, three triggers, representative multiline text, dependency, claim, decision, attachment, and reference rows.
2. Write the required failing integration test: copy the fixture DB to a `TempDir`; open with bundled Rusqlite; verify WAL remains active; run bound raw `MATCH` with `snippet()`; update indexed text and verify trigger visibility; assert `PRAGMA foreign_key_check` yields no rows.
3. Implement `open`: create parent directory only for `init`, open directly, execute `PRAGMA foreign_keys = ON`, and do not change journal mode on existing databases.
4. Implement only the existing additive `amended_at` compatibility check. Do not invent a general migration framework.
5. Parse joined ticket/claim/session data into strict sum types. Return `StorageError::CorruptData` for impossible persisted combinations.
6. Run `cargo test -p wayfind_cli --test sqlite_compat`. Expected: the copied Bash fixture passes FTS, trigger, WAL, and foreign-key checks.

## Task 8: Implement SQLite read, write, ID, and atomic workflow capabilities [18, 19]

**Files:**
- Create: `crates/cli/src/sqlite/read.rs`, `crates/cli/src/sqlite/write.rs`, `crates/cli/src/sqlite/atomic.rs`, `crates/cli/src/sqlite/attachment.rs`
- Modify: `crates/cli/src/sqlite/mod.rs`

**Interfaces:**
- Consumes: all traits and commands from Task 3.
- Produces: complete `Storage for SqliteStorage` implementation.

1. Implement typed prepared reads for project, current/newest-clear initiative, revision, tickets, dependencies, sessions, decisions, fog, exclusions, and attachment index. Treat SQLite reads as satisfying both consistency levels.
2. Add an adapter-private initiative revision table only if it can be added without changing existing schema meaning; initialize missing rows lazily. Every stable-view mutation compares and increments it in the same transaction. ID allocation and session touch do not increment it [19].
3. Implement explicit ID allocation per entity scope. Promise monotonic unique IDs with possible gaps; never return IDs as an implicit insert side effect.
4. Implement `claim_ticket`, `resolve_ticket`, and `insert_dependency` as transactions that match expected revisions and return exhaustive conflict outcomes. Claim and resolve must atomically cover ticket, session, claim/decision, and initiative revision.
5. Implement simple typed writes for create, amend, clear, session touch, fog, and scope exclusion. Guard implicit writes against clear initiatives while permitting explicit reads [19].
6. Implement attachment bytes as opaque `Vec<u8>` at the trait boundary while storing compatible text in SQLite. Preserve project/initiative reference rules, idempotent ref/unref, and cascading removal.
7. Run `cargo test -p wayfind_cli --test sqlite_compat` and `cargo check --workspace --all-targets`. Expected: compatibility remains green and all trait methods compile.

## Task 9: Implement SQLite FTS5 search [16, 17]

**Files:**
- Create: `crates/cli/src/sqlite/search.rs`
- Modify: `crates/cli/src/sqlite/mod.rs`

**Interfaces:**
- Consumes: `SearchRequest` and `SearchBackend` from Task 3.
- Produces: `SqliteFts5Search::search(&self, request: &SearchRequest) -> SearchResult<SearchPage>`.

1. Extend the compatibility test with initiative scoping, `limit`, `offset`, ranking ties, and immediate visibility after a write in the same process.
2. Bind the raw query string to `MATCH` so FTS5 owns syntax and parser errors. Use `snippet(ticket_search, 1, '**', '**', '…', 12)` and order by `bm25`, then ticket ID.
3. Return backend-neutral hits. Put score data only in optional `sqlite.bm25` metadata; normal rendering must not depend on it.
4. Run `cargo test -p wayfind_cli --test sqlite_compat`. Expected: FTS tests pass with bundled SQLite.

## Task 10: Implement the imperative command shell [12, 14, 15, 19]

**Files:**
- Create: `crates/cli/src/app.rs`, `crates/cli/src/context.rs`, `crates/cli/src/input.rs`, `crates/cli/src/output.rs`
- Create: `crates/cli/src/commands/{mod.rs,initiative.rs,ticket.rs,session.rs,attachment.rs,query.rs}`
- Modify: `crates/cli/src/main.rs`

**Interfaces:**
- Consumes: parsed `Cli`, `ResolvedConfig`, `dyn Storage`, `dyn SearchBackend`, core operations and render models.
- Produces: `run(cli: Cli, env: &dyn Environment, output: &mut dyn Output) -> Result<()>`.

1. Implement `Environment` as the shell boundary for cwd/git-root resolution, environment variables, stdin, filesystem, and `now`. Resolve project keys by physical path and session priority CLI > `WAYFIND_SESSION_ID` > `CLAUDE_SESSION_ID`.
2. Resolve and validate config before command database I/O, then initialize exactly one storage and one search adapter. Reject any unavailable or incomplete combination.
3. Implement each command as read → core decision → typed write → render. Keep handlers free of business rules.
4. Preserve current lifecycle semantics from commit `0120d61`: newest non-clear initiative for implicit writes, newest initiative including clear for explicit readable state/handoff, state guidance, active session listing, and full handoff decisions/attachments.
5. Implement content input once: non-empty, no NUL, raw size at most 1 MiB for files/stdin, strip at most one trailing LF for compatible storage. Inline resolution text is not size-limited [12]. Require an explicit attachment name for stdin as a deliberate narrow fix to Bash's dead branch, and add a focused compatibility assertion [15].
6. For `--move`, delete the source only after the adapter confirms a successful store and the stored size matches. Report post-write deletion failure clearly.
7. Map Clap errors idiomatically. Map stable domain errors to clear stderr text and failure; do not promise exact Bash wording or one universal nonzero code [15].
8. Run `cargo check --workspace --all-targets` and manual smoke commands against a temporary `XDG_CONFIG_HOME`. Expected: every command dispatches without touching the live database.

## Task 11: Add semantic parity fixtures and command smoke coverage [12, 15]

**Files:**
- Create: `crates/core/src/compat_tests.rs`
- Create: `tests/smoke.sh`, `tests/fixtures/expected-tree.md`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: core render functions and built `wayfind` binary.
- Produces: semantic regression coverage for all output kinds and a live-data-safe smoke script.

1. Add core tests that parse rendered front matter and compare required keys/values, assert Markdown heading order, compare CSV as records, and compare raw attachment bytes exactly. Cover map, ticket, next/session, attachment(s), search, handoff, and graph snapshot.
2. Write `tests/smoke.sh` to use `mktemp -d`, set `XDG_CONFIG_HOME`, set a fixed session, and exercise every command group. It must never read or write `~/.config/wayfind/wayfind.sqlite`.
3. Include ownership, cycle, blocked frontier, clear-readable state, attachment ref/unref/rm, amend, raw output, malformed FTS, and empty-frontier success scenarios.
4. Run `cargo test -p wayfind_core` and `bash tests/smoke.sh target/debug/wayfind`. Expected: all semantic comparisons and smoke cases pass.

## Task 12: Add xtask lint, hooks, CI, and release automation [13]

**Files:**
- Create: `xtask/src/lint/mod.rs`, `xtask/src/lint/hooks.rs`
- Modify: `xtask/src/main.rs`, `xtask/Cargo.toml`
- Create: `.github/workflows/ci.yml`, `.github/workflows/release.yml`

**Interfaces:**
- Produces: `cargo run -p xtask -- lint [--fix|--staged-only|--install-hooks|--uninstall-hooks|--hooks-status]`.

1. Port the Forgeguard lint functional-core/shell split, reduced to fmt, check, clippy, test, file length <=1000, and the ban on `#[allow(clippy::too_many_arguments)]`.
2. Keep `--fix`, staged re-format/re-stage behavior, hook lifecycle, and `target/xtask-lint.log`; remove Rail, publish, TypeScript, AWS, and crates.io operations.
3. Add fixed CI jobs for fmt, test, clippy, Ubuntu/macOS build, typos, and cargo-deny. Cache by `Cargo.lock`; do not add diff planning.
4. Add one-binary release targets for Apple Intel/ARM and Linux x86_64/ARM64, git-cliff notes, and GitHub release assets. Do not add Docker.
5. Run `cargo run -p xtask -- lint`. Expected: every check passes.

## Task 13: Verify against a copy of live data and document handoff

**Files:**
- Modify: `README.md`, `CLAUDE.md`
- Create during execution handoff: `.claude/plans/2026-08-02-rust-wayfind-port-qa.md`

**Interfaces:**
- Consumes: complete workspace and a copied live SQLite database.
- Produces: verified binary, operator documentation, and Manual QA Testing Plan.

1. Copy `~/.config/wayfind/wayfind.sqlite` into a temporary directory. Never point a development command at the live file.
2. Run read commands (`map`, `tree`, `handoff`, `session list`, `search`, `dump`) against the copy and compare semantics with Bash commit `0120d61`.
3. Run mutation commands against a second copy, then run `PRAGMA foreign_key_check`, FTS queries, and both Rust and SQLite reads of the results.
4. Run final verification:

```text
cargo fmt -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo run -p xtask -- lint
bash tests/smoke.sh target/debug/wayfind
```

Expected: all commands exit 0; no foreign-key violations; no live database writes.
5. Document configuration precedence, the existing-database default, command examples, safe development with copied data, and explicit non-goals. Do not document cutover until a separate decision selects it.
6. Use @verification-before-completion before any completion claim, @requesting-code-review for the finished implementation, and @committing to synchronize behavioral documentation and commit each task.
7. Create and actually run the Manual QA Testing Plan as required by the selected execution skill.
