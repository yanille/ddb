# Progress

Status of the initial version. Only items with implementation + passing tests
are marked complete.

## Complete

- **Project scaffolding**: Cargo library + `ddb` binary; dependencies
  (`aws-config`, `aws-sdk-dynamodb`, `clap`, `tokio`, `serde`/`serde_json`,
  `anyhow`, `thiserror`, `base64`). Builds clean; `cargo clippy --all-targets`
  is warning-free.
- **Read-only trait boundary**: `DynamoDbReader` with `list_tables`,
  `describe_table`, `get_item`, `query`, `scan`. Handlers are generic over it.
- **AWS-backed client**: `AwsDynamoDbClient` implements the trait using read APIs
  only. Config loading is factored into `client::load_config`, which uses the
  standard AWS chain with `--profile`/`--region` overrides. **Shared files are
  honored**: credentials from `~/.aws/credentials` and region/settings from
  `~/.aws/config` (verified live — `ddb tables` with all `AWS_*` env cleared read
  region `us-west-2` from config and creds from the credentials file — and by an
  automated regression test, `tests/aws_config.rs`). Also verified against the
  no-credentials path (exit 5, clean stdout, JSON error on stderr).
- **Commands**:
  - `tables` — lists tables, follows pagination.
  - `describe` — schema, indexes, status, item count, size, billing mode.
  - `indexes` — secondary indexes only.
  - `get` — single item by primary key; schema-typed keys; composite-key
    validation; hit/miss handling (human note vs JSON `null`).
  - `query` — partition-key query; optional `--sk` (equals) and
    `--sk-begins-with`; `--index`; pagination via `--limit`/`--max-pages`.
  - `scan` — with conservative `--limit`/`--max-pages` defaults and truncation
    warnings; `--index` validated via describe.
- **Attribute conversion**: all `AttributeValue` variants → JSON, precision-safe
  numbers, base64 binary, sorted keys. Total (never panics).
- **Key parsing**: bare and `name=value`; `S`/`N`/`B` typing; base64 binary;
  `=`-in-value disambiguation.
- **Output**: `human` (YAML-like) and `json` (deterministic, jq-friendly);
  stdout/stderr split enforced in `main`.
- **Errors & exit codes**: categorized `DdbError` with stable exit codes (0–6)
  and machine codes; AWS errors mapped; no secret leakage.
- **Tests** (70 passing): attribute conversion (incl. composite/nested, precision
  fallback, base64, sets), key parsing, CLI parsing (defaults, conflicts, global
  flags), error codes, human/JSON renderers, `no_write_path` guarantee, shared
  AWS config/credentials file resolution (`tests/aws_config.rs`), and end-to-end
  command handlers via an in-memory fake reader (tables, get hit/miss, query,
  empty results, missing table, bad key, describe).
- **Documentation**: README + `docs/` (architecture, decisions, progress,
  backlog, current-task).

## Definition-of-Done checklist (from the spec)

1. Builds with Cargo — ✅
2. Authenticates via standard AWS config — ✅
3. `ddb tables` — ✅
4. `ddb get <table> --pk <value>` — ✅
5. `ddb query <table> --pk <value>` — ✅
6. Valid JSON output — ✅
7. Correct DynamoDB data-type handling — ✅
8. Pagination — ✅
9. Useful errors and exit codes — ✅
10. Automated tests — ✅
11. No DynamoDB mutation operations — ✅ (enforced by test)
12. Docs match implementation — ✅

## Not yet done

See `backlog.md`. Notably: live integration tests against DynamoDB Local, a
`count` command, projection/attribute selection, jsonl/table output formats,
consistent-read and reverse-order flags, shell completion.
