//! `ddb query <table> --pk <value> [--sk <value> | --sk-begins-with <prefix>]`.

use crate::dynamodb::{
    DynamoDbReader, KeySchema, QueryRequest, ScalarType, SortKeyCondition,
};
use crate::error::DdbError;
use crate::keys::resolve_key_arg;
use crate::output::{human, json, OutputFormat};

use super::Rendered;

/// Arguments for the query command (grouped to keep the dispatcher readable).
pub struct QueryArgs<'a> {
    pub table: &'a str,
    pub pk: &'a str,
    pub sk: Option<&'a str>,
    pub sk_begins_with: Option<&'a str>,
    pub index: Option<&'a str>,
    pub limit: Option<i32>,
    pub max_pages: usize,
}

pub async fn run<R: DynamoDbReader>(
    reader: &R,
    args: QueryArgs<'_>,
    output: OutputFormat,
) -> Result<Rendered, DdbError> {
    let schema = reader.describe_table(args.table).await?;
    let key_schema = schema.key_schema_for(args.index)?.clone();

    let request = build_query_request(&args, &key_schema)?;
    let page = reader.query(request).await?;

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
                "results truncated at {} page(s); raise --max-pages to read more",
                args.max_pages
            ));
        }
    }
    Ok(rendered)
}

fn build_query_request(
    args: &QueryArgs<'_>,
    key_schema: &KeySchema,
) -> Result<QueryRequest, DdbError> {
    let key_names = key_schema.attribute_names();
    let pk_value = resolve_key_arg(args.pk, &key_schema.partition, &key_names)?;

    let sort = match (args.sk, args.sk_begins_with) {
        (Some(_), _) | (_, Some(_)) if key_schema.sort.is_none() => {
            return Err(DdbError::InvalidUsage(
                "the target has no sort key; --sk/--sk-begins-with is not applicable".to_string(),
            ));
        }
        (Some(sk_raw), None) => {
            let sort_def = key_schema.sort.as_ref().expect("checked above");
            let value = resolve_key_arg(sk_raw, sort_def, &key_names)?;
            Some(SortKeyCondition::Equals {
                name: sort_def.name.clone(),
                value,
            })
        }
        (None, Some(prefix)) => {
            let sort_def = key_schema.sort.as_ref().expect("checked above");
            if sort_def.attr_type == ScalarType::Number {
                return Err(DdbError::InvalidUsage(
                    "--sk-begins-with is only valid for string or binary sort keys".to_string(),
                ));
            }
            let value = resolve_key_arg(prefix, sort_def, &key_names)?;
            Some(SortKeyCondition::BeginsWith {
                name: sort_def.name.clone(),
                value,
            })
        }
        (None, None) => None,
        // clap enforces mutual exclusivity; defensive guard for library reuse.
        (Some(_), Some(_)) => {
            return Err(DdbError::InvalidUsage(
                "--sk and --sk-begins-with are mutually exclusive".to_string(),
            ));
        }
    };

    Ok(QueryRequest {
        table: args.table.to_string(),
        index: args.index.map(|s| s.to_string()),
        partition: (key_schema.partition.name.clone(), pk_value),
        sort,
        limit: args.limit,
        max_pages: args.max_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamodb::KeyDef;
    use aws_sdk_dynamodb::types::AttributeValue;

    fn composite_schema() -> KeySchema {
        KeySchema {
            partition: KeyDef {
                name: "pk".into(),
                attr_type: ScalarType::String,
            },
            sort: Some(KeyDef {
                name: "sk".into(),
                attr_type: ScalarType::String,
            }),
        }
    }

    fn args<'a>(pk: &'a str) -> QueryArgs<'a> {
        QueryArgs {
            table: "T",
            pk,
            sk: None,
            sk_begins_with: None,
            index: None,
            limit: None,
            max_pages: 10,
        }
    }

    #[test]
    fn partition_only_query() {
        let req = build_query_request(&args("v"), &composite_schema()).unwrap();
        assert_eq!(req.partition.0, "pk");
        assert_eq!(req.partition.1, AttributeValue::S("v".into()));
        assert!(req.sort.is_none());
    }

    #[test]
    fn equals_sort_condition() {
        let mut a = args("v");
        a.sk = Some("x");
        let req = build_query_request(&a, &composite_schema()).unwrap();
        assert!(matches!(req.sort, Some(SortKeyCondition::Equals { .. })));
    }

    #[test]
    fn begins_with_condition() {
        let mut a = args("v");
        a.sk_begins_with = Some("2026-");
        let req = build_query_request(&a, &composite_schema()).unwrap();
        assert!(matches!(req.sort, Some(SortKeyCondition::BeginsWith { .. })));
    }

    #[test]
    fn begins_with_rejected_for_number_sort_key() {
        let schema = KeySchema {
            partition: KeyDef {
                name: "pk".into(),
                attr_type: ScalarType::String,
            },
            sort: Some(KeyDef {
                name: "sk".into(),
                attr_type: ScalarType::Number,
            }),
        };
        let mut a = args("v");
        a.sk_begins_with = Some("1");
        let err = build_query_request(&a, &schema).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }

    #[test]
    fn sort_condition_rejected_without_sort_key() {
        let schema = KeySchema {
            partition: KeyDef {
                name: "pk".into(),
                attr_type: ScalarType::String,
            },
            sort: None,
        };
        let mut a = args("v");
        a.sk = Some("x");
        let err = build_query_request(&a, &schema).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }
}
