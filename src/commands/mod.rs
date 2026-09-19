//! Command handlers.
//!
//! Each handler is generic over `R: DynamoDbReader` and therefore can only
//! perform read operations. Handlers do not print directly: they return a
//! [`Rendered`] value (stdout payload + stderr diagnostics) so they can be unit
//! tested against a fake reader, and so the `stdout`/`stderr` split is enforced
//! in exactly one place (`main`).

pub mod describe;
pub mod get;
pub mod query;
pub mod scan;
pub mod shell;
pub mod tables;
pub mod use_table;

use crate::active;
use crate::cli::Command;
use crate::dynamodb::DynamoDbReader;
use crate::error::DdbError;
use crate::output::OutputFormat;

/// The rendered result of a command: the stdout payload and any stderr
/// diagnostic lines (never written to stdout, so JSON output stays clean).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rendered {
    pub stdout: String,
    pub stderr: Vec<String>,
}

impl Rendered {
    pub fn stdout(s: impl Into<String>) -> Self {
        Rendered {
            stdout: s.into(),
            stderr: Vec::new(),
        }
    }

    pub fn with_stderr(mut self, line: impl Into<String>) -> Self {
        self.stderr.push(line.into());
        self
    }
}

/// Build the `Rendered` for a picker/`use` selection.
///
/// With the shell integration active (`DDB_ACTIVATE_FILE` set), the selected
/// name is written to that file — nothing goes to stdout — and the wrapper
/// exports it; a short confirmation goes to stderr. Without the integration, the
/// name is printed to stdout with a one-line hint on how to enable activation.
pub fn selection_rendered(selection: String) -> Rendered {
    if let Ok(path) = std::env::var(active::ACTIVATE_FILE_ENV) {
        if !path.is_empty() {
            return match std::fs::write(&path, &selection) {
                Ok(()) => Rendered::default().with_stderr(format!("activated {selection}")),
                Err(e) => Rendered::stdout(selection)
                    .with_stderr(format!("could not write activation file: {e}")),
            };
        }
    }
    let hint = crate::picker::activation_hint(&selection);
    let rendered = Rendered::stdout(selection);
    match hint {
        Some(line) => rendered.with_stderr(line),
        None => rendered,
    }
}

/// Handle `ddb deactivate` when it reaches the binary.
///
/// Normally the shell wrapper intercepts `deactivate` and unsets `DDB_TABLE`
/// directly (a child process cannot modify its parent shell's environment).
/// Reaching here means no wrapper is active, so we guide the user.
fn deactivate() -> Rendered {
    if active::active_table().is_some() {
        Rendered::default().with_stderr(
            "shell integration not active; run  unset DDB_TABLE  to deactivate, or add  \
             eval \"$(ddb shell-init zsh)\"  to your shell rc",
        )
    } else {
        Rendered::default().with_stderr("no active table")
    }
}

/// Dispatch a parsed command to its handler.
pub async fn run<R: DynamoDbReader>(
    reader: &R,
    command: &Command,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    match command {
        Command::Tables { plain } => tables::run(reader, output, *plain).await,
        Command::Use { table } => use_table::run(reader, table.as_deref()).await,
        Command::Deactivate => Ok(deactivate()),
        Command::ShellInit { shell } => Ok(shell::run(*shell)),
        Command::Describe { table } => {
            let table = active::resolve_table(table.as_deref())?;
            describe::run(reader, &table, output).await
        }
        Command::Indexes { table } => {
            let table = active::resolve_table(table.as_deref())?;
            describe::run_indexes(reader, &table, output).await
        }
        Command::Get { table, pk, sk } => {
            let table = active::resolve_table(table.as_deref())?;
            get::run(reader, &table, pk, sk.as_deref(), output).await
        }
        Command::Query {
            table,
            pk,
            sk,
            sk_begins_with,
            index,
            limit,
            max_pages,
        } => {
            let table = active::resolve_table(table.as_deref())?;
            query::run(
                reader,
                query::QueryArgs {
                    table: table.as_str(),
                    pk,
                    sk: sk.as_deref(),
                    sk_begins_with: sk_begins_with.as_deref(),
                    index: index.as_deref(),
                    limit: *limit,
                    max_pages: *max_pages,
                },
                output,
            )
            .await
        }
        Command::Scan {
            table,
            index,
            limit,
            max_pages,
        } => {
            let table = active::resolve_table(table.as_deref())?;
            scan::run(reader, &table, index.as_deref(), *limit, *max_pages, output).await
        }
    }
}
