//! `ddb tables` — list all accessible tables, or interactively pick one.
//!
//! When a human runs `ddb tables` (stdout/stderr are a TTY, `human` output, and
//! not `--plain`), it presents a fuzzy picker and prints the chosen table to
//! stdout so the shell wrapper can activate it. When piped or in `--output json`
//! (or with `--plain`), it prints the plain list exactly as before, preserving
//! scriptability.

use crate::active;
use crate::dynamodb::DynamoDbReader;
use crate::error::DdbError;
use crate::output::{human, json, OutputFormat};
use crate::picker;

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    output: OutputFormat,
    plain: bool,
) -> Result<Rendered, DdbError> {
    let names = reader.list_tables().await?;

    let want_picker =
        !plain && !output.is_json() && !names.is_empty() && picker::is_interactive();
    if want_picker {
        return match picker::select_table(&names, active::active_table().as_deref())? {
            Some(selection) => Ok(super::selection_rendered(selection)),
            None => Ok(Rendered::default()),
        };
    }

    let stdout = if output.is_json() {
        json::render_tables(&names)
    } else {
        human::render_tables(&names)
    };
    let rendered = Rendered::stdout(stdout);
    if !output.is_json() && names.is_empty() {
        Ok(rendered.with_stderr("no tables found"))
    } else {
        Ok(rendered)
    }
}
