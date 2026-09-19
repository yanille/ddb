# Decisions

Architectural decisions evident in the codebase. "Decision" and "Evidence" are
observed facts; "Rationale" is inferred where noted.

## D1 — Read-only enforced by a trait boundary, not discipline

- **Decision:** All DynamoDB access goes through the `DynamoDbReader` trait
  (`src/dynamodb/mod.rs`), which exposes only read methods. Command handlers are
  generic over `R: DynamoDbReader`.
- **Evidence:** Handlers in `src/commands/*` take `&R` and never see the SDK
  `Client`; only `AwsDynamoDbClient` (`client.rs`) holds it, and `main.rs` is the
  only constructor.
- **Rationale:** The spec requires the read-only property to be structural. A
  handler cannot call a write API because the trait gives it no such method.

## D2 — Source-level write-path test as a hard guarantee

- **Decision:** `tests/no_write_path.rs` scans `src/` for mutating DynamoDB SDK
  operations (in `.op(` call form) and fails if any appear.
- **Evidence:** The test enumerates `put_item`, `update_item`, `delete_item`,
  batch/transact writes, table admin ops, backup/restore, streaming destinations,
  resource policies, import/export, and PartiQL statement APIs.
- **Rationale:** Makes the safety invariant machine-checkable and regression-proof
  even against future edits. Matching the call form (`.op(`) avoids false
  positives from prose in comments/docs.

## D3 — clap derive for the CLI

- **Decision:** `clap` v4 with the derive API; global flags via `global = true`.
- **Evidence:** `src/cli.rs`.
- **Rationale (inferred):** Recommended in the spec; derive keeps the CLI surface
  declarative and generates `--help`/validation. Global flags let `--output`,
  `--profile`, `--region` appear before or after the subcommand.

## D4 — Handlers return `Rendered`, printing happens only in `main`

- **Decision:** Handlers produce `Rendered { stdout, stderr }`; `main` performs
  all real I/O and sets exit codes.
- **Evidence:** `src/commands/mod.rs`, `src/main.rs`.
- **Rationale:** Enables unit testing handlers against a fake reader without
  capturing process stdout, and centralizes the stdout(data)/stderr(diagnostics)
  contract in one place.

## D5 — Schema-driven key typing

- **Decision:** `get`/`query` call `DescribeTable` first and type key values per
  the schema (`S`/`N`/`B`) rather than assuming strings.
- **Evidence:** `src/keys.rs`, `src/commands/get.rs`, `src/commands/query.rs`.
- **Rationale:** The spec explicitly forbids silently assuming string keys.
  `name=value` is disambiguated from `=`-containing values by matching key names,
  which also makes base64 binary keys work.

## D6 — Precision-safe number conversion

- **Decision:** DynamoDB `N` becomes a JSON number only when it has ≤15
  significant digits (and parses finitely); otherwise the exact decimal string is
  preserved. Integers within `i64`/`u64` are always exact.
- **Evidence:** `number_to_json` / `significant_digits` in `src/dynamodb/convert.rs`.
- **Rationale:** DynamoDB numbers are arbitrary-precision (up to 38 digits); f64
  reliably carries ~15. Preserving the string avoids silent precision loss while
  keeping ordinary numbers as JSON numbers.

## D7 — Binary as base64

- **Decision:** `B`/`BS` are base64-encoded in all output; binary key inputs are
  base64-decoded.
- **Evidence:** `convert.rs`, `keys.rs`.
- **Rationale:** The spec forbids emitting arbitrary/invalid UTF-8; base64 is a
  deterministic, round-trippable representation.

## D8 — Deterministic output (sorted keys)

- **Decision:** Object keys are emitted in sorted order in both JSON and human
  output.
- **Evidence:** `BTreeMap` use in `convert.rs`.
- **Rationale:** The spec asks for deterministic output where practical; stable
  ordering aids diffing and scripting.

## D9 — Conservative scan defaults

- **Decision:** `scan` defaults to `--limit 100`, `--max-pages 1`; `query`
  defaults to `--max-pages 10` (no per-page limit). Truncation is reported on
  stderr. `--max-pages` must be ≥ 1.
- **Evidence:** `src/cli.rs`.
- **Rationale:** The spec wants it hard to accidentally pull an enormous dataset.
  Query is partition-scoped so it gets a looser page budget than scan.

## D10 — Errors mapped to categories; secrets never leaked

- **Decision:** AWS SDK errors are classified into `TableNotFound`,
  `IndexNotFound`, `AccessDenied`, `NoCredentials`, `InvalidUsage`, `Service`,
  each with a stable exit code and machine code. Credentials failures are detected
  structurally (service error codes) with a string-heuristic fallback.
- **Evidence:** `src/error.rs`.
- **Rationale:** Developer-useful diagnostics without exposing credential
  material. Error messages are built by walking the error `source()` chain, which
  contains AWS codes, not secrets.

## D11 — JSON errors to stderr, stdout stays empty on failure

- **Decision:** In `--output json`, an error prints a JSON object to **stderr**
  and leaves stdout empty.
- **Evidence:** `src/main.rs`.
- **Rationale:** Keeps `ddb ... --output json | jq .` pipelines uncorrupted; a
  failed command yields no stdout payload rather than a mix of data and error.

## D12 — Library + binary split

- **Decision:** Logic lives in a library crate (`lib.rs`); `main.rs` is a thin
  binary over it.
- **Evidence:** `Cargo.toml` `[lib]` implied by `src/lib.rs`; `[[bin]]` for `ddb`.
- **Rationale:** Lets integration tests (`tests/`) exercise handlers and the
  read-only trait directly.

## D13 — Rely on the default AWS provider chain, do not hardcode file paths

- **Decision:** Credentials/region are loaded via `aws_config::defaults(...)`
  (`client::load_config`), which already consults the shared credentials file
  (`~/.aws/credentials`) and config file (`~/.aws/config`) alongside env vars,
  SSO, and IAM-role/metadata providers. `ddb` does not construct file-only
  providers.
- **Evidence:** `src/dynamodb/client.rs::load_config`; verified live and by
  `tests/aws_config.rs` (region from config file, credentials from credentials
  file, named profiles, and `--region` override).
- **Rationale:** The spec requires all standard sources (env, files, profiles,
  SSO, IAM roles, container/EC2 metadata) to keep working. Hardcoding the shared
  files would satisfy "read from ~/.aws/*" but break SSO/role/env resolution.
  The default chain honors the shared files *and* the other sources; the
  regression test locks in the shared-file behavior specifically.

## D14 — Active table as a shell-session env var, activated via a shell wrapper

- **Decision:** A table can be "activated" for the session; the active table is
  stored in the `DDB_TABLE` environment variable and surfaced in the prompt as
  `(ddb:Name)`, virtualenv-style. Data commands take an optional `[table]`;
  `active::resolve_table` applies *explicit arg > `$DDB_TABLE` > error*. `ddb
  shell-init <shell>` emits a `ddb` shell function that captures the picker/`use`
  selection and exports `DDB_TABLE` in the parent shell; `deactivate` unsets it.
- **Evidence:** `src/active.rs`, `src/commands/shell.rs`, `src/commands/use_table.rs`,
  dispatch in `src/commands/mod.rs`.
- **Rationale:** A child process can't mutate its parent shell's env or prompt, so
  the established pattern (`pyenv`/`nvm`/`conda`) is a sourced shell function. Env
  scoping keeps activation per-terminal, matching the virtualenv mental model the
  user asked for. The explicit arg always overrides, so scripts are unaffected.

## D15 — Interactive picker is TTY-gated; `dialoguer` renders on stderr

- **Decision:** `ddb tables` (and bare `ddb use`) open a fuzzy picker only when
  **stdin and stderr** are TTYs and output is `human` and not `--plain`;
  otherwise `tables` prints the plain list. The picker (`dialoguer::FuzzySelect`)
  draws on stderr; the selected name is printed to stdout. Crucially the gate
  does **not** require stdout to be a TTY, because the shell wrapper activates a
  selection by capturing stdout (`$(command ddb …)`) — which makes stdout a pipe.
  Gating on stdout (the original bug) made the picker never run under the wrapper,
  so selections were never captured and later commands had no active table. When
  a selection is made but stdout is still a TTY (no wrapper), an
  `activation_hint` is printed to stderr telling the user how to enable/replicate
  activation.
- **Evidence:** `src/picker.rs` (`is_interactive`, `activation_hint`),
  `src/commands/tables.rs`, `src/commands/use_table.rs`, `selection_rendered` in
  `src/commands/mod.rs`. Verified via a PTY test (stdout piped, stderr/stdin a
  TTY): the picker runs and stdout stays empty until selection.
- **Rationale:** Preserves `ddb tables` scriptability (`| grep`, `| jq`) while
  giving humans the picker. Drawing UI on stderr keeps stdout a clean, capturable
  single value for the shell wrapper. `dialoguer` was chosen over `inquire`
  because it renders to stderr by default, guaranteeing the clean-stdout contract.

## Open items / conflicts

None. No feature to date has required AWS functionality beyond reads. If one ever
does, per the spec it must be documented here and in `backlog.md` rather than
silently expanding scope.
