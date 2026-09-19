//! `ddb use [table]` — pick/activate a table for the session.
//!
//! With a name, the table is validated against `ListTables` before activating,
//! so a typo fails immediately instead of activating a table that later 404s on
//! `get`/`query`/`scan`. Bare and interactive, it runs the fuzzy picker (whose
//! options are already real tables). The selected name is written to stdout /
//! the activation file for the shell wrapper to export; the picker UI is on stderr.

use crate::active;
use crate::dynamodb::DynamoDbReader;
use crate::error::DdbError;
use crate::picker;

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    table: Option<&str>,
) -> Result<Rendered, DdbError> {
    let names = reader.list_tables().await?;

    match table {
        Some(name) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(DdbError::InvalidUsage("table name is empty".to_string()));
            }
            if names.iter().any(|t| t == name) {
                Ok(super::selection_rendered(name.to_string()))
            } else {
                Err(unknown_table(name, &names))
            }
        }
        None => {
            if !picker::is_interactive() {
                return Err(DdbError::InvalidUsage(
                    "no table given; run `ddb use <table>`, or use an interactive \
                     terminal to pick from a list"
                        .to_string(),
                ));
            }
            if names.is_empty() {
                return Ok(Rendered::default().with_stderr("no tables found"));
            }
            match picker::select_table(&names, active::active_table().as_deref())? {
                Some(selection) => Ok(super::selection_rendered(selection)),
                None => Ok(Rendered::default()),
            }
        }
    }
}

/// Error for a `use <name>` that doesn't match any table. If a table differs
/// only by case (a likely typo), suggest it; otherwise report table-not-found.
fn unknown_table(name: &str, names: &[String]) -> DdbError {
    match names.iter().find(|t| t.eq_ignore_ascii_case(name)) {
        Some(suggestion) => DdbError::InvalidUsage(format!(
            "table '{name}' was not found (did you mean '{suggestion}'?)"
        )),
        None => DdbError::TableNotFound(name.to_string()),
    }
}
