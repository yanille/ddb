//! End-to-end command-handler tests using an in-memory fake reader.
//!
//! These exercise the full path from a parsed `Command` through a handler to
//! rendered stdout/stderr, without touching AWS. They cover table listing, get
//! (hit and miss), query (with schema-driven key typing), JSON vs human output,
//! and empty results.

use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;

use ddb::cli::Command;
use ddb::commands;
use ddb::dynamodb::{
    DynamoDbReader, GetItemRequest, Item, KeyDef, KeySchema, Page, QueryRequest, ScalarType,
    ScanRequest, TableSchema,
};
use ddb::error::DdbError;
use ddb::output::OutputFormat;

/// A configurable in-memory reader.
struct FakeReader {
    tables: Vec<String>,
    schema: TableSchema,
    get_result: Option<Item>,
    query_items: Vec<Item>,
}

impl FakeReader {
    fn new() -> Self {
        FakeReader {
            tables: vec!["Users".into(), "Orders".into()],
            schema: TableSchema {
                name: "Users".into(),
                key_schema: KeySchema {
                    partition: KeyDef {
                        name: "id".into(),
                        attr_type: ScalarType::Number,
                    },
                    sort: None,
                },
                global_secondary_indexes: vec![],
                local_secondary_indexes: vec![],
                item_count: Some(2),
                size_bytes: Some(128),
                status: Some("ACTIVE".into()),
                billing_mode: Some("PAY_PER_REQUEST".into()),
            },
            get_result: None,
            query_items: vec![],
        }
    }
}

impl DynamoDbReader for FakeReader {
    async fn list_tables(&self) -> Result<Vec<String>, DdbError> {
        Ok(self.tables.clone())
    }

    async fn describe_table(&self, table: &str) -> Result<TableSchema, DdbError> {
        if table == self.schema.name {
            Ok(self.schema.clone())
        } else {
            Err(DdbError::TableNotFound(table.to_string()))
        }
    }

    async fn get_item(&self, _request: GetItemRequest) -> Result<Option<Item>, DdbError> {
        Ok(self.get_result.clone())
    }

    async fn query(&self, _request: QueryRequest) -> Result<Page, DdbError> {
        Ok(Page {
            items: self.query_items.clone(),
            scanned_count: self.query_items.len() as i64,
            truncated: false,
        })
    }

    async fn scan(&self, _request: ScanRequest) -> Result<Page, DdbError> {
        Ok(Page {
            items: self.query_items.clone(),
            scanned_count: self.query_items.len() as i64,
            truncated: false,
        })
    }
}

fn item(pairs: &[(&str, AttributeValue)]) -> Item {
    let mut m = HashMap::new();
    for (k, v) in pairs {
        m.insert((*k).to_string(), v.clone());
    }
    m
}

#[tokio::test]
async fn tables_human() {
    let reader = FakeReader::new();
    let out = commands::run(&reader, &Command::Tables { plain: true }, OutputFormat::Human)
        .await
        .unwrap();
    assert_eq!(out.stdout, "Users\nOrders");
}

#[tokio::test]
async fn tables_json() {
    let reader = FakeReader::new();
    let out = commands::run(&reader, &Command::Tables { plain: true }, OutputFormat::Json)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed, serde_json::json!(["Users", "Orders"]));
}

#[tokio::test]
async fn get_hit_json_uses_typed_key() {
    let mut reader = FakeReader::new();
    reader.get_result = Some(item(&[
        ("id", AttributeValue::N("12345".into())),
        ("name", AttributeValue::S("Jane".into())),
    ]));
    let cmd = Command::Get {
        table: Some("Users".into()),
        pk: "12345".into(),
        sk: None,
    };
    let out = commands::run(&reader, &cmd, OutputFormat::Json)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed, serde_json::json!({"id": 12345, "name": "Jane"}));
}

#[tokio::test]
async fn get_miss_human_reports_on_stderr() {
    let reader = FakeReader::new();
    let cmd = Command::Get {
        table: Some("Users".into()),
        pk: "999".into(),
        sk: None,
    };
    let out = commands::run(&reader, &cmd, OutputFormat::Human)
        .await
        .unwrap();
    assert_eq!(out.stdout, "");
    assert_eq!(out.stderr, vec!["item not found".to_string()]);
}

#[tokio::test]
async fn get_miss_json_is_null() {
    let reader = FakeReader::new();
    let cmd = Command::Get {
        table: Some("Users".into()),
        pk: "999".into(),
        sk: None,
    };
    let out = commands::run(&reader, &cmd, OutputFormat::Json)
        .await
        .unwrap();
    assert_eq!(out.stdout, "null");
}

#[tokio::test]
async fn get_bad_number_key_is_usage_error() {
    let reader = FakeReader::new();
    let cmd = Command::Get {
        table: Some("Users".into()),
        pk: "not-a-number".into(),
        sk: None,
    };
    let err = commands::run(&reader, &cmd, OutputFormat::Json)
        .await
        .unwrap_err();
    assert!(matches!(err, DdbError::InvalidUsage(_)));
    assert_eq!(err.exit_code(), 2);
}

#[tokio::test]
async fn get_missing_table_errors() {
    let reader = FakeReader::new();
    let cmd = Command::Get {
        table: Some("Nope".into()),
        pk: "1".into(),
        sk: None,
    };
    let err = commands::run(&reader, &cmd, OutputFormat::Json)
        .await
        .unwrap_err();
    assert!(matches!(err, DdbError::TableNotFound(_)));
    assert_eq!(err.exit_code(), 3);
}

#[tokio::test]
async fn query_json_returns_array() {
    let mut reader = FakeReader::new();
    reader.query_items = vec![
        item(&[("id", AttributeValue::N("1".into()))]),
        item(&[("id", AttributeValue::N("2".into()))]),
    ];
    let cmd = Command::Query {
        table: Some("Users".into()),
        pk: "1".into(),
        sk: None,
        sk_begins_with: None,
        index: None,
        limit: None,
        max_pages: 10,
    };
    let out = commands::run(&reader, &cmd, OutputFormat::Json)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed, serde_json::json!([{"id": 1}, {"id": 2}]));
}

#[tokio::test]
async fn query_empty_results_human() {
    let reader = FakeReader::new();
    let cmd = Command::Query {
        table: Some("Users".into()),
        pk: "1".into(),
        sk: None,
        sk_begins_with: None,
        index: None,
        limit: None,
        max_pages: 10,
    };
    let out = commands::run(&reader, &cmd, OutputFormat::Human)
        .await
        .unwrap();
    assert_eq!(out.stdout, "");
    // Summary line present on stderr.
    assert!(out.stderr.iter().any(|l| l.contains("0 item(s)")));
}

#[tokio::test]
async fn use_valid_table_activates() {
    let reader = FakeReader::new(); // tables: Users, Orders
    let out = commands::run(
        &reader,
        &Command::Use {
            table: Some("Users".into()),
        },
        OutputFormat::Human,
    )
    .await
    .unwrap();
    assert_eq!(out.stdout, "Users");
}

#[tokio::test]
async fn use_unknown_table_errors_not_found() {
    let reader = FakeReader::new();
    let err = commands::run(
        &reader,
        &Command::Use {
            table: Some("Nope".into()),
        },
        OutputFormat::Human,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, DdbError::TableNotFound(_)));
    assert_eq!(err.exit_code(), 3);
}

#[tokio::test]
async fn use_case_typo_suggests_correct_table() {
    let reader = FakeReader::new();
    let err = commands::run(
        &reader,
        &Command::Use {
            table: Some("users".into()), // wrong case of "Users"
        },
        OutputFormat::Human,
    )
    .await
    .unwrap_err();
    match err {
        DdbError::InvalidUsage(msg) => assert!(msg.contains("Users"), "expected suggestion: {msg}"),
        other => panic!("expected InvalidUsage with suggestion, got {other:?}"),
    }
}

#[tokio::test]
async fn describe_json_has_key_schema() {
    let reader = FakeReader::new();
    let cmd = Command::Describe {
        table: Some("Users".into()),
    };
    let out = commands::run(&reader, &cmd, OutputFormat::Json)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(parsed["key_schema"]["partition_key"], "id");
    assert_eq!(parsed["status"], "ACTIVE");
}
