//! `ddb scan <table>` — read arbitrary items, with safety defaults.

use crate::dynamodb::{DynamoDbReader, ScanRequest};
use crate::error::DdbError;
use crate::output::{human, json, OutputFormat};

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    table: &str,
    index: Option<&str>,
    limit: i32,
    max_pages: usize,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    // Validate the index exists (and give a precise error) before scanning.
    if let Some(index_name) = index {
        let schema = reader.describe_table(table).await?;
        schema.key_schema_for(Some(index_name))?;
    }

    let page = reader
        .scan(ScanRequest {
            table: table.to_string(),
            index: index.map(|s| s.to_string()),
            limit: Some(limit),
            max_pages,
        })
        .await?;

    let stdout = if output.is_json() {
        json::render_items(&page.items)
    } else {
        human::render_items(&page.items)
    };

    let mut rendered = Rendered::stdout(stdout);
    if !output.is_json() {
        rendered = rendered.with_stderr(format!(
            "{} item(s), {} scanned",
            page.items.len(),
            page.scanned_count
        ));
        if page.truncated {
            rendered = rendered.with_stderr(format!(
                "results truncated at {max_pages} page(s) of up to {limit} item(s); \
                 raise --limit/--max-pages to read more"
            ));
        }
    }
    Ok(rendered)
}
