//! `ddb use [table]` — pick/activate a table for the session.
//!
//! With a name, echoes it (the shell wrapper exports it as `DDB_TABLE`). Bare
//! and interactive, runs the fuzzy picker. The selected name is written to
//! stdout for the wrapper to capture; the picker UI is drawn on stderr.
//!
//! Activation is intentionally not validated against AWS here: it stays fast and
//! works offline, mirroring how `source venv/bin/activate` does not check
//! anything. A mistyped name surfaces as a normal `table_not_found` on first use.

use crate::active;
use crate::dynamodb::DynamoDbReader;
use crate::error::DdbError;
use crate::picker;

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    table: Option<&str>,
) -> Result<Rendered, DdbError> {
    if let Some(name) = table {
        let name = name.trim();
        if name.is_empty() {
            return Err(DdbError::InvalidUsage("table name is empty".to_string()));
        }
        return Ok(super::selection_rendered(name.to_string()));
    }

    if !picker::is_interactive() {
        return Err(DdbError::InvalidUsage(
            "no table given; run `ddb use <table>`, or use an interactive terminal \
             to pick from a list"
                .to_string(),
        ));
    }

    let names = reader.list_tables().await?;
    if names.is_empty() {
        return Ok(Rendered::default().with_stderr("no tables found"));
    }
    match picker::select_table(&names, active::active_table().as_deref())? {
        Some(selection) => Ok(super::selection_rendered(selection)),
        None => Ok(Rendered::default()),
    }
}
