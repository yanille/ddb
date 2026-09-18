# Current task

## State (2026-09-17)

The initial version of `ddb` is **implemented, tested, and documented**. The
repository started empty; this session built the full initial release per the
build specification.

- All Definition-of-Done items are met (see `progress.md`).
- `cargo build`, `cargo test` (70 tests passing), and `cargo clippy
  --all-targets` are clean.
- The read-only guarantee is enforced by `tests/no_write_path.rs`.
- AWS credential/region resolution from the shared files (`~/.aws/credentials`,
  `~/.aws/config`) is verified live and by `tests/aws_config.rs`.
- End-to-end AWS wiring verified via a live `ddb tables` call and the
  no-credentials path (exit 5, structured JSON error on stderr, empty stdout).

## In progress

Nothing actively in flight.

## Recommended next steps

1. **Live integration tests** against DynamoDB Local (or `aws-smithy-mocks`) to
   exercise real pagination and AWS error mapping (access denied, not found).
   This is the biggest remaining testing gap.
2. **`ddb count`** command (read-only `Select=COUNT`).
3. **Output formats** `jsonl` / `table`.
4. Sort-key operators and projection/attribute selection.

See `backlog.md` for the full list.

## Blockers

None.

## Assumptions

- AWS SDK crate versions resolved at build time: `aws-sdk-dynamodb` 1.126,
  `aws-config` 1.12 (see `Cargo.lock`). Behavior version is
  `BehaviorVersion::latest()`.
- Rust edition 2021, toolchain 1.97.
- Number→JSON precision cutoff is 15 significant digits (documented in
  `decisions.md` D6).
- `--max-pages` bounds pages; `--limit` is a per-page cap (DynamoDB semantics),
  not a total-item cap.
