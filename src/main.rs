//! `ddb` CLI entry point.
//!
//! Responsibilities kept here (and only here): parse arguments, build the
//! AWS-backed read-only client, dispatch to a command handler, enforce the
//! stdout (data) / stderr (diagnostics) split, and translate errors into
//! process exit codes.

use std::io::Write;

use clap::Parser;

use ddb::cli::Cli;
use ddb::commands;
use ddb::dynamodb::client::AwsDynamoDbClient;
use ddb::output::json;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let exit_code = run(cli).await;
    std::process::exit(exit_code);
}

async fn run(cli: Cli) -> i32 {
    let reader =
        AwsDynamoDbClient::from_env(cli.profile.as_deref(), cli.region.as_deref()).await;

    match commands::run(&reader, &cli.command, cli.output).await {
        Ok(rendered) => {
            if !rendered.stdout.is_empty() {
                write_line(&mut std::io::stdout(), &rendered.stdout);
            }
            let mut stderr = std::io::stderr();
            for line in &rendered.stderr {
                write_line(&mut stderr, line);
            }
            0
        }
        Err(err) => {
            // On error, stdout is left empty so JSON pipelines are never
            // corrupted; diagnostics always go to stderr.
            let mut stderr = std::io::stderr();
            if cli.output.is_json() {
                write_line(&mut stderr, &json::render_error(&err));
            } else {
                write_line(&mut stderr, &format!("error: {err}"));
            }
            err.exit_code()
        }
    }
}

/// Write a line, tolerating a closed pipe (e.g. `ddb tables | head`) instead of
/// panicking.
fn write_line<W: Write>(writer: &mut W, line: &str) {
    let _ = writeln!(writer, "{line}");
}
