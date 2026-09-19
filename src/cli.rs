//! Command-line interface definition (clap).
//!
//! Every subcommand is a read-only DynamoDB inspection operation. There is no
//! subcommand that mutates DynamoDB state.

use clap::{Parser, Subcommand, ValueEnum};

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
    /// List all accessible DynamoDB tables, or interactively pick one.
    ///
    /// Run by a human in a terminal, this opens a fuzzy picker and activates the
    /// chosen table for the session (with the shell integration installed). When
    /// piped or with --output json (or --plain), it prints the plain list.
    Tables {
        /// Always print the plain list, even in an interactive terminal.
        #[arg(long)]
        plain: bool,
    },

    /// Activate a table for this shell session (venv-style).
    ///
    /// With a name, activates it directly; with no name in a terminal, opens the
    /// fuzzy picker. Requires the shell integration (`ddb shell-init`) to update
    /// the prompt and export DDB_TABLE.
    Use {
        /// Table name to activate; omit to pick interactively.
        table: Option<String>,
    },

    /// Clear the active table for this shell session.
    ///
    /// With the shell integration installed, `ddb deactivate` unsets DDB_TABLE
    /// in your current shell (the prompt chip disappears).
    Deactivate,

    /// Print shell integration to enable the venv-style active table.
    ///
    /// Add `eval "$(ddb shell-init zsh)"` (or bash) to your shell rc file.
    ShellInit {
        /// Which shell to emit integration for.
        #[arg(value_enum)]
        shell: ShellKind,
    },

    /// Describe a table's schema, indexes, and metadata.
    Describe {
        /// Table name (defaults to the active table).
        table: Option<String>,
    },

    /// List a table's secondary indexes.
    Indexes {
        /// Table name (defaults to the active table).
        table: Option<String>,
    },

    /// Get a single item by primary key.
    ///
    /// Keys may be given as a bare value (--pk 12345) or as name=value
    /// (--pk employee_id=12345). Value types are taken from the table schema.
    Get {
        /// Table name (defaults to the active table).
        table: Option<String>,
        /// Partition key value (bare value or name=value).
        #[arg(long)]
        pk: String,
        /// Sort key value, for tables with a composite primary key.
        #[arg(long)]
        sk: Option<String>,
    },

    /// Query items by partition key (and optional sort-key condition).
    Query {
        /// Table name (defaults to the active table).
        table: Option<String>,
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
        /// Table name (defaults to the active table).
        table: Option<String>,
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

/// Shells supported by `ddb shell-init`.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum ShellKind {
    Zsh,
    Bash,
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
        assert!(matches!(cli.command, Command::Tables { plain: false }));
        assert_eq!(cli.output, OutputFormat::Human);
    }

    #[test]
    fn parses_tables_plain() {
        let cli = Cli::try_parse_from(["ddb", "tables", "--plain"]).unwrap();
        assert!(matches!(cli.command, Command::Tables { plain: true }));
    }

    #[test]
    fn parses_use_with_and_without_table() {
        let bare = Cli::try_parse_from(["ddb", "use"]).unwrap();
        assert!(matches!(bare.command, Command::Use { table: None }));
        let named = Cli::try_parse_from(["ddb", "use", "Orders"]).unwrap();
        match named.command {
            Command::Use { table } => assert_eq!(table.as_deref(), Some("Orders")),
            _ => panic!("expected use"),
        }
    }

    #[test]
    fn parses_deactivate() {
        let cli = Cli::try_parse_from(["ddb", "deactivate"]).unwrap();
        assert!(matches!(cli.command, Command::Deactivate));
    }

    #[test]
    fn parses_shell_init() {
        let cli = Cli::try_parse_from(["ddb", "shell-init", "zsh"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::ShellInit {
                shell: ShellKind::Zsh
            }
        ));
        assert!(Cli::try_parse_from(["ddb", "shell-init", "fish"]).is_err());
    }

    #[test]
    fn query_allows_omitted_table() {
        let cli = Cli::try_parse_from(["ddb", "query", "--pk", "1"]).unwrap();
        match cli.command {
            Command::Query { table, pk, .. } => {
                assert_eq!(table, None);
                assert_eq!(pk, "1");
            }
            _ => panic!("expected query"),
        }
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
                assert_eq!(table.as_deref(), Some("EmployeeHistory"));
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
