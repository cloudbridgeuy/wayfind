# Behavior: v2 initiative creation

Requirements for `wayfind2 initiative create`, the first `wayfind2` command
that writes to the graph. Every other member of the `initiative` group is
still the stub in [v2 store lifecycle](v2-store.md).

## Behavior

### Requirement: Charting a new initiative writes its destination as the graph's first record
`wayfind2 initiative create --name NAME --destination TEXT [--notes TEXT]`
validates and hashes the write, then commits one `IMMEDIATE` transaction that
inserts the initiative, its destination as a node record, and the root
snapshot (`S1`) whose only member is that node. The destination node's
encoding fixes `title` to the initiative name, `summary` to the destination
text, and `content` to the destination text, plus one blank line and the
notes when notes are non-empty — so a migrated initiative and a v2-born one
hash alike.

#### Scenario: The name is new to the project
- **WHEN** `wayfind2 initiative create --name NAME --destination TEXT` runs
  and no initiative in the project already holds `NAME`
- **THEN** the command exits 0 and prints an `initiative` document whose front
  matter carries `initiative` (the new numeric id), `name`, `destination`,
  `head = "S1"`, `created`, and `destination_node` (the full 66-character
  record id); the body repeats the name and destination and shows the
  destination node's 10-character abbreviation

#### Scenario: The name is already taken in the project
- **WHEN** `wayfind2 initiative create --name NAME ...` runs and another
  initiative in the same project already holds `NAME`
- **THEN** the command writes nothing, exits 4, and prints an `error`
  document whose token is `name-taken` and whose `initiative` key names the
  existing initiative's id

#### Scenario: Two initiatives share destination text
- **WHEN** two initiatives in the same project are created with the same
  `--destination` text but different names (and so different `created_at`
  timestamps)
- **THEN** their destination nodes hash differently, because `created_at` and
  the creating session are hash inputs alongside the text
