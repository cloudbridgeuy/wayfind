# Behavior: v2 store lifecycle

Requirements for the `wayfind2` binary: creating its store, and what every
command answers before the rewrite reaches it. `wayfind2` shares the v1
configuration home but keeps its own configuration file (`config.v2.toml`)
and its own database (`wayfind2.sqlite`); v1's `wayfind` is unaffected.

## Behavior

### Requirement: Creating the v2 store
`wayfind2 init` creates the `wayfind2.sqlite` store at the configured path and
reports the path in one plain-text line, with no front matter. Running it
again does not fail.

#### Scenario: No store exists
- **WHEN** `wayfind2 init` runs and no store is at the configured path
- **THEN** the store is created, its path is printed as "created a store at
  \<path\>", and the command exits 0

#### Scenario: A store already exists
- **WHEN** `wayfind2 init` runs and a store is already at the configured path
- **THEN** nothing is changed, its path is printed as "a store already exists
  at \<path\>", and the command exits 0

### Requirement: Every other v2 command needs an existing store
Every command but `init` opens the store rather than creating it.

#### Scenario: The store is missing
- **WHEN** any v2 command other than `init` runs and no store is at the
  configured path
- **THEN** the command refuses, exits 3, and its error document names
  `wayfind2 init` as the fix

### Requirement: v2's command surface is declared but not yet carried out
The full v2 command tree parses, but only `init`, `initiative create` (see
[v2 initiative creation](v2-initiative.md)), and the six read commands (see
[v2 graph reads](v2-graph-read.md): `initiative list`, `initiative show`,
`graph show`, `snapshot list`, `snapshot show`, `node show`) are implemented;
every other command not covered by [the retired v1 surface](v2-retired.md) is
a stub.

#### Scenario: A command this slice does not implement
- **WHEN** a v2 command other than `init`, `initiative create`, one of the six
  read commands, or a retired v1 spelling runs against an existing store
- **THEN** the command refuses with exit 2 and an error document whose body
  says the command is not implemented in this slice
