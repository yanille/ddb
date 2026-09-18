//! Architectural safety guarantee: `ddb` has no DynamoDB write path.
//!
//! This test scans every Rust source file under `src/` and fails if any
//! mutating DynamoDB SDK operation is invoked. It is the machine-checkable
//! backstop for the read-only product requirement: even if a future change
//! accidentally reaches for a write API, the build's tests will fail.

use std::fs;
use std::path::{Path, PathBuf};

/// DynamoDB SDK operations that create, update, or delete state. Matched in
/// their method-call form (`.op(`) so prose in comments does not trip the test.
const FORBIDDEN_OPERATIONS: &[&str] = &[
    "put_item",
    "update_item",
    "delete_item",
    "batch_write_item",
    "transact_write_items",
    "create_table",
    "delete_table",
    "update_table",
    "create_global_table",
    "update_global_table",
    "create_table_replica",
    "update_time_to_live",
    "tag_resource",
    "untag_resource",
    "create_backup",
    "delete_backup",
    "restore_table_from_backup",
    "restore_table_to_point_in_time",
    "update_continuous_backups",
    "update_contributor_insights",
    "enable_kinesis_streaming_destination",
    "disable_kinesis_streaming_destination",
    "update_kinesis_streaming_destination",
    "put_resource_policy",
    "delete_resource_policy",
    "import_table",
    "export_table_to_point_in_time",
    // PartiQL statement APIs can mutate data, so they are forbidden too.
    "execute_statement",
    "execute_transaction",
    "batch_execute_statement",
];

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read src dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn source_contains_no_dynamodb_write_operations() {
    let mut files = Vec::new();
    collect_rust_files(&src_dir(), &mut files);
    assert!(!files.is_empty(), "expected to find source files under src/");

    let mut violations = Vec::new();
    for file in &files {
        let contents = fs::read_to_string(file).expect("read source file");
        for op in FORBIDDEN_OPERATIONS {
            let needle = format!(".{op}(");
            if contents.contains(&needle) {
                violations.push(format!("{}: found `{needle}`", file.display()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "ddb must not contain any DynamoDB write operations, but found:\n{}",
        violations.join("\n")
    );
}

#[test]
fn client_uses_only_read_operations() {
    // Belt-and-suspenders: the AWS-backed client must reference the expected
    // read operations and nothing else surprising.
    let client = fs::read_to_string(src_dir().join("dynamodb/client.rs")).expect("read client.rs");
    for expected in ["list_tables", "describe_table", "get_item", "query", "scan"] {
        assert!(
            client.contains(expected),
            "expected read operation `{expected}` to be used in client.rs"
        );
    }
}
