//! `ddb describe <table>` and `ddb indexes <table>` — read table metadata.

use crate::dynamodb::{DynamoDbReader, SecondaryIndex, TableSchema};
use crate::error::DdbError;
use crate::output::{human, json, OutputFormat};

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    table: &str,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    let schema = reader.describe_table(table).await?;
    let stdout = if output.is_json() {
        json::render_table_schema(&schema)
    } else {
        human::render_table_schema(&schema)
    };
    Ok(Rendered::stdout(stdout))
}

pub async fn run_indexes<R: DynamoDbReader>(
    reader: &R,
    table: &str,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    let schema = reader.describe_table(table).await?;
    let stdout = if output.is_json() {
        render_indexes_json(&schema)
    } else {
        render_indexes_human(&schema)
    };
    Ok(Rendered::stdout(stdout))
}

fn render_indexes_json(schema: &TableSchema) -> String {
    let map = |list: &[SecondaryIndex]| -> Vec<serde_json::Value> {
        list.iter()
            .map(|i| {
                serde_json::json!({
                    "name": i.name,
                    "partition_key": i.key_schema.partition.name,
                    "sort_key": i.key_schema.sort.as_ref().map(|s| s.name.clone()),
                })
            })
            .collect()
    };
    let value = serde_json::json!({
        "global_secondary_indexes": map(&schema.global_secondary_indexes),
        "local_secondary_indexes": map(&schema.local_secondary_indexes),
    });
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| "null".to_string())
}

fn render_indexes_human(schema: &TableSchema) -> String {
    let mut lines = Vec::new();
    for (label, list) in [
        ("GSI", &schema.global_secondary_indexes),
        ("LSI", &schema.local_secondary_indexes),
    ] {
        for idx in list {
            let sort = idx
                .key_schema
                .sort
                .as_ref()
                .map(|s| format!(", sort={}", s.name))
                .unwrap_or_default();
            lines.push(format!(
                "{label}  {}  (partition={}{sort})",
                idx.name, idx.key_schema.partition.name
            ));
        }
    }
    if lines.is_empty() {
        format!("table '{}' has no secondary indexes", schema.name)
    } else {
        lines.join("\n")
    }
}
