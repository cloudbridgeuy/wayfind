# Behavior: v2 graph reads

Requirements for the six `wayfind2` commands that read the graph without
changing it: `initiative list`, `initiative show`, `graph show`,
`snapshot list`, `snapshot show`, and `node show`. Every one of these needs an
existing store (see [v2 store lifecycle](v2-store.md)); `initiative create` is
covered separately in [v2 initiative creation](v2-initiative.md).

## Language

- **Record prefix**: a kind letter (`R` for a node, `T` for a transition, `C`
  for a connection, `A` for an artifact), a dash, and 4 to 64 lowercase hex
  characters — the short form an operator types to address a record, such as
  `R-a3f9`. Fewer than 4 hex characters, uppercase hex, an unknown kind
  letter, or a missing dash are all the same failure: the text is not a
  well-formed prefix.

## Behavior

### Requirement: `initiative list` shows every initiative of the project
`wayfind2 initiative list` prints an `initiative-list` document whose front
matter carries `project` and `count`, and whose body holds one section per
initiative (its name as the heading, then `initiative`, `destination`, and
`created`).

#### Scenario: The project holds initiatives
- **WHEN** `wayfind2 initiative list` runs
- **THEN** the command exits 0 and the document's `count` equals the number of
  initiatives in the current project; a project with none prints `count = 0`
  and no sections

### Requirement: `initiative show` reads one initiative by its numeric id
`wayfind2 initiative show ID` prints the same `initiative` document
`initiative create` prints for that initiative, computed from its current head
snapshot and destination node.

#### Scenario: The id names an initiative
- **WHEN** `wayfind2 initiative show ID` runs and an initiative holds `ID`
- **THEN** the command exits 0 and prints its `initiative` document

#### Scenario: The id names no initiative
- **WHEN** `wayfind2 initiative show ID` runs and no initiative holds `ID`
- **THEN** the command exits 3 with an error document whose token is
  `not-found`

### Requirement: Every command scoped to one initiative's graph needs `--initiative ID`
`graph show`, `snapshot list`, and `snapshot show` read one initiative's
snapshots. Unlike v1, v2 has no active-initiative fallback: the global
`--initiative ID` flag must name one explicitly.

#### Scenario: No initiative is named
- **WHEN** `graph show`, `snapshot list`, or `snapshot show` runs without
  `--initiative ID`
- **THEN** the command exits 2 with an error document whose token is `usage`
  and whose body says an initiative is required

#### Scenario: The named initiative holds nothing
- **WHEN** `--initiative ID` names an id no initiative holds
- **THEN** `snapshot list` exits 0 and prints `count = 0` and `head = ""`
  (an unknown initiative reads the same as one with no snapshots); `graph
  show` and `snapshot show` exit 3 with a `not-found` error, since neither
  finds an ordinal to read

### Requirement: `snapshot list` and `snapshot show` read an initiative's snapshot history
`wayfind2 --initiative ID snapshot list` prints a `snapshot-list` document
(front matter: `initiative`, `count`, `head`; body: one `## SN` section per
snapshot with its `transition`, `base`, and `chain_hash`). `wayfind2
--initiative ID snapshot show <head|SN|N>` prints a `snapshot` document for
one snapshot (front matter: `initiative`, `snapshot`, `chain_hash`, and the
`nodes`, `transitions`, `connections`, and `artifacts` id lists that make up
its full membership).

#### Scenario: The selector names an existing snapshot
- **WHEN** `snapshot show` runs with `head`, `SN`, or bare `N`, and that
  ordinal exists
- **THEN** the command exits 0 and prints the `snapshot` document; `head`
  resolves to the highest ordinal the initiative has

#### Scenario: The selector names no snapshot
- **WHEN** `snapshot show` runs with an ordinal the initiative does not have
- **THEN** the command exits 3 with a `not-found` error

### Requirement: `graph show` renders the graph at one snapshot
`wayfind2 --initiative ID graph show [--snapshot <head|SN|N>]` derives that
snapshot's full membership and prints a `graph` document (front matter:
`initiative`, `snapshot`, and the `nodes`, `transitions`, and `connections`
counts; body: one `## Nodes` / `## Transitions` / `## Connections` section
naming each member by its abbreviated id plus its title or summary).
`--snapshot` defaults to `head`.

#### Scenario: The snapshot exists
- **WHEN** `graph show` runs against an existing snapshot
- **THEN** the command exits 0 and the document's counts match that
  snapshot's membership

#### Scenario: The snapshot does not exist
- **WHEN** `--snapshot` names an ordinal the initiative does not have
- **THEN** the command exits 3 with a `not-found` error

### Requirement: `node show` addresses one node by a full id or an unambiguous prefix
`wayfind2 node show R-<hex>` parses `<hex>` as a record prefix, resolves it
against every node whose hash starts with those hex characters, and answers
one of four ways depending on what matched. `node show` needs no
`--initiative`: a node's hash is enough to find it regardless of which
initiative it belongs to.

| Outcome | Exit | Token | Extra keys |
| --- | :---: | --- | --- |
| Exactly one node matches | 0 | — | the `node` document |
| More than one node matches | 3 | `ambiguous-id` | `prefix`, `candidates` |
| No node matches | 3 | `not-found` | `id` |
| The text is not a well-formed prefix | 4 | `unknown-word` | `id` |

The `node` document's front matter carries `node` (the full id), `node_kind`,
`title`, `summary` (when the draft has one), `created`, and `created_by`; the
body repeats the title, the summary, and the full content.

#### Scenario: The prefix matches exactly one node
- **WHEN** `node show R-<hex>` runs and exactly one node's hash starts with
  `<hex>`
- **THEN** the command exits 0 and prints that node's `node` document

#### Scenario: The prefix matches more than one node
- **WHEN** two or more nodes' hashes start with `<hex>`
- **THEN** the command exits 3 with an `ambiguous-id` error whose `prefix` key
  echoes the input and whose `candidates` key lists every match, sorted by hex

#### Scenario: The prefix matches no node
- **WHEN** no node's hash starts with `<hex>`
- **THEN** the command exits 3 with a `not-found` error whose `id` key echoes
  the input

#### Scenario: The text is not a well-formed prefix
- **WHEN** `node show` is given text with the wrong kind letter, uppercase
  hex, fewer than 4 or more than 64 hex characters, or no dash
- **THEN** the command exits 4 with an `unknown-word` error whose `id` key
  echoes the input, before any lookup runs
