//! DynamoDB access layer.
//!
//! The **only** application-facing entry point to DynamoDB is the
//! [`DynamoDbReader`] trait. By construction it exposes read operations only —
//! there is no method that can create, update, or delete DynamoDB data or
//! configuration. The concrete AWS-backed implementation lives in
//! [`client`] and calls exclusively read APIs of `aws-sdk-dynamodb`.
//!
//! This is the load-bearing safety boundary of the tool: commands are generic
//! over `R: DynamoDbReader` and therefore *cannot* reach a write API even if a
//! future command handler tried to.

pub mod client;
pub mod convert;

use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;

use crate::error::DdbError;

/// A DynamoDB item: attribute name -> value, in native DynamoDB representation.
pub type Item = HashMap<String, AttributeValue>;

/// The scalar type of a key attribute. DynamoDB key attributes are always one
/// of these three types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    /// String (`S`).
    String,
    /// Number (`N`).
    Number,
    /// Binary (`B`).
    Binary,
}

impl ScalarType {
    pub fn from_dynamo(code: &str) -> Option<ScalarType> {
        match code {
            "S" => Some(ScalarType::String),
            "N" => Some(ScalarType::Number),
            "B" => Some(ScalarType::Binary),
            _ => None,
        }
    }
}

/// A single key attribute definition (name + scalar type).
#[derive(Debug, Clone)]
pub struct KeyDef {
    pub name: String,
    pub attr_type: ScalarType,
}

/// A partition + optional sort key schema.
#[derive(Debug, Clone)]
pub struct KeySchema {
    pub partition: KeyDef,
    pub sort: Option<KeyDef>,
}

impl KeySchema {
    /// Names of all key attributes in this schema.
    pub fn attribute_names(&self) -> Vec<&str> {
        let mut names = vec![self.partition.name.as_str()];
        if let Some(sort) = &self.sort {
            names.push(sort.name.as_str());
        }
        names
    }
}

/// A secondary index (GSI or LSI) with its own key schema.
#[derive(Debug, Clone)]
pub struct SecondaryIndex {
    pub name: String,
    pub key_schema: KeySchema,
}

/// A resolved table schema, derived from `DescribeTable`.
#[derive(Debug, Clone)]
pub struct TableSchema {
    pub name: String,
    pub key_schema: KeySchema,
    pub global_secondary_indexes: Vec<SecondaryIndex>,
    pub local_secondary_indexes: Vec<SecondaryIndex>,
    pub item_count: Option<i64>,
    pub size_bytes: Option<i64>,
    pub status: Option<String>,
    pub billing_mode: Option<String>,
}

impl TableSchema {
    /// Resolve the key schema for a query/scan target: the base table when
    /// `index` is `None`, otherwise the named secondary index.
    pub fn key_schema_for(&self, index: Option<&str>) -> Result<&KeySchema, DdbError> {
        match index {
            None => Ok(&self.key_schema),
            Some(name) => self
                .global_secondary_indexes
                .iter()
                .chain(self.local_secondary_indexes.iter())
                .find(|i| i.name == name)
                .map(|i| &i.key_schema)
                .ok_or_else(|| DdbError::IndexNotFound {
                    table: self.name.clone(),
                    index: name.to_string(),
                }),
        }
    }
}

/// Request for a single-item lookup by primary key.
#[derive(Debug, Clone)]
pub struct GetItemRequest {
    pub table: String,
    /// Fully-built key: attribute name -> typed value.
    pub key: Item,
}

/// A sort-key condition for a query.
#[derive(Debug, Clone)]
pub enum SortKeyCondition {
    Equals { name: String, value: AttributeValue },
    BeginsWith { name: String, value: AttributeValue },
}

/// Request for a partition-key query, with an optional sort-key condition.
#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub table: String,
    pub index: Option<String>,
    pub partition: (String, AttributeValue),
    pub sort: Option<SortKeyCondition>,
    /// Per-page item cap passed to DynamoDB.
    pub limit: Option<i32>,
    /// Maximum number of result pages to fetch before stopping.
    pub max_pages: usize,
}

/// Request for a table/index scan.
#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub table: String,
    pub index: Option<String>,
    pub limit: Option<i32>,
    pub max_pages: usize,
}

/// The result of a paginated read (query or scan).
#[derive(Debug, Default, Clone)]
pub struct Page {
    pub items: Vec<Item>,
    /// Number of items actually evaluated by DynamoDB (scanned count).
    pub scanned_count: i64,
    /// True if more results remained but pagination stopped due to `max_pages`.
    pub truncated: bool,
}

/// Read-only DynamoDB interface. **Every** method is a read; there is
/// intentionally no way to mutate DynamoDB through this trait.
#[allow(async_fn_in_trait)]
pub trait DynamoDbReader {
    /// List all accessible table names (following pagination).
    async fn list_tables(&self) -> Result<Vec<String>, DdbError>;

    /// Describe a table's schema and metadata.
    async fn describe_table(&self, table: &str) -> Result<TableSchema, DdbError>;

    /// Get a single item by its fully-specified primary key.
    async fn get_item(&self, request: GetItemRequest) -> Result<Option<Item>, DdbError>;

    /// Query items by partition key (and optional sort-key condition).
    async fn query(&self, request: QueryRequest) -> Result<Page, DdbError>;

    /// Scan items from a table or index.
    async fn scan(&self, request: ScanRequest) -> Result<Page, DdbError>;
}
