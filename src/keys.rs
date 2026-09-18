//! Parsing and typing of key arguments supplied on the command line.
//!
//! Key values can be given either as a bare value (`--pk 12345`) or in an
//! explicit `name=value` form (`--pk employee_id=12345`). The scalar type of
//! each key is taken from the table's (or index's) schema via `DescribeTable`,
//! so a numeric key is sent as a DynamoDB `N`, a binary key is decoded from
//! base64, and so on — values are never blindly assumed to be strings.

use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::AttributeValue;
use base64::Engine as _;

use crate::dynamodb::{KeyDef, ScalarType};
use crate::error::DdbError;

/// Convert a raw string into a typed [`AttributeValue`] for a key attribute.
pub fn typed_value(raw: &str, ty: ScalarType) -> Result<AttributeValue, DdbError> {
    match ty {
        ScalarType::String => Ok(AttributeValue::S(raw.to_string())),
        ScalarType::Number => {
            if is_valid_number(raw) {
                // DynamoDB stores numbers as decimal strings; preserve exactly.
                Ok(AttributeValue::N(raw.to_string()))
            } else {
                Err(DdbError::InvalidUsage(format!(
                    "expected a number value but got '{raw}'"
                )))
            }
        }
        ScalarType::Binary => {
            match base64::engine::general_purpose::STANDARD.decode(raw) {
                Ok(bytes) => Ok(AttributeValue::B(Blob::new(bytes))),
                Err(_) => Err(DdbError::InvalidUsage(format!(
                    "binary key value must be base64-encoded, but '{raw}' is not valid base64"
                ))),
            }
        }
    }
}

/// Resolve a raw `--pk`/`--sk` argument against the expected key definition,
/// handling the optional `name=value` form.
///
/// `key_names` is the set of key attribute names on the target table/index,
/// used to disambiguate an explicit key name from a value that merely contains
/// an `=` character (e.g. base64 padding).
pub fn resolve_key_arg(
    raw: &str,
    expected: &KeyDef,
    key_names: &[&str],
) -> Result<AttributeValue, DdbError> {
    let value_str = match raw.split_once('=') {
        Some((lhs, rhs)) if lhs == expected.name => rhs,
        Some((lhs, _)) if key_names.contains(&lhs) => {
            return Err(DdbError::InvalidUsage(format!(
                "'{lhs}' is not the expected key attribute (expected '{}')",
                expected.name
            )));
        }
        // No name prefix, or the text before '=' is not a key name: treat the
        // entire argument as the value.
        _ => raw,
    };
    typed_value(value_str, expected.attr_type)
}

/// Permissive DynamoDB-number validation: finite decimal (optionally signed,
/// optionally with an exponent).
fn is_valid_number(raw: &str) -> bool {
    if raw.is_empty() {
        return false;
    }
    match raw.parse::<f64>() {
        Ok(f) => f.is_finite(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str, ty: ScalarType) -> KeyDef {
        KeyDef {
            name: name.to_string(),
            attr_type: ty,
        }
    }

    #[test]
    fn bare_string_value() {
        let k = key("id", ScalarType::String);
        let v = resolve_key_arg("hello", &k, &["id"]).unwrap();
        assert_eq!(v, AttributeValue::S("hello".into()));
    }

    #[test]
    fn named_value_matches_key() {
        let k = key("employee_id", ScalarType::Number);
        let v = resolve_key_arg("employee_id=12345", &k, &["employee_id", "timestamp"]).unwrap();
        assert_eq!(v, AttributeValue::N("12345".into()));
    }

    #[test]
    fn wrong_key_name_is_rejected() {
        let k = key("employee_id", ScalarType::Number);
        let err = resolve_key_arg("timestamp=5", &k, &["employee_id", "timestamp"]).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }

    #[test]
    fn value_with_equals_is_kept_when_not_a_key_name() {
        // base64 padding contains '='; the left side is not a key name.
        let k = key("blob", ScalarType::Binary);
        let v = resolve_key_arg("3q2+7w==", &k, &["blob"]).unwrap();
        assert_eq!(v, AttributeValue::B(Blob::new(vec![0xDE, 0xAD, 0xBE, 0xEF])));
    }

    #[test]
    fn non_numeric_number_key_is_rejected() {
        let k = key("age", ScalarType::Number);
        let err = resolve_key_arg("abc", &k, &["age"]).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }

    #[test]
    fn negative_and_exponent_numbers_accepted() {
        let k = key("n", ScalarType::Number);
        assert!(resolve_key_arg("-42", &k, &["n"]).is_ok());
        assert!(resolve_key_arg("1.5e3", &k, &["n"]).is_ok());
    }

    #[test]
    fn invalid_base64_binary_rejected() {
        let k = key("b", ScalarType::Binary);
        let err = resolve_key_arg("not base64!!!", &k, &["b"]).unwrap_err();
        assert!(matches!(err, DdbError::InvalidUsage(_)));
    }
}
