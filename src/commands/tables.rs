//! `ddb tables` — list all accessible tables.

use crate::dynamodb::DynamoDbReader;
use crate::error::DdbError;
use crate::output::{human, json, OutputFormat};

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    let names = reader.list_tables().await?;
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
