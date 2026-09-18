//! `ddb` — a fast, terminal-native, strictly **read-only** DynamoDB inspection
//! library backing the `ddb` CLI.
//!
//! # Read-only safety guarantee
//!
//! All DynamoDB access flows through the [`dynamodb::DynamoDbReader`] trait,
//! whose methods are exclusively reads. Command handlers are generic over that
//! trait and cannot reach a mutating API. The AWS-backed implementation in
//! [`dynamodb::client`] calls only `list_tables`, `describe_table`, `get_item`,
//! `query`, and `scan`. The `tests/no_write_path.rs` integration test enforces
//! that no mutating DynamoDB operation appears anywhere in `src/`.

pub mod cli;
pub mod commands;
pub mod dynamodb;
pub mod error;
pub mod keys;
pub mod output;
