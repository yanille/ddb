//! Command-line interface definition (clap).
//!
//! Every subcommand is a read-only DynamoDB inspection operation. There is no
//! subcommand that mutates DynamoDB state.

use clap::{Parser, Subcommand};

use crate::output::OutputFormat;

/// A fast, terminal-native, strictly read-only DynamoDB inspection tool.
#[derive(Parser, Debug)]
#[command(
    name = "ddb",
    version,
    about = "Strictly read-only DynamoDB inspection CLI",
    long_about = "ddb is a developer-focused, strictly read-only CLI for inspecting Amazon \
DynamoDB data from the terminal. It can never create, update, or delete DynamoDB \
data or configuration.\n\nAWS credentials and region are resolved through the \
standard AWS configuration chain (environment, shared config/credentials files, \
profiles, SSO, and IAM roles)."
)]
pub struct Cli {
    /// AWS profile to use (defaults to the standard resolution chain).
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// AWS region to use (e.g. us-west-2).
    #[arg(long, global = true)]
    pub region: Option<String>,

    /// Output format.
    #[arg(long, value_enum, global = true, default_value_t = OutputFormat::Human)]
    pub output: OutputFormat,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List all accessible DynamoDB tables.
    Tables,

    /// Describe a table's schema, indexes, and metadata.
    Describe {
        /// Table name.
        table: String,
    },

    /// List a table's secondary indexes.
    Indexes {
        /// Table name.
        table: String,
    },

    /// Get a single item by primary key.
    ///
    /// Keys may be given as a bare value (--pk 12345) or as name=value
    /// (--pk employee_id=12345). Value types are taken from the table schema.
    Get {
        /// Table name.
        table: String,
        /// Partition key value (bare value or name=value).
        #[arg(long)]
        pk: String,
        /// Sort key value, for tables with a composite primary key.
        #[arg(long)]
        sk: Option<String>,
    },

    /// Query items by partition key (and optional sort-key condition).
    Query {
        /// Table name.
        table: String,
        /// Partition key value (bare value or name=value).
        #[arg(long)]
        pk: String,
        /// Sort key equality condition.
        #[arg(long, conflicts_with = "sk_begins_with")]
        sk: Option<String>,
        /// Sort key begins_with condition (string/binary keys only).
        #[arg(long = "sk-begins-with", conflicts_with = "sk")]
        sk_begins_with: Option<String>,
        /// Query a secondary index instead of the base table.
        #[arg(long)]
        index: Option<String>,
        /// Maximum items per page requested from DynamoDB.
        #[arg(long)]
        limit: Option<i32>,
        /// Maximum number of result pages to fetch before stopping.
        #[arg(long = "max-pages", default_value_t = 10, value_parser = parse_max_pages)]
        max_pages: usize,
    },

    /// Scan items from a table or index.
    ///
    /// Scans can be expensive; by default this reads a single page of up to
    /// --limit items. Raise --limit and --max-pages deliberately to read more.
    Scan {
        /// Table name.
        table: String,
        /// Scan a secondary index instead of the base table.
        #[arg(long)]
        index: Option<String>,
        /// Maximum items per page requested from DynamoDB.
        #[arg(long, default_value_t = 100)]
        limit: i32,
        /// Maximum number of result pages to fetch before stopping.
        #[arg(long = "max-pages", default_value_t = 1, value_parser = parse_max_pages)]
        max_pages: usize,
    },
}

/// Parse a `--max-pages` value, enforcing a minimum of 1.
fn parse_max_pages(s: &str) -> Result<usize, String> {
    let n: usize = s
        .parse()
        .map_err(|_| format!("`{s}` is not a valid number"))?;
    if n < 1 {
        return Err("must be at least 1".to_string());
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_tables() {
        let cli = Cli::try_parse_from(["ddb", "tables"]).unwrap();
        assert!(matches!(cli.command, Command::Tables));
        assert_eq!(cli.output, OutputFormat::Human);
    }

    #[test]
    fn parses_global_flags_after_subcommand() {
        let cli =
            Cli::try_parse_from(["ddb", "tables", "--output", "json", "--region", "us-west-2"])
                .unwrap();
        assert_eq!(cli.output, OutputFormat::Json);
        assert_eq!(cli.region.as_deref(), Some("us-west-2"));
    }

    #[test]
    fn parses_get_with_composite_key() {
        let cli = Cli::try_parse_from([
            "ddb",
            "get",
            "EmployeeHistory",
            "--pk",
            "employee_id=12345",
            "--sk",
            "timestamp=2026-09-17",
        ])
        .unwrap();
        match cli.command {
            Command::Get { table, pk, sk } => {
                assert_eq!(table, "EmployeeHistory");
                assert_eq!(pk, "employee_id=12345");
                assert_eq!(sk.as_deref(), Some("timestamp=2026-09-17"));
            }
            _ => panic!("expected get"),
        }
    }

    #[test]
    fn query_sk_and_begins_with_are_mutually_exclusive() {
        let result = Cli::try_parse_from([
            "ddb",
            "query",
            "T",
            "--pk",
            "1",
            "--sk",
            "a",
            "--sk-begins-with",
            "b",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn query_defaults() {
        let cli = Cli::try_parse_from(["ddb", "query", "T", "--pk", "1"]).unwrap();
        match cli.command {
            Command::Query {
                max_pages, limit, ..
            } => {
                assert_eq!(max_pages, 10);
                assert_eq!(limit, None);
            }
            _ => panic!("expected query"),
        }
    }

    #[test]
    fn scan_defaults_are_safe() {
        let cli = Cli::try_parse_from(["ddb", "scan", "T"]).unwrap();
        match cli.command {
            Command::Scan {
                limit, max_pages, ..
            } => {
                assert_eq!(limit, 100);
                assert_eq!(max_pages, 1);
            }
            _ => panic!("expected scan"),
        }
    }

    #[test]
    fn max_pages_zero_is_rejected() {
        assert!(Cli::try_parse_from(["ddb", "scan", "T", "--max-pages", "0"]).is_err());
    }
}
