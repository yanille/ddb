//! AWS-backed implementation of [`DynamoDbReader`].
//!
//! SAFETY: This module calls **only** read APIs of `aws-sdk-dynamodb`:
//! `list_tables`, `describe_table`, `get_item`, `query`, and `scan`. It never
//! constructs or invokes any mutating operation. The `tests/no_write_path.rs`
//! integration test enforces this by scanning the source for forbidden calls.

use std::collections::HashMap;

use aws_sdk_dynamodb::config::Region;
use aws_sdk_dynamodb::types::AttributeValue;
use aws_sdk_dynamodb::Client;

use crate::error::{map_sdk_error, DdbError};

use super::{
    GetItemRequest, Item, KeyDef, KeySchema, Page, QueryRequest, ScalarType, ScanRequest,
    SecondaryIndex, SortKeyCondition, TableSchema,
};

/// AWS-backed, read-only DynamoDB client.
pub struct AwsDynamoDbClient {
    inner: Client,
}

impl AwsDynamoDbClient {
    /// Build a client from the standard AWS configuration chain, optionally
    /// overriding the profile and region.
    pub async fn from_env(profile: Option<&str>, region: Option<&str>) -> Self {
        let config = load_config(profile, region).await;
        AwsDynamoDbClient {
            inner: Client::new(&config),
        }
    }

    /// Construct directly from an existing SDK client (used in integration tests
    /// against DynamoDB Local).
    pub fn from_client(inner: Client) -> Self {
        AwsDynamoDbClient { inner }
    }
}

/// Load AWS configuration from the standard credential/configuration chain.
///
/// This uses `aws_config::defaults`, which resolves — in the usual precedence —
/// from environment variables, the shared **credentials** file
/// (`~/.aws/credentials`, or `$AWS_SHARED_CREDENTIALS_FILE`), the shared
/// **config** file (`~/.aws/config`, or `$AWS_CONFIG_FILE`), AWS SSO / IAM
/// Identity Center, and IAM role / container / EC2 metadata providers. The
/// shared files are therefore always honored; `ddb` does not implement its own
/// credential storage. An explicit `--profile` / `--region` overrides only the
/// profile selection and region, not which sources are consulted.
pub async fn load_config(profile: Option<&str>, region: Option<&str>) -> aws_config::SdkConfig {
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(profile) = profile {
        loader = loader.profile_name(profile);
    }
    if let Some(region) = region {
        loader = loader.region(Region::new(region.to_string()));
    }
    loader.load().await
}

impl super::DynamoDbReader for AwsDynamoDbClient {
    async fn list_tables(&self) -> Result<Vec<String>, DdbError> {
        let mut names = Vec::new();
        let mut start: Option<String> = None;
        loop {
            let output = self
                .inner
                .list_tables()
                .set_exclusive_start_table_name(start.take())
                .send()
                .await
                .map_err(|e| map_sdk_error(None, e))?;
            names.extend(output.table_names().iter().cloned());
            match output.last_evaluated_table_name() {
                Some(next) => start = Some(next.to_string()),
                None => break,
            }
        }
        Ok(names)
    }

    async fn describe_table(&self, table: &str) -> Result<TableSchema, DdbError> {
        let output = self
            .inner
            .describe_table()
            .table_name(table)
            .send()
            .await
            .map_err(|e| map_sdk_error(Some(table), e))?;

        let desc = output
            .table()
            .ok_or_else(|| DdbError::TableNotFound(table.to_string()))?;

        // Build a lookup of attribute name -> scalar type from AttributeDefinitions.
        let mut attr_types: HashMap<&str, ScalarType> = HashMap::new();
        for def in desc.attribute_definitions() {
            if let Some(t) = ScalarType::from_dynamo(def.attribute_type().as_str()) {
                attr_types.insert(def.attribute_name(), t);
            }
        }

        let key_schema = build_key_schema(desc.key_schema(), &attr_types).ok_or_else(|| {
            DdbError::Service(format!("table '{table}' returned an incomplete key schema"))
        })?;

        let global_secondary_indexes = desc
            .global_secondary_indexes()
            .iter()
            .filter_map(|gsi| {
                let name = gsi.index_name()?.to_string();
                let schema = build_key_schema(gsi.key_schema(), &attr_types)?;
                Some(SecondaryIndex {
                    name,
                    key_schema: schema,
                })
            })
            .collect();

        let local_secondary_indexes = desc
            .local_secondary_indexes()
            .iter()
            .filter_map(|lsi| {
                let name = lsi.index_name()?.to_string();
                let schema = build_key_schema(lsi.key_schema(), &attr_types)?;
                Some(SecondaryIndex {
                    name,
                    key_schema: schema,
                })
            })
            .collect();

        Ok(TableSchema {
            name: table.to_string(),
            key_schema,
            global_secondary_indexes,
            local_secondary_indexes,
            item_count: desc.item_count(),
            size_bytes: desc.table_size_bytes(),
            status: desc.table_status().map(|s| s.as_str().to_string()),
            billing_mode: desc
                .billing_mode_summary()
                .and_then(|b| b.billing_mode())
                .map(|m| m.as_str().to_string()),
        })
    }

    async fn get_item(&self, request: GetItemRequest) -> Result<Option<Item>, DdbError> {
        let output = self
            .inner
            .get_item()
            .table_name(&request.table)
            .set_key(Some(request.key))
            .send()
            .await
            .map_err(|e| map_sdk_error(Some(&request.table), e))?;
        Ok(output.item().cloned())
    }

    async fn query(&self, request: QueryRequest) -> Result<Page, DdbError> {
        let (expr, names, values) = build_key_condition(&request);

        let mut page = Page::default();
        let mut start: Option<Item> = None;
        let mut pages_fetched = 0usize;

        loop {
            let mut builder = self
                .inner
                .query()
                .table_name(&request.table)
                .key_condition_expression(&expr)
                .set_expression_attribute_names(Some(names.clone()))
                .set_expression_attribute_values(Some(values.clone()))
                .set_exclusive_start_key(start.take());
            if let Some(index) = &request.index {
                builder = builder.index_name(index);
            }
            if let Some(limit) = request.limit {
                builder = builder.limit(limit);
            }

            let output = builder
                .send()
                .await
                .map_err(|e| map_sdk_error(Some(&request.table), e))?;

            page.items.extend(output.items().iter().cloned());
            page.scanned_count += i64::from(output.scanned_count());
            pages_fetched += 1;

            match output.last_evaluated_key() {
                Some(key) if pages_fetched < request.max_pages => {
                    start = Some(key.clone());
                }
                Some(_) => {
                    page.truncated = true;
                    break;
                }
                None => break,
            }
        }
        Ok(page)
    }

    async fn scan(&self, request: ScanRequest) -> Result<Page, DdbError> {
        let mut page = Page::default();
        let mut start: Option<Item> = None;
        let mut pages_fetched = 0usize;

        loop {
            let mut builder = self
                .inner
                .scan()
                .table_name(&request.table)
                .set_exclusive_start_key(start.take());
            if let Some(index) = &request.index {
                builder = builder.index_name(index);
            }
            if let Some(limit) = request.limit {
                builder = builder.limit(limit);
            }

            let output = builder
                .send()
                .await
                .map_err(|e| map_sdk_error(Some(&request.table), e))?;

            page.items.extend(output.items().iter().cloned());
            page.scanned_count += i64::from(output.scanned_count());
            pages_fetched += 1;

            match output.last_evaluated_key() {
                Some(key) if pages_fetched < request.max_pages => {
                    start = Some(key.clone());
                }
                Some(_) => {
                    page.truncated = true;
                    break;
                }
                None => break,
            }
        }
        Ok(page)
    }
}

/// Build a [`KeySchema`] from an SDK key-schema slice + attribute-type lookup.
fn build_key_schema(
    elements: &[aws_sdk_dynamodb::types::KeySchemaElement],
    attr_types: &HashMap<&str, ScalarType>,
) -> Option<KeySchema> {
    use aws_sdk_dynamodb::types::KeyType;
    let mut partition: Option<KeyDef> = None;
    let mut sort: Option<KeyDef> = None;
    for element in elements {
        let name = element.attribute_name();
        let attr_type = *attr_types.get(name)?;
        let def = KeyDef {
            name: name.to_string(),
            attr_type,
        };
        match element.key_type() {
            KeyType::Hash => partition = Some(def),
            KeyType::Range => sort = Some(def),
            _ => {}
        }
    }
    Some(KeySchema {
        partition: partition?,
        sort,
    })
}

/// Build the KeyConditionExpression and expression attribute name/value maps for
/// a query, using placeholders to avoid reserved-word collisions.
fn build_key_condition(
    request: &QueryRequest,
) -> (
    String,
    HashMap<String, String>,
    HashMap<String, AttributeValue>,
) {
    let mut names = HashMap::new();
    let mut values = HashMap::new();

    let (pk_name, pk_value) = &request.partition;
    names.insert("#pk".to_string(), pk_name.clone());
    values.insert(":pk".to_string(), pk_value.clone());
    let mut expr = "#pk = :pk".to_string();

    if let Some(sort) = &request.sort {
        match sort {
            SortKeyCondition::Equals { name, value } => {
                names.insert("#sk".to_string(), name.clone());
                values.insert(":sk".to_string(), value.clone());
                expr.push_str(" AND #sk = :sk");
            }
            SortKeyCondition::BeginsWith { name, value } => {
                names.insert("#sk".to_string(), name.clone());
                values.insert(":sk".to_string(), value.clone());
                expr.push_str(" AND begins_with(#sk, :sk)");
            }
        }
    }

    (expr, names, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_condition_partition_only() {
        let req = QueryRequest {
            table: "T".into(),
            index: None,
            partition: ("pk".into(), AttributeValue::S("v".into())),
            sort: None,
            limit: None,
            max_pages: 1,
        };
        let (expr, names, values) = build_key_condition(&req);
        assert_eq!(expr, "#pk = :pk");
        assert_eq!(names.get("#pk").unwrap(), "pk");
        assert!(values.contains_key(":pk"));
        assert!(!values.contains_key(":sk"));
    }

    #[test]
    fn key_condition_with_sort_equals() {
        let req = QueryRequest {
            table: "T".into(),
            index: None,
            partition: ("pk".into(), AttributeValue::S("v".into())),
            sort: Some(SortKeyCondition::Equals {
                name: "sk".into(),
                value: AttributeValue::N("1".into()),
            }),
            limit: None,
            max_pages: 1,
        };
        let (expr, names, _values) = build_key_condition(&req);
        assert_eq!(expr, "#pk = :pk AND #sk = :sk");
        assert_eq!(names.get("#sk").unwrap(), "sk");
    }

    #[test]
    fn key_condition_with_begins_with() {
        let req = QueryRequest {
            table: "T".into(),
            index: None,
            partition: ("pk".into(), AttributeValue::S("v".into())),
            sort: Some(SortKeyCondition::BeginsWith {
                name: "sk".into(),
                value: AttributeValue::S("2026-".into()),
            }),
            limit: None,
            max_pages: 1,
        };
        let (expr, _names, _values) = build_key_condition(&req);
        assert_eq!(expr, "#pk = :pk AND begins_with(#sk, :sk)");
    }
}
