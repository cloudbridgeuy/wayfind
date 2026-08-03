#!/usr/bin/env bash
#
# End-to-end smoke test for the `wayfind` binary.
#
# Usage: bash tests/smoke.sh [path-to-binary]
#
# Every command runs against a database in a temporary directory. The live
# database at ~/.config/wayfind/wayfind.sqlite is never read and never written:
# XDG_CONFIG_HOME is redirected before the first command, HOME is redirected as
# well, and the temporary tree is removed on exit.
#
# What this covers is the shell, not the rules. The rules have unit tests in
# `wayfind_core`. What can only be checked by running the binary is checked
# here: that each command dispatches, that a refusal is a non-zero exit with a
# message on standard error, that a document survives the round trip through
# SQLite unchanged, and that the whole lifecycle works end to end.

set -o errexit
set -o nounset
set -o pipefail

readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly BINARY="$(cd "$(dirname "${1:-$ROOT/target/debug/wayfind}")" && pwd)/$(basename "${1:-wayfind}")"
readonly FIXTURES="$ROOT/tests/fixtures"

if [[ ! -x "$BINARY" ]]; then
  echo "smoke: no binary at $BINARY; run \`cargo build\` first" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# An isolated machine
# ---------------------------------------------------------------------------

WORKSPACE="$(mktemp -d "${TMPDIR:-/tmp}/wayfind-smoke.XXXXXX")"
readonly WORKSPACE
trap 'rm -rf "$WORKSPACE"' EXIT

export XDG_CONFIG_HOME="$WORKSPACE/config"
export HOME="$WORKSPACE/home"
export WAYFIND_SESSION_ID="agent-primary"
unset CLAUDE_SESSION_ID || true
mkdir -p "$HOME" "$WORKSPACE/project"
cd "$WORKSPACE/project"

readonly DATABASE="$XDG_CONFIG_HOME/wayfind/wayfind.sqlite"

# ---------------------------------------------------------------------------
# Reporting
# ---------------------------------------------------------------------------

PASSED=0
FAILED=0
CURRENT="startup"

scenario() {
  CURRENT="$1"
  printf '\n== %s\n' "$CURRENT"
}

pass() {
  PASSED=$((PASSED + 1))
  printf '  ok    %s\n' "$1"
}

fail() {
  FAILED=$((FAILED + 1))
  printf '  FAIL  %s\n' "$1"
  if [[ $# -gt 1 ]]; then
    printf '        %s\n' "$2"
  fi
}

# Run a command that must succeed, and keep its output in $OUTPUT.
run() {
  if OUTPUT="$("$BINARY" "$@" 2>"$WORKSPACE/stderr")"; then
    pass "wayfind $*"
    return 0
  fi
  fail "wayfind $*" "$(cat "$WORKSPACE/stderr")"
  OUTPUT=""
  return 0
}

# Run a command that must be refused. The refusal must be non-zero and must
# say something on standard error.
refused() {
  local description="$1"
  shift
  if OUTPUT="$("$BINARY" "$@" 2>"$WORKSPACE/stderr")"; then
    fail "$description" "the command succeeded and should not have"
    return 0
  fi
  if [[ ! -s "$WORKSPACE/stderr" ]]; then
    fail "$description" "the command failed silently"
    return 0
  fi
  pass "$description — $(head -n 1 "$WORKSPACE/stderr")"
}

# Assert that the last output holds a piece of text.
holds() {
  if [[ "$OUTPUT" == *"$1"* ]]; then
    pass "output holds \"$1\""
    return 0
  fi
  fail "output holds \"$1\"" "output was: $(printf '%s' "$OUTPUT" | head -n 12 | tr '\n' '~')"
}

# Assert that the last output does not hold a piece of text.
lacks() {
  if [[ "$OUTPUT" != *"$1"* ]]; then
    pass "output lacks \"$1\""
    return 0
  fi
  fail "output lacks \"$1\"" "output was: $(printf '%s' "$OUTPUT" | head -n 12 | tr '\n' '~')"
}

# Assert that two files are identical, byte for byte.
same_bytes() {
  if cmp --silent "$1" "$2"; then
    pass "$3"
    return 0
  fi
  fail "$3" "$(cmp "$1" "$2" 2>&1 || true)"
}

# ---------------------------------------------------------------------------
# The golden path
# ---------------------------------------------------------------------------

scenario "the live database is never touched"
if [[ "$DATABASE" == "$HOME"* ]] || [[ "$DATABASE" == "$WORKSPACE"* ]]; then
  pass "the database lives at $DATABASE"
else
  fail "the database lives inside the workspace" "it is at $DATABASE"
fi

scenario "init creates a database and a project"
run init
holds "initialized"
[[ -f "$DATABASE" ]] && pass "the database file exists" || fail "the database file exists"

scenario "a command before any initiative is refused, not guessed at"
refused "map without an initiative" map

scenario "initiative create charts a map"
run initiative create --name "Smoke run" --destination "Every command dispatches" --notes "Written by tests/smoke.sh"
holds 'kind = "map"'
holds "## Destination"
holds "## Notes"

scenario "ticket create adds questions"
run ticket create --title "Chart the map" --type research --question "What is already known?"
holds 'kind = "ticket"'
holds 'status = "open"'
run ticket create --title "Port the shell" --type task --question "Which commands remain?"
run ticket create --title "Grill the plan" --type grilling --question "What did we assume?"

scenario "map lists the frontier"
run map
holds "- [1] Chart the map (research)"
holds "- [2] Port the shell (task)"
holds "- [3] Grill the plan (grilling)"

scenario "next offers a ticket"
run next
holds 'kind = "ticket"'

scenario "claim takes a ticket and resume shows it back"
run ticket claim 1
holds 'status = "claimed"'
run session resume
# A session holding a ticket gets that ticket back, not the guidance document.
holds 'kind = "ticket"'
holds "id = 1"
run sessions list
holds "agent-primary"
holds "working"

# ---------------------------------------------------------------------------
# Ownership
# ---------------------------------------------------------------------------

scenario "another session cannot take or settle a held ticket"
refused "a second session claims a held ticket" --session agent-second ticket claim 1
refused "a second session resolves a held ticket" --session agent-second ticket resolve 1 --resolution "Not mine to settle."

scenario "a ticket must be held before it is settled"
refused "resolve without a claim" ticket resolve 2 --resolution "Never claimed."

# ---------------------------------------------------------------------------
# Dependencies and cycles
# ---------------------------------------------------------------------------

scenario "block makes one ticket wait on another"
run ticket block 2 --by 3
holds "blocked_by = [3]"

scenario "a dependency that would close a loop is refused"
refused "a cycle" ticket block 3 --by 2
refused "a ticket blocking itself" ticket block 3 --by 3

scenario "the tree draws the graph"
run tree
printf '%s\n' "$OUTPUT" >"$WORKSPACE/tree.md"
same_bytes "$WORKSPACE/tree.md" "$FIXTURES/expected-tree.md" "the tree matches the fixture"

# ---------------------------------------------------------------------------
# Attachments
# ---------------------------------------------------------------------------

scenario "attach add stores a document"
printf 'benchmark rows\nsecond line\n' >"$WORKSPACE/bench.txt"
run attach add 1 --file "$WORKSPACE/bench.txt" --description "Timings"
holds "attachments = 1"

scenario "attach show --raw gives the file back byte for byte"
"$BINARY" attach show 1 --raw >"$WORKSPACE/roundtrip.txt"
same_bytes "$WORKSPACE/roundtrip.txt" "$WORKSPACE/bench.txt" "the stored document survives the round trip"

scenario "attach show without --raw adds a header"
run attach show 1
holds 'kind = "attachment"'
holds "# bench.txt"
holds "benchmark rows"

scenario "attach list reports one row per document"
run attach list
holds 'count = 1'
holds "| 1 | 1 | bench.txt |"

scenario "attach add --move deletes the source only after a successful store"
printf 'moved document\n' >"$WORKSPACE/moved.txt"
run attach add 1 --file "$WORKSPACE/moved.txt" --description "Moved" --move
[[ -f "$WORKSPACE/moved.txt" ]] && fail "the source was deleted" || pass "the source was deleted"

scenario "a document read from standard input needs a name"
refused "stdin without --name" attach add 1 --file - --description "Piped"
printf 'piped document\n' | "$BINARY" attach add 1 --file - --description "Piped" --name piped.txt >/dev/null
pass "stdin with --name is accepted"

scenario "ref and unref point a ticket at another ticket's document"
run attach ref 2 --attachment 1
holds "referenced = 1"
run attach unref 2 --attachment 1
holds "referenced = 0"
refused "a ticket referencing its own document" attach ref 1 --attachment 1

scenario "attach rm deletes a document"
run attach rm 3
holds "attachments = 2"
refused "showing a deleted document" attach show 3

# ---------------------------------------------------------------------------
# Resolution and amendment
# ---------------------------------------------------------------------------

scenario "resolve settles a held ticket"
run ticket resolve 1 --resolution "Everything known is written down."
holds 'status = "resolved"'
holds "## Resolution"

scenario "amend repairs a recorded decision without a claim"
run ticket amend 1 --resolution "Everything known is written down, twice."
holds "amended_at"
holds "written down, twice"

scenario "amend refuses a ticket that has no decision"
refused "amending an open ticket" ticket amend 2 --resolution "Nothing to repair."

scenario "a resolution can be read from a file or from standard input"
printf 'Settled from a file.\n' >"$WORKSPACE/resolution.txt"
run ticket claim 3
run ticket resolve 3 --resolution-file "$WORKSPACE/resolution.txt"
holds "Settled from a file."

# A fresh session, because a session may settle only one non-research ticket
# and `agent-primary` has spent that on ticket 3.
scenario "a session that has spent its budget is refused"
refused "a second non-research resolution" ticket claim 2

run --session agent-third ticket claim 2
printf 'Settled from standard input.\n' | "$BINARY" --session agent-third ticket resolve 2 --resolution-file - >/dev/null
pass "a resolution arrives on standard input"

# ---------------------------------------------------------------------------
# Search, dump, fog, scope
# ---------------------------------------------------------------------------

scenario "search finds a ticket by its text"
run search benchmark
holds 'kind = "search"'
run search "written down"
holds "[1]"

scenario "a malformed search is refused rather than crashing"
refused "an unbalanced quotation mark" search '"unbalanced'

scenario "a search with no hits is a document, not a failure"
run search zzzznothingmatchesthis
holds "# Search results"

scenario "dump writes records"
run dump --csv
holds "id,title,type,status,question,resolution"
holds "1,Chart the map,research,resolved,"

scenario "fog and scope collect what is not decided"
run fog add --note "Windows paths are untested"
holds "- Windows paths are untested"
run scope exclude --note "A web interface"
holds "- A web interface"

# ---------------------------------------------------------------------------
# An empty frontier, and a cleared initiative
# ---------------------------------------------------------------------------

scenario "an empty frontier is an answer, not a failure"
run next
holds 'kind = "next"'
holds "# No available ticket"

scenario "handoff collects the decisions"
run handoff
holds 'kind = "handoff"'
holds "## Decisions"
holds "### [1] Chart the map"
holds "## Attachments"

scenario "initiative clear closes the map"
run initiative clear
holds "is clear"
# Clearing is a write, so it picks the newest non-clear initiative. Once there
# is none, a second clear is refused and says which one to name — the same as
# every other write against a finished map.
refused "clearing twice without naming the map" initiative clear
run --initiative 1 initiative clear
holds "is clear" # naming it deliberately is not a mistake

scenario "a cleared initiative still reads"
run map
holds 'status = "clear"'
run handoff
holds 'status = "clear"'
run tree
run dump --csv
run attach list

scenario "a cleared initiative refuses writes until a new one is charted"
refused "creating a ticket after clear" ticket create --title "Too late" --type task --question "?"
refused "adding fog after clear" fog add --note "Too late"

scenario "an explicit --initiative reaches a cleared map deliberately"
run --initiative 1 ticket create --title "Deliberate" --type task --question "Reached on purpose?"
holds 'kind = "ticket"'

scenario "a second initiative starts a fresh map"
run initiative create --name "Second run" --destination "A new map"
holds 'initiative_id = 2'
run map
holds 'initiative_id = 2'
lacks "- [1] Chart the map" # the first map's tickets are not on this one

# ---------------------------------------------------------------------------
# Refusals that protect the operator
# ---------------------------------------------------------------------------

scenario "an identifier outside the current map is refused"
refused "a ticket from another initiative" ticket 1
refused "a ticket that does not exist" ticket 999

scenario "a command with no session is refused"
if OUTPUT="$(env -u WAYFIND_SESSION_ID -u CLAUDE_SESSION_ID "$BINARY" session resume 2>"$WORKSPACE/stderr")"; then
  fail "session resume without a session" "the command succeeded"
else
  pass "session resume without a session — $(head -n 1 "$WORKSPACE/stderr")"
fi

scenario "an unknown backend is refused before any database is opened"
refused "an unknown storage backend" --backend nowhere map
refused "an unknown search backend" --search-backend nowhere search anything

scenario "a missing database says so instead of creating one"
refused "a database that is not there" --sqlite.database "$WORKSPACE/absent.sqlite" map
[[ -f "$WORKSPACE/absent.sqlite" ]] && fail "no database was created" || pass "no database was created"

scenario "a session belongs to one initiative for life"
# `agent-primary` was bound to initiative 1. Initiative 2 is now the current
# one, and the session may not follow the work across.
refused "moving a bound session to another map" session resume
run --initiative 1 session resume # its own map still answers it
holds 'initiative_id = 1'

scenario "an empty document is refused"
: >"$WORKSPACE/empty.txt"
run --session agent-fresh ticket create --title "Somewhere to file" --type task --question "?"
refused "an empty attachment" --session agent-fresh attach add 5 --file "$WORKSPACE/empty.txt" --description "Empty"
printf '\0binary\n' >"$WORKSPACE/binary.bin"
refused "a document holding a NUL byte" --session agent-fresh attach add 5 --file "$WORKSPACE/binary.bin" --description "Binary"
refused "a file that is not there" --session agent-fresh attach add 5 --file "$WORKSPACE/absent.txt" --description "Missing"

# ---------------------------------------------------------------------------
# The verdict
# ---------------------------------------------------------------------------

printf '\n%s\n' "-----------------------------------------------------------"
printf 'smoke: %d passed, %d failed\n' "$PASSED" "$FAILED"

if [[ "$FAILED" -gt 0 ]]; then
  exit 1
fi
