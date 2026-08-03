# Manual QA Testing Plan: Rust Wayfind port

**Source plan:** `.claude/plans/2026-08-02-rust-wayfind-port.md`
**Generated:** 2026-08-02

## Overview

The 988-line Bash `wayfind` script is now a Cargo workspace: a pure
`wayfind_core` crate and a `wayfind_cli` shell that owns Clap, configuration,
and the Rusqlite adapters. This plan validates that the binary charts an
initiative from empty to clear, keeps the same semantics as the script, refuses
what the script refused, and never touches the live database.

## Prerequisites

- Rust 1.95.0 (`rust-toolchain.toml` pins it), `cargo`, `sqlite3`, `git`,
  `python3`.
- The repository at `~/Projects/Rust/wayfind`, on `main`.
- The Bash reference script, extracted from the `Personal/scripts` repository:

  ```sh
  cd ~/Projects/Personal/scripts && git show 0120d614:scripts/wayfind > /tmp/wayfind-qa-bash && chmod +x /tmp/wayfind-qa-bash
  ```

- **The live database is never touched.** Fingerprint it first, and check the
  fingerprint again at the end:

  ```sh
  shasum -a 256 ~/.config/wayfind/wayfind.sqlite | tee /tmp/wayfind-qa-live-before.txt
  ```

### Clean state

Every scenario runs inside one throwaway directory with its own configuration
home. Set it up once, and keep the shell open:

```sh
cd ~/Projects/Rust/wayfind
cargo build
export QA=$(mktemp -d /tmp/wayfind-qa.XXXXXX)
export XDG_CONFIG_HOME="$QA/xdg"
export HOME_REAL="$HOME"
export WAYFIND_SESSION_ID=qa-session-1
mkdir -p "$XDG_CONFIG_HOME"
export W="$PWD/target/debug/wayfind"
mkdir -p "$QA/project" && cd "$QA/project" && git init -q .
```

`$W` is the binary, `$QA` the sandbox, and `$QA/project` a git checkout that
becomes the project key. Every command below runs from `$QA/project`.

## How to Run

- **Solo:** Execute each step manually. Compare output to the "Expected output"
  block. Mark pass/fail.
- **With an agent:** Ask the agent to "walk me through the QA plan one step at a
  time." The agent should show each command, wait for you to run it (or run it
  on your behalf with your approval), display the output, compare it to the
  expected output, and only advance after you confirm.

Variable parts of expected output are marked `<…>`. Timestamps, the sandbox
path, and identifiers on a fresh database are the only ones.

---

## Scenarios

### Scenario 1: Initialize and register the project

**Purpose:** `wayfind init` creates the database and records the project key,
and the project key is the git checkout root.

**Steps:**

1. Run:
   ```sh
   $W init
   ```
   **Expected output** — on macOS the project key is the *physical* path, so
   `/tmp/…` appears as `/private/tmp/…`:
   ```
   initialized <QA>/xdg/wayfind/wayfind.sqlite for /private<QA>/project
   ```

2. Run:
   ```sh
   sqlite3 "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" ".tables"
   ```
   **Expected output** (order may vary across columns) — twelve script tables,
   the two additive port tables, and the FTS5 shadow tables:
   ```
   attachment_references     scope_exclusions          ticket_search_data
   attachments               sessions                  ticket_search_docsize
   decisions                 ticket_claims             ticket_search_idx
   fog_notes                 ticket_dependencies       tickets
   initiatives               ticket_search             wayfind_id_sequences
   projects                  ticket_search_config      wayfind_initiative_revisions
   ```

3. Run from a subdirectory, to prove the key is the checkout root:
   ```sh
   mkdir -p sub/deeper && cd sub/deeper && $W init && cd "$QA/project"
   ```
   **Expected output:** the same `for <QA>/project` as step 1 — not
   `for <QA>/project/sub/deeper`.

**Pass criteria:** The database exists, holds the twelve tables plus the FTS5
shadow tables, and the project key is the checkout root from any depth.
**Common failure modes:** the key follows the current directory; `init` writes
to `$HOME/.config` instead of `$XDG_CONFIG_HOME`.

---

### Scenario 2: Chart an initiative from empty to clear

**Purpose:** The golden path — create, ticket, block, next, claim, resolve,
clear.

**Steps:**

1. Run:
   ```sh
   $W initiative create --name "QA run" \
     --destination "Every command in the QA plan passes." \
     --notes "Created by the manual QA plan."
   ```
   **Expected output:**
   ```
   +++
   kind = "map"
   initiative_id = 1
   name = "QA run"
   status = "charting"
   +++

   # QA run

   ## Destination

   Every command in the QA plan passes.

   ## Notes

   Created by the manual QA plan.

   ## Frontier

   Initiative 1 has no tickets yet.
   ...
   ```

2. Run:
   ```sh
   $W ticket create --title "Read the reference" --type research \
     --question "What does the script do?"
   $W ticket create --title "Write the port" --type task \
     --question "Does the port do the same?"
   ```
   **Expected output** (second command):
   ```
   +++
   kind = "ticket"
   id = 2
   title = "Write the port"
   type = "task"
   status = "open"
   blocked_by = []
   attachments = 0
   referenced = 0
   +++
   ```

3. Record the dependency:
   ```sh
   $W ticket block 2 --by 1
   ```
   **Expected output:**
   ```
   blocked_by = [1]
   ```

4. Ask for the work:
   ```sh
   $W next
   ```
   **Expected output:** ticket **1**, not ticket 2 — ticket 2 waits.
   ```
   kind = "ticket"
   id = 1
   title = "Read the reference"
   ```

5. Claim and resolve it:
   ```sh
   $W ticket claim 1
   $W ticket resolve 1 --resolution "The script keeps one initiative at a time."
   ```
   **Expected output** (second command):
   ```
   status = "resolved"
   ```
   and a `## Resolution` section holding the text.

6. Ask again:
   ```sh
   $W next
   ```
   **Expected output:** ticket **2** — its blocker now carries a decision.

7. Settle it in a second session, and close the initiative:
   ```sh
   WAYFIND_SESSION_ID=qa-session-2 $W ticket claim 2
   WAYFIND_SESSION_ID=qa-session-2 $W ticket resolve 2 \
     --resolution "The port keeps the same semantics."
   $W initiative clear
   ```
   **Expected output** (last command):
   ```
   initiative 1 is clear
   ```

**Pass criteria:** `next` hands out 1 before 2 and 2 only after 1 is resolved;
the initiative clears once nothing is outstanding.
**Common failure modes:** `next` offers a blocked ticket; `clear` succeeds with
open work; a resolution lands without a claim.

---

### Scenario 3: The three documents

**Purpose:** `map`, `tree`, and `handoff` report the initiative in the shape a
caller parses.

**Steps:**

1. Run:
   ```sh
   $W --initiative 1 map
   ```
   **Expected output:** front matter with `kind = "map"` and
   `status = "clear"`, then these headings in this order:
   ```
   # QA run
   ## Destination
   ## Notes
   ## Frontier
   ## Decisions so far
   ## Not yet specified
   ## Out of scope
   ```

2. Run:
   ```sh
   $W --initiative 1 tree
   ```
   **Expected output:**
   ```
   # QA run

   ✓ [2] Write the port · resolved · task
   |  -> [1]
   ✓ [1] Read the reference · resolved · research
   ```

3. Run:
   ```sh
   $W --initiative 1 handoff
   ```
   **Expected output:** front matter counting the work, then every decision in
   full under its own heading:
   ```
   kind = "handoff"
   initiative_id = 1
   name = "QA run"
   status = "clear"
   decisions = 2
   unresolved = 0
   attachments = 0
   ```

4. Check the front matter parses:
   ```sh
   $W --initiative 1 map | sed -n '2,/^+++$/p' | sed '$d' | python3 -c "import sys,tomllib; print(tomllib.loads(sys.stdin.read()))"
   ```
   **Expected output:** a Python dict, no traceback:
   ```
   {'kind': 'map', 'initiative_id': 1, 'name': 'QA run', 'status': 'clear'}
   ```

**Pass criteria:** All three documents render, the headings keep their order,
and the front matter is valid TOML.
**Common failure modes:** a section out of order; front matter that fails to
parse because a value was not escaped.

---

### Scenario 4: Attachments

**Purpose:** Filing, referencing, reading, and deleting documents.

**Steps:**

1. Set up a second initiative with two tickets, and file a document. `attach
   add` binds the session to the initiative, and `qa-session-1` already belongs
   to initiative 1 — so this scenario uses a new session from here on:
   ```sh
   export WAYFIND_SESSION_ID=qa-session-3
   $W initiative create --name "Attachments" --destination "Documents work."
   $W ticket create --title "Owner" --type research --question "Who owns it?"
   $W ticket create --title "Reader" --type research --question "Who reads it?"
   printf 'line one\nline two\n' > "$QA/note.md"
   $W attach add 3 --file "$QA/note.md" --description "QA note"
   ```
   **Expected output:** ticket 3 with `attachments = 1`. Reusing `qa-session-1`
   instead gives `wayfind: this session already belongs to initiative 1 …`,
   which is the rule working, not a fault.

2. List them:
   ```sh
   $W attach list
   ```
   **Expected output:**
   ```
   +++
   kind = "attachments"
   initiative_id = 2
   count = 1
   +++

   # Attachments

   | ID | Ticket | Name | Size | Description |
   | --- | --- | --- | --- | --- |
   | 1 | 3 | note.md | 17 B | QA note |
   ```
   The size is 17 B, not 18 — the terminating line feed is not stored.

3. Reference and un-reference it:
   ```sh
   $W attach ref 4 --attachment 1
   $W ticket 4
   $W attach unref 4 --attachment 1
   ```
   **Expected output:** after `attach ref`, ticket 4 reports
   `referenced = 1`; after `attach unref`, `referenced = 0`.

4. Read it back exactly:
   ```sh
   $W attach show 1 --raw > "$QA/roundtrip.md"
   cmp "$QA/note.md" "$QA/roundtrip.md" && echo IDENTICAL
   ```
   **Expected output:**
   ```
   IDENTICAL
   ```

5. Delete it:
   ```sh
   $W attach rm 1
   $W attach list
   ```
   **Expected output:** `count = 0`, and the table is a header row on its own.

**Pass criteria:** `attach show --raw` reproduces the source file byte for byte;
references are counted apart from owned documents; deleting takes the
references with it.
**Common failure modes:** a trailing line feed lost or doubled on the round
trip; a reference left dangling after `attach rm`.

---

### Scenario 5: Search and export

**Purpose:** FTS5 search and CSV records over the initiative in play.

**Steps:**

1. Run:
   ```sh
   $W --initiative 1 search port
   ```
   **Expected output:**
   ```
   +++
   kind = "search"
   query = "port"
   limit = 10
   offset = 0
   +++

   # Search results

   - [2] Write the port (resolved) — Does the **port** do the same?
   ```

2. Run an FTS5 expression:
   ```sh
   $W --initiative 1 search "port OR script"
   ```
   **Expected output:** both tickets, the query echoed verbatim in the front
   matter.

3. Run a query FTS5 cannot parse:
   ```sh
   $W --initiative 1 search 'port AND'; echo "exit=$?"
   ```
   **Expected output:** a refusal naming the query, and a non-zero exit:
   ```
   wayfind: <fts5 syntax complaint>
   exit=1
   ```

4. Export:
   ```sh
   $W --initiative 1 dump --csv
   ```
   **Expected output:**
   ```
   id,title,type,status,question,resolution
   1,Read the reference,research,resolved,What does the script do?,The script keeps one initiative at a time.
   2,Write the port,task,resolved,Does the port do the same?,The port keeps the same semantics.
   ```

5. Check it parses as records:
   ```sh
   $W --initiative 1 dump --csv | python3 -c "import sys,csv; rows=list(csv.reader(sys.stdin)); print(len(rows), rows[0])"
   ```
   **Expected output:**
   ```
   3 ['id', 'title', 'type', 'status', 'question', 'resolution']
   ```

**Pass criteria:** search marks the matched term, scopes to the initiative, and
tells a syntax complaint from a failure; the CSV header names `question` and
the records parse.
**Common failure modes:** a search crossing into another initiative; a CSV
header leaking SQL; a syntax error reported as a crash.

---

### Scenario 6: Configuration layers

**Purpose:** Defaults, file, environment, and command line, each setting decided
on its own, later layer winning.

**Steps:**

1. Copy the database somewhere else and point the command line at it:
   ```sh
   cp "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" "$QA/elsewhere.sqlite"
   $W --sqlite.database "$QA/elsewhere.sqlite" --sqlite-fts5.database "$QA/elsewhere.sqlite" --initiative 1 map | head -5
   ```
   **Expected output:** the same map as Scenario 3.

2. Use the environment instead:
   ```sh
   WAYFIND_SQLITE__DATABASE="$QA/elsewhere.sqlite" \
   WAYFIND_SQLITE_FTS5__DATABASE="$QA/elsewhere.sqlite" \
   $W --initiative 1 map | head -5
   ```
   **Expected output:** the same map again.

3. Write a configuration file and let the command line beat it:
   ```sh
   cat > "$QA/config.toml" <<'TOML'
   [sqlite]
   database = "/nonexistent/decoy.sqlite"
   TOML
   $W --config "$QA/config.toml" --sqlite.database "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" --sqlite-fts5.database "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" --initiative 1 map | head -3
   ```
   **Expected output:** the map — the decoy path was overridden.

4. Refuse a misspelled key:
   ```sh
   printf '[sqlite]\ndatabse = "/tmp/x.sqlite"\n' > "$QA/bad.toml"
   $W --config "$QA/bad.toml" map; echo "exit=$?"
   ```
   **Expected output:** a refusal naming the unknown key, and `exit=1`.

5. Refuse a `--config` file that is not there:
   ```sh
   $W --config "$QA/absent.toml" map; echo "exit=$?"
   ```
   **Expected output:** a refusal naming the path, and `exit=1`.

**Pass criteria:** each of the four layers can name the database; a later layer
wins; unknown keys and missing named files are refused.
**Common failure modes:** an unknown key silently ignored; an empty environment
variable forcing an empty value.

---

### Scenario 7: Semantic parity with the Bash script

**Purpose:** The port reads a script-written database the same way, and the
script reads a port-written database the same way.

**Steps:**

1. Give the script its own copy of the QA database:
   ```sh
   mkdir -p "$QA/bash-xdg/wayfind"
   cp "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" "$QA/bash-xdg/wayfind/wayfind.sqlite"
   ```

2. Compare the read commands:
   ```sh
   for CMD in map tree handoff; do
     $W --initiative 1 $CMD > "$QA/rust-$CMD.txt"
     XDG_CONFIG_HOME="$QA/bash-xdg" /tmp/wayfind-qa-bash --project "$QA/project" --initiative 1 $CMD > "$QA/bash-$CMD.txt"
     diff -q "$QA/rust-$CMD.txt" "$QA/bash-$CMD.txt" >/dev/null && echo "IDENTICAL $CMD" || echo "DIFFERS $CMD"
   done
   ```
   **Expected output:**
   ```
   IDENTICAL map
   IDENTICAL tree
   IDENTICAL handoff
   ```

3. Compare the CSV as records, not as bytes — the header differs on purpose:
   ```sh
   $W --initiative 1 dump --csv > "$QA/rust-dump.csv"
   XDG_CONFIG_HOME="$QA/bash-xdg" /tmp/wayfind-qa-bash --project "$QA/project" --initiative 1 dump --csv > "$QA/bash-dump.csv"
   python3 - "$QA/rust-dump.csv" "$QA/bash-dump.csv" <<'PY'
   import csv, sys
   rows = lambda p: list(csv.reader(open(p, newline='')))
   r, b = rows(sys.argv[1]), rows(sys.argv[2])
   print("rust header:", r[0][4])
   print("bash header:", b[0][4])
   print("RECORDS_IDENTICAL" if r[1:] == b[1:] else "RECORDS_DIFFER")
   PY
   ```
   **Expected output:**
   ```
   rust header: question
   bash header: replace(t.question, char(10), ' ')
   RECORDS_IDENTICAL
   ```

4. Check the script reads what the port wrote:
   ```sh
   XDG_CONFIG_HOME="$QA/bash-xdg" /tmp/wayfind-qa-bash --project "$QA/project" --initiative 2 map | head -6
   ```
   **Expected output:** front matter naming initiative 2, "Attachments".

**Pass criteria:** `map`, `tree`, and `handoff` are byte-identical; the CSV
differs only in its header and matches record for record; the script reads the
port's rows.
**Common failure modes:** a heading reworded; a front-matter key renamed; a
column the script cannot read.

---

### Scenario 8: Storage integrity

**Purpose:** Everything the port wrote leaves the database valid.

**Steps:**

1. Run:
   ```sh
   sqlite3 "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" "PRAGMA foreign_key_check;"; echo "rows=$?"
   ```
   **Expected output:** no rows.

2. Run:
   ```sh
   sqlite3 "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" "PRAGMA integrity_check;"
   sqlite3 "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" "INSERT INTO ticket_search(ticket_search) VALUES('integrity-check');" && echo "fts5 ok"
   ```
   **Expected output:**
   ```
   ok
   fts5 ok
   ```

3. Check the journal mode was left alone:
   ```sh
   sqlite3 "$XDG_CONFIG_HOME/wayfind/wayfind.sqlite" "PRAGMA journal_mode;"
   ```
   **Expected output:**
   ```
   wal
   ```

**Pass criteria:** no foreign-key violation, no corruption, FTS5 consistent.
**Common failure modes:** an orphaned claim or attachment row; an FTS5 index
out of step with `tickets`.

---

### Scenario 9: The build gate

**Purpose:** The workspace passes every check the repository enforces.

**Steps:**

1. Run:
   ```sh
   cd ~/Projects/Rust/wayfind
   cargo fmt -- --check; echo "fmt=$?"
   cargo check --workspace --all-targets >/dev/null 2>&1; echo "check=$?"
   cargo clippy --workspace --all-targets -- -D warnings >/dev/null 2>&1; echo "clippy=$?"
   cargo test --workspace --all-targets >/dev/null 2>&1; echo "test=$?"
   ```
   **Expected output:**
   ```
   fmt=0
   check=0
   clippy=0
   test=0
   ```

2. Run the whole pipeline, including the file-length cap and the argument ban:
   ```sh
   cargo run -p xtask -- lint >/dev/null 2>&1; echo "lint=$?"
   grep -c '^=== ' target/xtask-lint.log
   ```
   **Expected output:**
   ```
   lint=0
   6
   ```

3. Run the command smoke script:
   ```sh
   bash tests/smoke.sh target/debug/wayfind | tail -2
   ```
   **Expected output:**
   ```
   smoke: 129 passed, 0 failed
   ```

**Pass criteria:** every command exits 0; six checks logged; the smoke script
passes every case.
**Common failure modes:** a Clippy warning; a source file over 1000 lines; an
`#[allow(clippy::too_many_arguments)]` slipped in.

---

## Edge Cases

Run these from `$QA/project` with the sandbox still set up. Each should refuse
in one sentence and exit non-zero. The `attach add` cases name their own
session, because that command binds the session to the initiative in play.

1. **A session may settle one non-research ticket, ever.**
   ```sh
   $W initiative create --name "Budget" --destination "One task per session."
   WAYFIND_SESSION_ID=budget-1 $W ticket create --title "First" --type task --question "One?"
   WAYFIND_SESSION_ID=budget-1 $W ticket create --title "Second" --type task --question "Two?"
   WAYFIND_SESSION_ID=budget-1 $W ticket claim 5
   WAYFIND_SESSION_ID=budget-1 $W ticket resolve 5 --resolution "Settled."
   WAYFIND_SESSION_ID=budget-1 $W ticket claim 6; echo "exit=$?"
   ```
   Expected: the last claim is refused, `exit=1`. A research ticket claimed by
   the same session would still be accepted.

2. **A session holds at most one ticket.**
   ```sh
   WAYFIND_SESSION_ID=hold-1 $W ticket claim 6
   WAYFIND_SESSION_ID=hold-1 $W ticket create --title "Third" --type research --question "Three?"
   WAYFIND_SESSION_ID=hold-1 $W ticket claim 7; echo "exit=$?"
   ```
   Expected: refused because the session already holds ticket 6, `exit=1`.

3. **Only the holder may resolve.**
   ```sh
   WAYFIND_SESSION_ID=other $W ticket resolve 6 --resolution "Not mine."; echo "exit=$?"
   ```
   Expected: refused, `exit=1`.

4. **A cycle is refused and named.**
   ```sh
   $W ticket block 7 --by 6
   $W ticket block 6 --by 7; echo "exit=$?"
   ```
   Expected: `wayfind: dependency would create a cycle: 6 -> 7 -> 6`, `exit=1`.

5. **A ticket cannot wait on itself.**
   ```sh
   $W ticket block 7 --by 7; echo "exit=$?"
   ```
   Expected: refused, `exit=1`.

6. **Clearing with open work is refused.**
   ```sh
   $W initiative clear; echo "exit=$?"
   ```
   Expected: `wayfind: N ticket(s) are still unresolved`, `exit=1`.

7. **An empty document is refused.**
   ```sh
   : > "$QA/empty.md"
   WAYFIND_SESSION_ID=att-1 $W attach add 7 --file "$QA/empty.md" --description "Nothing"; echo "exit=$?"
   ```
   Expected: `wayfind: content is empty`, `exit=1`.

8. **A document holding a NUL is refused.**
   ```sh
   printf 'before\0after' > "$QA/nul.bin"
   WAYFIND_SESSION_ID=att-1 $W attach add 7 --file "$QA/nul.bin" --description "Binary"; echo "exit=$?"
   ```
   Expected: a refusal naming NUL, `exit=1`.

9. **A document above the cap is refused.**
   ```sh
   python3 -c "open('$QA/big.md','w').write('x'*1048577)"
   WAYFIND_SESSION_ID=att-1 $W attach add 7 --file "$QA/big.md" --description "Too big"; echo "exit=$?"
   ```
   Expected: a refusal naming the 1 048 576 byte cap, `exit=1`.

10. **A piped document needs a name.**
    ```sh
    echo hello | WAYFIND_SESSION_ID=att-1 $W attach add 7 --file - --description "Piped"; echo "exit=$?"
    ```
    Expected: `wayfind: a document read from standard input has no name; pass
    --name`, `exit=1`.

11. **A command that acts as a session needs one.**
    Four commands do: `session resume`, `ticket claim`, `ticket resolve`, and
    `attach add`. `ticket create` is not one of them — it records no session, so
    it succeeds without one, exactly as the script does.
    ```sh
    env -u WAYFIND_SESSION_ID -u CLAUDE_SESSION_ID $W ticket claim 7; echo "exit=$?"
    env -u WAYFIND_SESSION_ID -u CLAUDE_SESSION_ID $W session resume; echo "exit=$?"
    env -u WAYFIND_SESSION_ID -u CLAUDE_SESSION_ID $W attach add 7 --file "$QA/note.md" --description x; echo "exit=$?"
    ```
    Expected, three times: `wayfind: a session is required; pass --session ID or
    set WAYFIND_SESSION_ID`, `exit=1`.

12. **Only `init` and `initiative create` may create a database.**
    ```sh
    $W --sqlite.database "$QA/absent.sqlite" --sqlite-fts5.database "$QA/absent.sqlite" map; echo "exit=$?"
    test -e "$QA/absent.sqlite" && echo "CREATED (wrong)" || echo "not created"
    ```
    Expected: refused, `exit=1`, and no file left behind.

13. **A search limit of zero, or above 500, is refused.**
    ```sh
    $W --initiative 1 search port --limit 0; echo "exit=$?"
    $W --initiative 1 search port --limit 501; echo "exit=$?"
    ```
    Expected: both refused, `exit=1`.

14. **`dump` needs `--csv`.**
    ```sh
    $W --initiative 1 dump; echo "exit=$?"
    ```
    Expected: Clap reports the missing required argument, `exit=2`.

15. **A file that is not there.**
    ```sh
    WAYFIND_SESSION_ID=att-1 $W attach add 7 --file "$QA/absent.txt" --description "Gone"; echo "exit=$?"
    ```
    Expected: a refusal naming the path, `exit=1`.

## Rollback / Cleanup

```sh
cd ~/Projects/Rust/wayfind
rm -rf "$QA"
rm -f /tmp/wayfind-qa-bash /tmp/wayfind-qa-live-before.txt
unset QA W XDG_CONFIG_HOME WAYFIND_SESSION_ID
```

Then prove the live database never moved:

```sh
shasum -a 256 ~/.config/wayfind/wayfind.sqlite
```

It must equal the fingerprint taken in the prerequisites. Nothing in this plan
writes to `~/.config/wayfind/wayfind.sqlite`; every command is pointed at
`$XDG_CONFIG_HOME` inside the sandbox, or at an explicit `--sqlite.database`.

## QA Run Results — 2026-08-02

Run in full on 2026-08-02 against `target/debug/wayfind`, in a sandbox at
`/tmp/wayfind-qa.OI905y` with its own `XDG_CONFIG_HOME`. The live database
fingerprint was `1b633f42…f567` before the run and the same after it: no command
touched `~/.config/wayfind/wayfind.sqlite`.

| Scenario | Result | Notes |
| -------- | ------ | ----- |
| 1: Initialize and register the project | PASS | Expected output corrected twice: the project key is the physical path (`/private/tmp/…`) on macOS, and the `.tables` list now names the real sixteen tables. |
| 2: Chart an initiative from empty to clear | PASS | `next` gave ticket 1 while 2 was blocked, then ticket 2 once 1 carried a decision. |
| 3: The three documents | PASS | Expected heading list corrected: `## Not yet specified` and `## Out of scope` also follow `## Decisions so far`. Front matter parsed with `tomllib`. |
| 4: Attachments | PASS after a plan fix | The step reused `qa-session-1`, already bound to initiative 1, so `attach add` refused. That refusal is the rule working — `/tmp/wayfind-qa-bash:709` binds the session the same way. The step now names a new session. |
| 5: Search and export | PASS | Marked snippet, echoed query, `wayfind: search query is not valid: fts5: syntax error near ""` with exit 1, and a three-row CSV headed `question`. |
| 6: Configuration layers | PASS | Command line, environment, and file each named the database; the command line beat the decoy; unknown key and missing `--config` file both refused with exit 1. |
| 7: Semantic parity with the Bash script | PASS | `map`, `tree`, and `handoff` byte-identical; CSV records identical, header differing on purpose; the script read the port-written initiative 2. |
| 8: Storage integrity | PASS | No foreign-key rows, `integrity_check` ok, FTS5 `integrity-check` ok, journal mode still `wal`. |
| 9: The build gate | PASS | `fmt`, `check`, `clippy -D warnings`, and `test` all exit 0; `xtask lint` exit 0 with six logged checks; smoke 129 passed, 0 failed. |
| Edge cases 1–10, 12–15 | PASS | Each refused in one sentence with exit 1, except case 14, where Clap exits 2. Cases 7–10 and 15 now name their own session, for the reason given in Scenario 4. |
| Edge case 11 | PASS after a plan fix | The case asked `ticket create` to refuse without a session. It should not: the script's `command_ticket_create` never calls `session_id()`, and records no session. Four commands do act as a session — `session resume`, `ticket claim`, `ticket resolve`, `attach add` — and all four refuse. The case now tests those three plus `attach add`, and `.claude/context/configuration.md` was corrected to say so. |

**Failures found in the port: none.** Both fixes were to this plan and to one
documentation sentence, not to the code. Nothing is deferred to the user; every
step is checkable from the command line, and every step was run.
