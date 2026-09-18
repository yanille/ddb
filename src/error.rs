//! Error types and process exit codes for `ddb`.
//!
//! Errors are deliberately developer-friendly and never leak credentials,
//! secret keys, or session tokens. AWS SDK errors are classified into a small
//! set of actionable categories.

use aws_sdk_dynamodb::error::{ProvideErrorMetadata, SdkError};

/// Top-level error type for all `ddb` operations.
#[derive(Debug, thiserror::Error)]
pub enum DdbError {
    #[error("table '{0}' was not found")]
    TableNotFound(String),

    #[error("index '{index}' was not found on table '{table}'")]
    IndexNotFound { table: String, index: String },

    #[error("access denied when accessing table '{0}'")]
    AccessDenied(String),

    #[error("AWS credentials could not be resolved")]
    NoCredentials,

    /// A usage/validation error caused by bad CLI arguments.
    #[error("{0}")]
    InvalidUsage(String),

    /// A DynamoDB service or transport error that does not map to a more
    /// specific category. The contained message is already sanitized.
    #[error("{0}")]
    Service(String),
}

impl DdbError {
    /// Process exit code for this error.
    ///
    /// Exit-code contract (documented in the README):
    /// - `0` success
    /// - `1` generic service/transport error
    /// - `2` invalid usage / bad arguments
    /// - `3` table not found
    /// - `4` access denied
    /// - `5` credentials could not be resolved
    /// - `6` index not found
    pub fn exit_code(&self) -> i32 {
        match self {
            DdbError::Service(_) => 1,
            DdbError::InvalidUsage(_) => 2,
            DdbError::TableNotFound(_) => 3,
            DdbError::AccessDenied(_) => 4,
            DdbError::NoCredentials => 5,
            DdbError::IndexNotFound { .. } => 6,
        }
    }

    /// Stable, machine-readable error code used in JSON error output.
    pub fn code(&self) -> &'static str {
        match self {
            DdbError::TableNotFound(_) => "table_not_found",
            DdbError::IndexNotFound { .. } => "index_not_found",
            DdbError::AccessDenied(_) => "access_denied",
            DdbError::NoCredentials => "no_credentials",
            DdbError::InvalidUsage(_) => "invalid_usage",
            DdbError::Service(_) => "service_error",
        }
    }
}

/// Map an AWS SDK error for a specific operation into a `DdbError`.
///
/// `table` is the table involved, used to produce helpful messages.
pub fn map_sdk_error<E>(table: Option<&str>, err: SdkError<E>) -> DdbError
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    match err {
        SdkError::ServiceError(context) => {
            let inner = context.into_err();
            let code = inner.code().map(str::to_string);
            match code.as_deref() {
                Some("ResourceNotFoundException") => {
                    DdbError::TableNotFound(table.unwrap_or("<unknown>").to_string())
                }
                Some("AccessDeniedException") => {
                    DdbError::AccessDenied(table.unwrap_or("<unknown>").to_string())
                }
                Some(
                    "UnrecognizedClientException"
                    | "InvalidSignatureException"
                    | "InvalidClientTokenId"
                    | "ExpiredTokenException"
                    | "MissingAuthenticationTokenException",
                ) => DdbError::NoCredentials,
                Some(other) => DdbError::Service(sanitized_service_message(other, &inner)),
                None => DdbError::Service(error_chain_string(&inner)),
            }
        }
        other => {
            // Construction / dispatch / timeout / auth failures resolved before a
            // service response. Classify credentials failures specifically.
            let chain = error_chain_string(&other);
            if looks_like_credentials_error(&chain) {
                DdbError::NoCredentials
            } else {
                DdbError::Service(chain)
            }
        }
    }
}

fn sanitized_service_message<E: std::error::Error>(code: &str, err: &E) -> String {
    // Prefer the modeled message but always include the AWS error code so the
    // developer can look it up. Neither contains credential material.
    let msg = err.to_string();
    if msg.is_empty() {
        code.to_string()
    } else {
        format!("{code}: {msg}")
    }
}

fn looks_like_credentials_error(chain: &str) -> bool {
    let lower = chain.to_lowercase();
    lower.contains("credential")
        || lower.contains("no providers in chain")
        || lower.contains("failed to load credentials")
        || lower.contains("could not load credentials")
}

/// Walk an error's `source()` chain into a single, secret-free string.
fn error_chain_string(err: &dyn std::error::Error) -> String {
    let mut out = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        out.push_str(": ");
        out.push_str(&inner.to_string());
        source = inner.source();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_distinct_and_stable() {
        assert_eq!(DdbError::TableNotFound("t".into()).exit_code(), 3);
        assert_eq!(DdbError::AccessDenied("t".into()).exit_code(), 4);
        assert_eq!(DdbError::NoCredentials.exit_code(), 5);
        assert_eq!(DdbError::InvalidUsage("x".into()).exit_code(), 2);
        assert_eq!(DdbError::Service("x".into()).exit_code(), 1);
        assert_eq!(
            DdbError::IndexNotFound {
                table: "t".into(),
                index: "i".into()
            }
            .exit_code(),
            6
        );
    }

    #[test]
    fn codes_are_stable() {
        assert_eq!(DdbError::NoCredentials.code(), "no_credentials");
        assert_eq!(DdbError::TableNotFound("t".into()).code(), "table_not_found");
    }

    #[test]
    fn credentials_heuristic_matches() {
        assert!(looks_like_credentials_error(
            "dispatch failure: no providers in chain"
        ));
        assert!(looks_like_credentials_error("failed to load credentials"));
        assert!(!looks_like_credentials_error("connection timed out"));
    }
}
