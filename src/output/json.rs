//! JSON rendering. All functions return a `String` of valid JSON; callers write
//! it to stdout. Object keys are emitted in sorted order (see `convert`), so
//! output is deterministic.

use serde_json::{json, Value};

use crate::dynamodb::{convert, Item, TableSchema};
use crate::error::DdbError;

/// Render a list of table names as a JSON array of strings.
pub fn render_tables(names: &[String]) -> String {
    to_pretty(&Value::Array(
        names.iter().map(|n| Value::String(n.clone())).collect(),
    ))
}

/// Render a single item (or JSON `null` when absent).
pub fn render_item(item: Option<&Item>) -> String {
    match item {
        Some(item) => to_pretty(&convert::item_to_json(item)),
        None => "null".to_string(),
    }
}

/// Render a list of items as a JSON array of objects.
pub fn render_items(items: &[Item]) -> String {
    let arr: Vec<Value> = items.iter().map(convert::item_to_json).collect();
    to_pretty(&Value::Array(arr))
}

/// Render a table schema as a JSON object.
pub fn render_table_schema(schema: &TableSchema) -> String {
    let index = |list: &[crate::dynamodb::SecondaryIndex]| -> Value {
        Value::Array(
            list.iter()
                .map(|i| {
                    json!({
                        "name": i.name,
                        "partition_key": i.key_schema.partition.name,
                        "sort_key": i.key_schema.sort.as_ref().map(|s| s.name.clone()),
                    })
                })
                .collect(),
        )
    };

    let value = json!({
        "name": schema.name,
        "status": schema.status,
        "item_count": schema.item_count,
        "size_bytes": schema.size_bytes,
        "billing_mode": schema.billing_mode,
        "key_schema": {
            "partition_key": schema.key_schema.partition.name,
            "sort_key": schema.key_schema.sort.as_ref().map(|s| s.name.clone()),
        },
        "global_secondary_indexes": index(&schema.global_secondary_indexes),
        "local_secondary_indexes": index(&schema.local_secondary_indexes),
    });
    to_pretty(&value)
}

/// Render an error as a JSON object for `--output json` failures.
pub fn render_error(err: &DdbError) -> String {
    to_pretty(&json!({
        "error": {
            "code": err.code(),
            "message": err.to_string(),
        }
    }))
}

fn to_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| "null".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_dynamodb::types::AttributeValue;
    use std::collections::HashMap;

    #[test]
    fn tables_render_as_array() {
        let out = render_tables(&["Users".into(), "Orders".into()]);
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed, json!(["Users", "Orders"]));
    }

    #[test]
    fn missing_item_is_json_null() {
        assert_eq!(render_item(None), "null");
    }

    #[test]
    fn item_renders_as_object() {
        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert("id".into(), AttributeValue::N("12345".into()));
        item.insert("name".into(), AttributeValue::S("Jane".into()));
        let out = render_item(Some(&item));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed, json!({"id": 12345, "name": "Jane"}));
    }

    #[test]
    fn items_render_as_array_of_objects() {
        let mut a: HashMap<String, AttributeValue> = HashMap::new();
        a.insert("id".into(), AttributeValue::N("1".into()));
        let out = render_items(&[a]);
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed, json!([{"id": 1}]));
    }

    #[test]
    fn empty_items_render_as_empty_array() {
        let out = render_items(&[]);
        assert_eq!(out, "[]");
    }

    #[test]
    fn error_renders_with_code_and_message() {
        let out = render_error(&DdbError::TableNotFound("EmployeeHistory".into()));
        let parsed: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["error"]["code"], "table_not_found");
        assert!(parsed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("EmployeeHistory"));
    }
}
