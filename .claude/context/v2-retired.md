# Behavior: the retired v1 surface

Requirements for the nineteen v1 command spellings v2 renamed, moved, or
folded elsewhere. Every other v1 spelling either survives unchanged
(`init`, `search`, `dump`, bare `ticket ID`) or collides with a real v2
command of the same name — the v2 command wins and the v1 shape is not
retired.

## Behavior

### Requirement: A retired v1 spelling names its exact v2 replacement
Running any of `initiative clear`, `map`, `tree`, `next`, `handoff`,
`ticket claim`, `ticket resolve`, `ticket amend`, `ticket block`, `session
resume`, `session list`, `fog add`, `scope exclude`, `attach add`, `attach
ref`, `attach unref`, `attach list`, `attach show`, or `attach rm` against
`wayfind2` refuses instead of carrying out any v1 behavior, naming the v2
command to run instead. The retired spellings do not appear in `wayfind2
--help`; they remain parseable so the refusal — not Clap's own
"unrecognized subcommand" message — is what an operator sees.

#### Scenario: A retired spelling is run with no trailing words
- **WHEN** `wayfind2 map` runs against an existing store
- **THEN** the command writes nothing to stdout, exits 2, and prints an
  `error` document on stderr whose token is `retired-command` and whose
  `replacement` key is `run map`

#### Scenario: A retired spelling is run with trailing words
- **WHEN** `wayfind2 ticket claim 4` runs against an existing store
- **THEN** the command exits 2 and its error document's `replacement` key
  is `run question claim 4` — the trailing `4` is carried onto the v2
  replacement rather than dropped

#### Scenario: A v1 spelling collides with a real v2 command
- **WHEN** `wayfind2 ticket create ...` runs
- **THEN** the command is not retired; it is v2's own `ticket create`, and
  the store-lifecycle stub or its future implementation answers, not the
  `retired-command` refusal
