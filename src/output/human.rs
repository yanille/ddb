//! Human-readable rendering for interactive terminal use.
//!
//! Items are printed in a compact, YAML-like form. All functions return a
//! `String`; callers decide the stream (data -> stdout, summaries -> stderr).

use serde_json::Value;

use crate::dynamodb::{convert, Item, TableSchema};

/// Render table names, one per line.
pub fn render_tables(names: &[String]) -> String {
    names.join("\n")
}

/// Render a single item as an indented key/value block.
pub fn render_item(item: &Item) -> String {
    render_value(&convert::item_to_json(item), 0)
}

/// Render a list of items, separated by `---` rules.
pub fn render_items(items: &[Item]) -> String {
    items
        .iter()
        .map(|item| render_value(&convert::item_to_json(item), 0))
        .collect::<Vec<_>>()
        .join("\n---\n")
}

/// Render a table schema in a readable summary form.
pub fn render_table_schema(schema: &TableSchema) -> String {
    let mut out = String::new();
    out.push_str(&format!("table: {}\n", schema.name));
    if let Some(status) = &schema.status {
        out.push_str(&format!("status: {status}\n"));
    }
    if let Some(mode) = &schema.billing_mode {
        out.push_str(&format!("billing_mode: {mode}\n"));
    }
    if let Some(count) = schema.item_count {
        out.push_str(&format!("item_count: {count}\n"));
    }
    if let Some(size) = schema.size_bytes {
        out.push_str(&format!("size_bytes: {size}\n"));
    }
    out.push_str("key_schema:\n");
    out.push_str(&format!(
        "  partition_key: {}\n",
        schema.key_schema.partition.name
    ));
    if let Some(sort) = &schema.key_schema.sort {
        out.push_str(&format!("  sort_key: {}\n", sort.name));
    }
    render_index_block(&mut out, "global_secondary_indexes", &schema.global_secondary_indexes);
    render_index_block(&mut out, "local_secondary_indexes", &schema.local_secondary_indexes);
    // Trim trailing newline for consistency with other renderers.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn render_index_block(
    out: &mut String,
    label: &str,
    indexes: &[crate::dynamodb::SecondaryIndex],
) {
    if indexes.is_empty() {
        return;
    }
    out.push_str(&format!("{label}:\n"));
    for idx in indexes {
        out.push_str(&format!("  - name: {}\n", idx.name));
        out.push_str(&format!(
            "    partition_key: {}\n",
            idx.key_schema.partition.name
        ));
        if let Some(sort) = &idx.key_schema.sort {
            out.push_str(&format!("    sort_key: {}\n", sort.name));
        }
    }
}

/// Recursively render a JSON value in a YAML-like style at the given indent
/// (in levels of two spaces).
fn render_value(value: &Value, indent: usize) -> String {
    match value {
        Value::Object(map) => {
            let pad = "  ".repeat(indent);
            let mut lines = Vec::new();
            for (k, v) in map {
                if is_scalar(v) {
                    lines.push(format!("{pad}{k}: {}", scalar_str(v)));
                } else if is_empty_container(v) {
                    lines.push(format!("{pad}{k}: {}", empty_container_str(v)));
                } else {
                    lines.push(format!("{pad}{k}:"));
                    lines.push(render_value(v, indent + 1));
                }
            }
            lines.join("\n")
        }
        Value::Array(items) => {
            let pad = "  ".repeat(indent);
            let mut lines = Vec::new();
            for item in items {
                if is_scalar(item) {
                    lines.push(format!("{pad}- {}", scalar_str(item)));
                } else if is_empty_container(item) {
                    lines.push(format!("{pad}- {}", empty_container_str(item)));
                } else {
                    // Render the nested container indented under the dash.
                    let nested = render_value(item, indent + 1);
                    // Replace the first indent of the nested block with "- ".
                    let mut nested_lines = nested.lines();
                    if let Some(first) = nested_lines.next() {
                        let trimmed = first.trim_start();
                        lines.push(format!("{pad}- {trimmed}"));
                        for line in nested_lines {
                            lines.push(line.to_string());
                        }
                    }
                }
            }
            lines.join("\n")
        }
        scalar => {
            let pad = "  ".repeat(indent);
            format!("{pad}{}", scalar_str(scalar))
        }
    }
}

fn is_scalar(v: &Value) -> bool {
    matches!(
        v,
        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
    )
}

fn is_empty_container(v: &Value) -> bool {
    matches!(v, Value::Array(a) if a.is_empty()) || matches!(v, Value::Object(o) if o.is_empty())
}

fn empty_container_str(v: &Value) -> &'static str {
    match v {
        Value::Array(_) => "[]",
        Value::Object(_) => "{}",
        _ => "",
    }
}

fn scalar_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_dynamodb::types::AttributeValue;
    use std::collections::HashMap;

    #[test]
    fn tables_one_per_line() {
        assert_eq!(
            render_tables(&["Users".into(), "Orders".into()]),
            "Users\nOrders"
        );
    }

    #[test]
    fn flat_item_renders_key_values() {
        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert("id".into(), AttributeValue::N("12345".into()));
        item.insert("name".into(), AttributeValue::S("Jane Doe".into()));
        item.insert("active".into(), AttributeValue::Bool(true));
        let out = render_item(&item);
        // Keys are sorted deterministically.
        assert_eq!(out, "active: true\nid: 12345\nname: Jane Doe");
    }

    #[test]
    fn nested_list_renders_with_dashes() {
        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert(
            "roles".into(),
            AttributeValue::Ss(vec!["engineer".into(), "admin".into()]),
        );
        let out = render_item(&item);
        assert_eq!(out, "roles:\n  - engineer\n  - admin");
    }

    #[test]
    fn nested_map_renders_indented() {
        let mut inner: HashMap<String, AttributeValue> = HashMap::new();
        inner.insert("city".into(), AttributeValue::S("NYC".into()));
        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert("address".into(), AttributeValue::M(inner));
        let out = render_item(&item);
        assert_eq!(out, "address:\n  city: NYC");
    }

    #[test]
    fn empty_list_renders_inline() {
        let mut item: HashMap<String, AttributeValue> = HashMap::new();
        item.insert("tags".into(), AttributeValue::L(vec![]));
        let out = render_item(&item);
        assert_eq!(out, "tags: []");
    }

    #[test]
    fn multiple_items_separated_by_rule() {
        let mut a: HashMap<String, AttributeValue> = HashMap::new();
        a.insert("id".into(), AttributeValue::N("1".into()));
        let mut b: HashMap<String, AttributeValue> = HashMap::new();
        b.insert("id".into(), AttributeValue::N("2".into()));
        let out = render_items(&[a, b]);
        assert_eq!(out, "id: 1\n---\nid: 2");
    }
}
