//! Human-readable rendering for interactive terminal use.
//!
//! Layout goals (monochrome — no color): align a record's values into a column,
//! render uniform multi-item results as an aligned table, and give `describe` a
//! grouped, aligned shape. All functions return a `String`; callers decide the
//! stream (data -> stdout, summaries -> stderr).

use std::collections::BTreeSet;

use serde_json::Value;

use crate::dynamodb::{convert, Item, ScalarType, TableSchema};

/// Render table names, one per line.
pub fn render_tables(names: &[String]) -> String {
    names.join("\n")
}

/// Render a single item as an aligned key/value block.
pub fn render_item(item: &Item) -> String {
    render_value(&convert::item_to_json(item), 0)
}

/// Render a list of items.
///
/// When every item has only scalar values, they render as an aligned table with
/// a header (missing fields show `-`). Otherwise they fall back to aligned
/// key/value blocks separated by `---`.
pub fn render_items(items: &[Item]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let objects: Vec<Value> = items.iter().map(convert::item_to_json).collect();
    if table_eligible(&objects) {
        render_table(&objects)
    } else {
        objects
            .iter()
            .map(|o| render_value(o, 0))
            .collect::<Vec<_>>()
            .join("\n---\n")
    }
}

/// Render a table schema in a grouped, aligned summary.
pub fn render_table_schema(schema: &TableSchema) -> String {
    let mut out = String::new();
    out.push_str(&schema.name);
    out.push('\n');

    let mut meta: Vec<(&str, String)> = Vec::new();
    if let Some(status) = &schema.status {
        meta.push(("status", status.clone()));
    }
    if let Some(mode) = &schema.billing_mode {
        meta.push(("billing", mode.clone()));
    }
    if let Some(count) = schema.item_count {
        meta.push(("items", count.to_string()));
    }
    if let Some(size) = schema.size_bytes {
        meta.push(("size", humanize_bytes(size)));
    }
    if !meta.is_empty() {
        out.push('\n');
        let width = meta.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (k, v) in &meta {
            out.push_str(&format!("  {k:<width$}  {v}\n"));
        }
    }

    let kw = "partition".len();
    out.push('\n');
    out.push_str(&format!(
        "  {:<kw$}  {} ({})\n",
        "partition",
        schema.key_schema.partition.name,
        type_code(schema.key_schema.partition.attr_type),
    ));
    if let Some(sort) = &schema.key_schema.sort {
        out.push_str(&format!(
            "  {:<kw$}  {} ({})\n",
            "sort",
            sort.name,
            type_code(sort.attr_type),
        ));
    }

    let mut index_lines = Vec::new();
    for index in schema
        .global_secondary_indexes
        .iter()
        .chain(schema.local_secondary_indexes.iter())
    {
        let sort = index
            .key_schema
            .sort
            .as_ref()
            .map(|s| format!(" / {} ({})", s.name, type_code(s.attr_type)))
            .unwrap_or_default();
        index_lines.push(format!(
            "    {}  ->  {} ({}){}",
            index.name,
            index.key_schema.partition.name,
            type_code(index.key_schema.partition.attr_type),
            sort,
        ));
    }
    if !index_lines.is_empty() {
        out.push_str("\n  indexes\n");
        for line in index_lines {
            out.push_str(&line);
            out.push('\n');
        }
    }

    out.trim_end().to_string()
}

/// True when every item is an object whose values are all scalars (so the set
/// can be rendered as a table).
fn table_eligible(objects: &[Value]) -> bool {
    objects.iter().all(|o| match o {
        Value::Object(map) => map.values().all(is_scalar),
        _ => false,
    })
}

/// Render objects as an aligned table with a header row and rule.
fn render_table(objects: &[Value]) -> String {
    let mut columns: BTreeSet<String> = BTreeSet::new();
    for object in objects {
        if let Value::Object(map) = object {
            for key in map.keys() {
                columns.insert(key.clone());
            }
        }
    }
    let columns: Vec<String> = columns.into_iter().collect();

    let cell = |object: &Value, key: &str| -> String {
        match object {
            Value::Object(map) => map.get(key).map(scalar_str).unwrap_or_else(|| "-".to_string()),
            _ => "-".to_string(),
        }
    };

    let widths: Vec<usize> = columns
        .iter()
        .map(|c| {
            let mut w = c.chars().count();
            for object in objects {
                w = w.max(cell(object, c).chars().count());
            }
            w
        })
        .collect();

    let mut lines = Vec::new();
    lines.push(join_row(&columns, &widths));
    let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    lines.push(join_row(&rule, &widths));
    for object in objects {
        let cells: Vec<String> = columns.iter().map(|c| cell(object, c)).collect();
        lines.push(join_row(&cells, &widths));
    }
    lines.join("\n")
}

/// Join a row of cells padded to their column widths, two spaces between, with
/// trailing whitespace trimmed.
fn join_row(cells: &[String], widths: &[usize]) -> String {
    let mut s = String::new();
    for (i, (c, w)) in cells.iter().zip(widths).enumerate() {
        if i > 0 {
            s.push_str("  ");
        }
        s.push_str(&format!("{c:<w$}"));
    }
    s.trim_end().to_string()
}

/// Recursively render a JSON value in an aligned, YAML-like style.
fn render_value(value: &Value, indent: usize) -> String {
    match value {
        Value::Object(map) => {
            let pad = "  ".repeat(indent);
            let width = map
                .iter()
                .filter(|(_, v)| is_inline(v))
                .map(|(k, _)| k.chars().count())
                .max()
                .unwrap_or(0);
            let mut lines = Vec::new();
            for (k, v) in map {
                if is_inline(v) {
                    lines.push(format!("{pad}{k:<width$}  {}", inline_str(v)));
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
                if is_inline(item) {
                    lines.push(format!("{pad}- {}", inline_str(item)));
                } else {
                    let nested = render_value(item, indent + 1);
                    let mut nested_lines = nested.lines();
                    if let Some(first) = nested_lines.next() {
                        lines.push(format!("{pad}- {}", first.trim_start()));
                        for line in nested_lines {
                            lines.push(line.to_string());
                        }
                    }
                }
            }
            lines.join("\n")
        }
        scalar => format!("{}{}", "  ".repeat(indent), scalar_str(scalar)),
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

/// Values that render on the same line as their key (scalars and empty containers).
fn is_inline(v: &Value) -> bool {
    is_scalar(v) || is_empty_container(v)
}

fn inline_str(v: &Value) -> String {
    match v {
        Value::Array(_) => "[]".to_string(),
        Value::Object(_) => "{}".to_string(),
        scalar => scalar_str(scalar),
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

fn type_code(t: ScalarType) -> &'static str {
    match t {
        ScalarType::String => "S",
        ScalarType::Number => "N",
        ScalarType::Binary => "B",
    }
}

fn humanize_bytes(n: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = n as f64;
    if n < 1024 {
        format!("{n} B")
    } else if f < MB {
        format!("{:.1} KB", f / KB)
    } else if f < GB {
        format!("{:.1} MB", f / MB)
    } else {
        format!("{:.1} GB", f / GB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_dynamodb::types::AttributeValue;
    use std::collections::HashMap;

    fn item(pairs: &[(&str, AttributeValue)]) -> Item {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    #[test]
    fn tables_one_per_line() {
        assert_eq!(
            render_tables(&["Users".into(), "Orders".into()]),
            "Users\nOrders"
        );
    }

    #[test]
    fn flat_item_aligns_values_into_a_column() {
        let it = item(&[
            ("id", AttributeValue::N("12345".into())),
            ("name", AttributeValue::S("Jane Doe".into())),
            ("active", AttributeValue::Bool(true)),
        ]);
        // Keys sorted; values aligned to the widest key ("active" = 6).
        assert_eq!(
            render_item(&it),
            "active  true\nid      12345\nname    Jane Doe"
        );
    }

    #[test]
    fn nested_list_renders_with_dashes() {
        let it = item(&[(
            "roles",
            AttributeValue::Ss(vec!["engineer".into(), "admin".into()]),
        )]);
        assert_eq!(render_item(&it), "roles:\n  - engineer\n  - admin");
    }

    #[test]
    fn nested_map_renders_indented_and_aligned() {
        let mut inner = HashMap::new();
        inner.insert("city".to_string(), AttributeValue::S("NYC".into()));
        let it = item(&[("address", AttributeValue::M(inner))]);
        assert_eq!(render_item(&it), "address:\n  city  NYC");
    }

    #[test]
    fn empty_list_renders_inline() {
        let it = item(&[("tags", AttributeValue::L(vec![]))]);
        assert_eq!(render_item(&it), "tags  []");
    }

    #[test]
    fn uniform_items_render_as_table() {
        let a = item(&[
            ("id", AttributeValue::N("1".into())),
            ("name", AttributeValue::S("Alice".into())),
        ]);
        let b = item(&[
            ("id", AttributeValue::N("2".into())),
            ("name", AttributeValue::S("Bob".into())),
        ]);
        assert_eq!(
            render_items(&[a, b]),
            "id  name\n--  -----\n1   Alice\n2   Bob"
        );
    }

    #[test]
    fn missing_field_shows_dash() {
        let a = item(&[
            ("id", AttributeValue::N("1".into())),
            ("name", AttributeValue::S("Alice".into())),
        ]);
        let b = item(&[("id", AttributeValue::N("2".into()))]);
        assert_eq!(render_items(&[a, b]), "id  name\n--  -----\n1   Alice\n2   -");
    }

    #[test]
    fn non_scalar_items_fall_back_to_blocks() {
        let a = item(&[(
            "roles",
            AttributeValue::Ss(vec!["engineer".into()]),
        )]);
        let b = item(&[(
            "roles",
            AttributeValue::Ss(vec!["admin".into()]),
        )]);
        assert_eq!(
            render_items(&[a, b]),
            "roles:\n  - engineer\n---\nroles:\n  - admin"
        );
    }

    #[test]
    fn empty_items_render_empty() {
        assert_eq!(render_items(&[]), "");
    }

    #[test]
    fn describe_is_grouped_and_aligned() {
        use crate::dynamodb::{KeyDef, KeySchema};
        let schema = TableSchema {
            name: "friends".into(),
            key_schema: KeySchema {
                partition: KeyDef {
                    name: "friends_id".into(),
                    attr_type: ScalarType::String,
                },
                sort: None,
            },
            global_secondary_indexes: vec![],
            local_secondary_indexes: vec![],
            item_count: Some(1),
            size_bytes: Some(84),
            status: Some("ACTIVE".into()),
            billing_mode: Some("PAY_PER_REQUEST".into()),
        };
        let out = render_table_schema(&schema);
        assert!(out.starts_with("friends\n"));
        assert!(out.contains("  status   ACTIVE"));
        assert!(out.contains("  size     84 B"));
        assert!(out.contains("  partition  friends_id (S)"));
    }
}
