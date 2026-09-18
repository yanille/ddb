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
pub mod tables;

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

/// Dispatch a parsed command to its handler.
pub async fn run<R: DynamoDbReader>(
    reader: &R,
    command: &Command,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    match command {
        Command::Tables => tables::run(reader, output).await,
        Command::Describe { table } => describe::run(reader, table, output).await,
        Command::Indexes { table } => describe::run_indexes(reader, table, output).await,
        Command::Get { table, pk, sk } => {
            get::run(reader, table, pk, sk.as_deref(), output).await
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
            query::run(
                reader,
                query::QueryArgs {
                    table,
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
            scan::run(reader, table, index.as_deref(), *limit, *max_pages, output).await
        }
    }
}
