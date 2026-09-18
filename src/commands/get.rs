//! `ddb get <table> --pk <value> [--sk <value>]` — single-item lookup.

use std::collections::HashMap;

use crate::dynamodb::{DynamoDbReader, GetItemRequest, Item, KeySchema};
use crate::error::DdbError;
use crate::keys::resolve_key_arg;
use crate::output::{human, json, OutputFormat};

use super::Rendered;

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    table: &str,
    pk: &str,
    sk: Option<&str>,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    let schema = reader.describe_table(table).await?;
    let key = build_primary_key(&schema.key_schema, pk, sk)?;

    let item = reader
        .get_item(GetItemRequest {
            table: table.to_string(),
            key,
        })
        .await?;

    match (output.is_json(), item) {
        (true, item) => Ok(Rendered::stdout(json::render_item(item.as_ref()))),
        (false, Some(item)) => Ok(Rendered::stdout(human::render_item(&item))),
        (false, None) => Ok(Rendered::stdout("").with_stderr("item not found")),
    }
}

/// Build the full primary key map for a `GetItem`, validating that the supplied
/// keys match the table's key schema.
fn build_primary_key(
    schema: &KeySchema,
    pk: &str,
    sk: Option<&str>,
) -> Result<Item, DdbError> {
    let key_names = schema.attribute_names();
    let mut key: Item = HashMap::new();

    let pk_value = resolve_key_arg(pk, &schema.partition, &key_names)?;
    key.insert(schema.partition.name.clone(), pk_value);

    match (&schema.sort, sk) {
        (Some(sort_def), Some(sk_raw)) => {
            let sk_value = resolve_key_arg(sk_raw, sort_def, &key_names)?;
            key.insert(sort_def.name.clone(), sk_value);
        }
        (Some(sort_def), None) => {
            return Err(DdbError::InvalidUsage(format!(
                "table has a composite primary key; --sk ({}) is required",
                sort_def.name
            )));
        }
        (None, Some(_)) => {
            return Err(DdbError::InvalidUsage(
                "table has a simple primary key; --sk is not applicable".to_string(),
            ));
        }
        (None, None) => {}
    }

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamodb::{KeyDef, ScalarType};
    use aws_sdk_dynamodb::types::AttributeValue;

    fn simple_schema() -> KeySchema {
        KeySchema {
            partition: KeyDef {
                name: "id".into(),
                attr_type: ScalarType::Number,
            },
            sort: None,
        }
    }

    fn composite_schema() -> KeySchema {
        KeySchema {
            partition: KeyDef {
                name: "employee_id".into(),
                attr_type: ScalarType::Number,
            },
            sort: Some(KeyDef {
                name: "timestamp".into(),
                attr_type: ScalarType::String,
            }),
        }
    }

    #[test]
    fn builds_simple_key() {
        let key = build_primary_key(&simple_schema(), "12345", None).unwrap();
        assert_eq!(key.get("id"), Some(&AttributeValue::N("12345".into())));
    }

    #[test]
    fn builds_composite_key() {
        let key =
            build_primary_key(&composite_schema(), "12345", Some("2026-09-17")).unwrap();
        assert_eq!(
            key.get("employee_id"),
            Some(&AttributeValue::N("12345".into()))
        );
        assert_eq!(
            key.get("timestamp"),
            Some(&AttributeValue::S("2026-09-17".into()))
        );
    }

    #[test]
    fn composite_key_requires_sort() {
        let err = build_primary_key(&composite_schema(), "12345", None).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }

    #[test]
    fn simple_key_rejects_sort() {
        let err = build_primary_key(&simple_schema(), "1", Some("x")).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }
}
