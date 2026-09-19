# Backlog

Distinguishes confirmed gaps from suggested future work. Nothing here may break
the read-only guarantee.

## Confirmed gaps / follow-ups

- **Live integration tests (DynamoDB Local)**: current tests use a fake reader
  and one live no-credentials check. Add tests against DynamoDB Local (or the
  `aws-smithy-mocks` harness) covering real pagination, access-denied, and
  resource-not-found responses. *(Testing gap.)*
- **Access-denied / not-found live verification**: `map_sdk_error` classification
  for `AccessDeniedException` and `ResourceNotFoundException` is unit-reasoned but
  not exercised against real SDK error values.
- **`scan`/`query` count semantics**: `Page.scanned_count` is summed across pages;
  there is no separate returned item-count field beyond `items.len()`.

## Active-table feature follow-ups

- `fish` shell-init (only `zsh`/`bash` are emitted today).
- Optional table-name **cache** to make the picker/`tables` instant and cut
  `ListTables` calls (deliberately deferred; revisit if latency is felt).
- Interactive picker has no automated test (needs a PTY); pure helpers and the
  TTY gating are covered, and the picker was verified manually.

## Suggested features (future)

- `ddb count <table>`: count via `Select=COUNT` scan/query (still read-only),
  with pagination and the same safeguards.
- Output formats `jsonl` and `table` (spec lists `human`, `json`, `jsonl`,
  `table`; only the first two are implemented).
- Projection / attribute selection (`--attributes a,b,c`) using
  `ProjectionExpression`.
- Sort-key operators beyond equals/begins_with: `>`, `>=`, `<`, `<=`, `between`.
- Consistent reads (`--consistent`) for `get`/`query`.
- Reverse ordering (`--reverse` → `ScanIndexForward=false`) for `query`.
- `--all` to fetch every page (only after documenting its safety implications).
- Shell completion generation (clap supports it).
- Colored terminal output; timing information; optional watch mode.
- Fuzzy table-name matching / interactive table selection.
- Personal config file for CLI defaults (e.g. default region/output).

## Technical debt / polish

- Broken-pipe handling: `main::write_line` ignores write errors so `| head`
  doesn't panic, but Rust still ignores SIGPIPE globally; consider restoring
  default SIGPIPE behavior for cleaner semantics.
- Number precision threshold (15 significant digits) is a heuristic; document/
  revisit if users need exact-decimal JSON numbers via an opt-in flag.
- `describe` human output could show throughput/GSI projection details; currently
  summarized.

## Safety notes

- No telemetry, analytics, or network services beyond direct AWS calls — keep it
  that way.
- Any request for a write/mutation feature is out of scope: document the conflict
  here and in `decisions.md` instead of implementing it.
