//! Conversion from DynamoDB's native `AttributeValue` representation into
//! `serde_json::Value`, used by every output formatter.
//!
//! Conversion rules (see `docs/decisions.md` for rationale):
//!
//! | DynamoDB | JSON |
//! |----------|------|
//! | `S`      | string |
//! | `N`      | number when losslessly representable, otherwise string |
//! | `BOOL`   | boolean |
//! | `NULL`   | null |
//! | `M`      | object |
//! | `L`      | array |
//! | `SS`     | array of strings |
//! | `NS`     | array of numbers/strings (same rule as `N`) |
//! | `B`      | base64 string |
//! | `BS`     | array of base64 strings |
//!
//! The conversion is total and never panics. It never emits arbitrary
//! (possibly invalid UTF-8) bytes: binary is always base64-encoded.

use std::collections::BTreeMap;

use aws_sdk_dynamodb::types::AttributeValue;
use base64::Engine as _;
use serde_json::{Map, Value};

/// Convert a single `AttributeValue` into a `serde_json::Value`.
pub fn attribute_to_json(value: &AttributeValue) -> Value {
    match value {
        AttributeValue::S(s) => Value::String(s.clone()),
        AttributeValue::N(n) => number_to_json(n),
        AttributeValue::Bool(b) => Value::Bool(*b),
        AttributeValue::Null(_) => Value::Null,
        AttributeValue::M(map) => {
            // BTreeMap gives deterministic (sorted) key ordering in output.
            let sorted: BTreeMap<&String, &AttributeValue> = map.iter().collect();
            let mut obj = Map::with_capacity(map.len());
            for (k, v) in sorted {
                obj.insert(k.clone(), attribute_to_json(v));
            }
            Value::Object(obj)
        }
        AttributeValue::L(list) => Value::Array(list.iter().map(attribute_to_json).collect()),
        AttributeValue::Ss(items) => {
            Value::Array(items.iter().map(|s| Value::String(s.clone())).collect())
        }
        AttributeValue::Ns(items) => {
            Value::Array(items.iter().map(|n| number_to_json(n)).collect())
        }
        AttributeValue::B(blob) => Value::String(encode_base64(blob.as_ref())),
        AttributeValue::Bs(items) => Value::Array(
            items
                .iter()
                .map(|b| Value::String(encode_base64(b.as_ref())))
                .collect(),
        ),
        // `AttributeValue` is `#[non_exhaustive]`; any future variant degrades
        // to a debug string rather than panicking or dropping data silently.
        other => Value::String(format!("{other:?}")),
    }
}

/// Convert a whole DynamoDB item (map of attribute name -> value) into a JSON object.
pub fn item_to_json(item: &std::collections::HashMap<String, AttributeValue>) -> Value {
    let sorted: BTreeMap<&String, &AttributeValue> = item.iter().collect();
    let mut obj = Map::with_capacity(item.len());
    for (k, v) in sorted {
        obj.insert(k.clone(), attribute_to_json(v));
    }
    Value::Object(obj)
}

/// Convert a DynamoDB numeric string into a JSON value.
///
/// DynamoDB numbers are arbitrary-precision decimal strings. We only emit a JSON
/// *number* when it round-trips losslessly through a native numeric type;
/// otherwise we preserve the exact decimal string to avoid silent precision loss.
fn number_to_json(n: &str) -> Value {
    if let Ok(i) = n.parse::<i64>() {
        return Value::Number(i.into());
    }
    if let Ok(u) = n.parse::<u64>() {
        return Value::Number(u.into());
    }
    if let Ok(f) = n.parse::<f64>() {
        // Guard against precision loss: f64 reliably carries ~15 significant
        // decimal digits, so only emit a JSON number when the original decimal
        // has no more than that. Anything longer keeps its exact string form.
        if f.is_finite() && significant_digits(n) <= 15 {
            if let Some(num) = serde_json::Number::from_f64(f) {
                return Value::Number(num);
            }
        }
    }
    // Not safely representable as a JSON number: keep the exact string form.
    Value::String(n.to_string())
}

/// Count significant decimal digits in a DynamoDB number string (ignoring sign,
/// decimal point, exponent, and leading zeros).
fn significant_digits(n: &str) -> usize {
    let mantissa = n.split(['e', 'E']).next().unwrap_or(n);
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.trim_start_matches('0').len()
}

fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_dynamodb::primitives::Blob;
    use std::collections::HashMap;

    #[test]
    fn string_converts() {
        assert_eq!(
            attribute_to_json(&AttributeValue::S("hi".into())),
            Value::String("hi".into())
        );
    }

    #[test]
    fn integer_number_converts_to_json_number() {
        assert_eq!(
            attribute_to_json(&AttributeValue::N("12345".into())),
            Value::Number(12345i64.into())
        );
    }

    #[test]
    fn negative_integer_converts() {
        assert_eq!(
            attribute_to_json(&AttributeValue::N("-42".into())),
            Value::Number((-42i64).into())
        );
    }

    #[test]
    fn large_unsigned_number_converts() {
        // Larger than i64::MAX but fits u64.
        let big = "18446744073709551615"; // u64::MAX
        assert_eq!(
            attribute_to_json(&AttributeValue::N(big.into())),
            Value::Number(u64::MAX.into())
        );
    }

    #[test]
    fn float_number_converts() {
        let v = attribute_to_json(&AttributeValue::N("3.5".into()));
        assert_eq!(v, serde_json::json!(3.5));
    }

    #[test]
    fn high_precision_number_falls_back_to_string() {
        // 39 significant digits: not representable losslessly as i64/u64/f64.
        let precise = "123456789012345678901234567890.123456789";
        assert_eq!(
            attribute_to_json(&AttributeValue::N(precise.into())),
            Value::String(precise.into())
        );
    }

    #[test]
    fn bool_and_null_convert() {
        assert_eq!(
            attribute_to_json(&AttributeValue::Bool(true)),
            Value::Bool(true)
        );
        assert_eq!(
            attribute_to_json(&AttributeValue::Null(true)),
            Value::Null
        );
    }

    #[test]
    fn binary_converts_to_base64() {
        let v = attribute_to_json(&AttributeValue::B(Blob::new(vec![0xDE, 0xAD, 0xBE, 0xEF])));
        assert_eq!(v, Value::String("3q2+7w==".into()));
    }

    #[test]
    fn string_set_converts_to_array() {
        let v = attribute_to_json(&AttributeValue::Ss(vec!["a".into(), "b".into()]));
        assert_eq!(v, serde_json::json!(["a", "b"]));
    }

    #[test]
    fn number_set_converts_to_array_of_numbers() {
        let v = attribute_to_json(&AttributeValue::Ns(vec!["1".into(), "2".into()]));
        assert_eq!(v, serde_json::json!([1, 2]));
    }

    #[test]
    fn binary_set_converts_to_base64_array() {
        let v = attribute_to_json(&AttributeValue::Bs(vec![
            Blob::new(vec![0x00]),
            Blob::new(vec![0xFF]),
        ]));
        assert_eq!(v, serde_json::json!(["AA==", "/w=="]));
    }

    #[test]
    fn list_converts() {
        let v = attribute_to_json(&AttributeValue::L(vec![
            AttributeValue::S("x".into()),
            AttributeValue::N("1".into()),
        ]));
        assert_eq!(v, serde_json::json!(["x", 1]));
    }

    #[test]
    fn map_converts_with_sorted_keys() {
        let mut m = HashMap::new();
        m.insert("b".to_string(), AttributeValue::S("2".into()));
        m.insert("a".to_string(), AttributeValue::S("1".into()));
        let v = attribute_to_json(&AttributeValue::M(m));
        // Object key order is sorted deterministically.
        assert_eq!(v.to_string(), r#"{"a":"1","b":"2"}"#);
    }

    #[test]
    fn nested_structure_converts() {
        let mut inner = HashMap::new();
        inner.insert(
            "roles".to_string(),
            AttributeValue::L(vec![
                AttributeValue::S("engineer".into()),
                AttributeValue::S("admin".into()),
            ]),
        );
        inner.insert("active".to_string(), AttributeValue::Bool(true));
        let v = attribute_to_json(&AttributeValue::M(inner));
        assert_eq!(v, serde_json::json!({"active": true, "roles": ["engineer", "admin"]}));
    }

    #[test]
    fn item_to_json_produces_object() {
        let mut item = HashMap::new();
        item.insert("id".to_string(), AttributeValue::N("12345".into()));
        item.insert("name".to_string(), AttributeValue::S("Jane".into()));
        let v = item_to_json(&item);
        assert_eq!(v, serde_json::json!({"id": 12345, "name": "Jane"}));
    }
}
