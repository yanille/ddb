# ddb

A fast, terminal-native, **strictly read-only** Amazon DynamoDB inspection CLI
for developers.

`ddb` lets you inspect DynamoDB data from the shell — list tables, describe
schemas, get items, run partition-key queries, and scan — with concise commands,
sensible defaults, and machine-readable JSON. It is **read-only by design**: the
tool cannot create, update, or delete DynamoDB data or configuration. That
guarantee is enforced structurally (all access flows through a read-only trait)
and checked by an automated test.

## Install / build

```bash
cargo build --release
# binary at target/release/ddb
```

## Authentication

`ddb` uses the standard AWS configuration chain — it does not store credentials
itself. Credentials and region resolve from (in the usual precedence):

- Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_PROFILE`, `AWS_REGION`, …)
- Shared config/credentials files (`~/.aws/config`, `~/.aws/credentials`)
- AWS SSO / IAM Identity Center
- IAM roles and EC2/ECS/container credential providers

Override the profile or region per invocation:

```bash
ddb tables --profile my-profile --region us-west-2
```

Credentials, secret keys, and session tokens are never printed.

## Commands

```bash
ddb tables                                   # list tables, or pick one interactively
ddb use [table]                              # activate a table for the session
ddb deactivate                               # clear the active table
ddb describe [table]                         # table schema, indexes, metadata
ddb indexes [table]                          # secondary indexes only
ddb get [table] --pk <value> [--sk <value>]  # single item by primary key
ddb query [table] --pk <value> [--sk <value> | --sk-begins-with <prefix>]
ddb scan [table] [--limit N] [--max-pages N]
ddb shell-init <zsh|bash>                     # print shell integration
```

Every data command supports `--output human` (default) or `--output json`. The
`[table]` argument is optional when an active table is set (see below).

### Active table (virtualenv-style)

Long table names get tedious. `ddb` lets you "activate" a table for your shell
session — like a Python virtualenv's `(env)` — so subsequent commands can omit
it. Install the shell integration once:

```bash
# ~/.zshrc  (or ~/.bashrc with: eval "$(ddb shell-init bash)")
eval "$(ddb shell-init zsh)"
```

Then:

```bash
ddb tables                 # fuzzy-pick a table in the terminal → it activates
# prompt now shows:  (ddb:EmployeeHistory) $
ddb query --pk 12345       # runs against the active table, no name needed
ddb use Orders             # switch tables directly
ddb use                    # switch via the picker
ddb deactivate             # clear the active table
```

The active table lives in the `DDB_TABLE` environment variable, scoped to that
shell. An explicit `[table]` argument always overrides it, so scripts are never
affected by session state. The interactive picker only appears on a real
terminal — piped or `--output json`, `ddb tables` prints the plain list exactly
as before (`ddb tables | grep …`, `ddb tables --output json | jq .`). Force the
plain list on a TTY with `ddb tables --plain`.

### Keys

Key values may be given bare or as `name=value`; the scalar type (`S`/`N`/`B`)
is taken from the table schema via `DescribeTable`, so numbers, strings, and
binary keys are handled correctly:

```bash
ddb get Users --pk 12345
ddb get EmployeeHistory --pk employee_id=12345 --sk timestamp=2026-09-17
```

Binary (`B`) key values are supplied as base64.

### Query

```bash
ddb query EmployeeHistory --pk 12345
ddb query EmployeeHistory --pk 12345 --sk 2026-09-17
ddb query EmployeeHistory --pk 12345 --sk-begins-with 2026-
ddb query Users --index email-index --pk jane@example.com
```

`--sk-begins-with` is valid only for string/binary sort keys.

### Scan safeguards

Scans can be expensive, so defaults are conservative: `ddb scan <table>` reads a
single page of up to `--limit 100` items. Read more deliberately with `--limit`
and `--max-pages`. When results are truncated, a note is printed to stderr.

## Output

- **Human** (default): readable, YAML-like key/value blocks.
- **JSON**: valid, deterministic (sorted keys), `jq`-friendly.

Data goes to **stdout**; summaries, warnings, and errors go to **stderr**. In
JSON mode stdout carries only the JSON payload, so pipelines are safe:

```bash
ddb get Users --pk 12345 --output json | jq .
ddb query Orders --pk 12345 --output json | jq '.[].total'
```

### Attribute conversion (JSON)

| DynamoDB | JSON |
|----------|------|
| `S` | string |
| `N` | number when losslessly representable (≤15 significant digits), else string |
| `BOOL` | boolean |
| `NULL` | null |
| `M` | object |
| `L` | array |
| `SS` | array of strings |
| `NS` | array of numbers/strings |
| `B` | base64 string |
| `BS` | array of base64 strings |

Binary is always base64-encoded — never emitted as raw/invalid UTF-8.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | success |
| 1 | generic service/transport error |
| 2 | invalid usage / bad arguments |
| 3 | table not found |
| 4 | access denied |
| 5 | credentials could not be resolved |
| 6 | index not found |

In `--output json`, failures print a `{"error": {"code", "message"}}` object to
stderr (stdout stays empty).

## Development

```bash
cargo test          # unit + integration tests
cargo clippy --all-targets
```

The `tests/no_write_path.rs` test scans `src/` and fails if any mutating
DynamoDB operation is ever introduced — the machine-checked read-only guarantee.

Project documentation lives under [`docs/`](docs/): architecture, decisions,
progress, backlog, and current task.

## Safety scope

`ddb` is intentionally narrow: a read-only DynamoDB inspection tool, not a
general-purpose AWS CLI. Any feature that could mutate DynamoDB state is out of
scope by design.

## License

Licensed under the [MIT License](LICENSE).
