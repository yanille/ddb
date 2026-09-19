//! The "active table" session model.
//!
//! `ddb` supports a virtualenv-style workflow: a table can be "activated" for
//! the current shell session (stored in the `DDB_TABLE` environment variable,
//! surfaced in the prompt via the shell integration in [`crate::commands::shell`]).
//! Commands that take a table may then omit it and operate on the active table.
//!
//! Resolution precedence is always: an explicit table argument wins, then the
//! active table (`$DDB_TABLE`), then a helpful error. The explicit argument
//! always overrides session state, so scripts are never affected by it.

use crate::error::DdbError;

/// The name of the environment variable that holds the active table.
pub const ACTIVE_TABLE_ENV: &str = "DDB_TABLE";

/// Environment variable the shell wrapper sets to a temp-file path. When present,
/// `use`/`tables` write the selected table name to it (instead of stdout) so the
/// wrapper can export it — robust against stdout being a TUI's terminal.
pub const ACTIVATE_FILE_ENV: &str = "DDB_ACTIVATE_FILE";

/// The active table for this shell session, if any. Empty/whitespace is treated
/// as unset.
pub fn active_table() -> Option<String> {
    match std::env::var(ACTIVE_TABLE_ENV) {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}

/// Resolve the table to operate on: the explicit argument if given, otherwise
/// the active table, otherwise an actionable usage error.
pub fn resolve_table(explicit: Option<&str>) -> Result<String, DdbError> {
    if let Some(table) = explicit {
        let table = table.trim();
        if !table.is_empty() {
            return Ok(table.to_string());
        }
    }
    if let Some(active) = active_table() {
        return Ok(active);
    }
    Err(DdbError::InvalidUsage(
        "no table specified and no active table set; pass a table name or run \
         `ddb use` to pick one"
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_argument_is_used_verbatim() {
        // Does not depend on the environment.
        assert_eq!(resolve_table(Some("Orders")).unwrap(), "Orders");
        assert_eq!(resolve_table(Some("  Orders  ")).unwrap(), "Orders");
    }

    #[test]
    fn env_resolution_and_precedence() {
        // All env-dependent assertions live in a single test to avoid races with
        // other tests mutating the same process-global variable.
        std::env::remove_var(ACTIVE_TABLE_ENV);
        assert!(active_table().is_none());
        // No explicit arg, no active table -> error.
        assert!(matches!(
            resolve_table(None),
            Err(DdbError::InvalidUsage(_))
        ));

        std::env::set_var(ACTIVE_TABLE_ENV, "EmployeeHistory");
        assert_eq!(active_table().as_deref(), Some("EmployeeHistory"));
        // Active table used when no explicit arg.
        assert_eq!(resolve_table(None).unwrap(), "EmployeeHistory");
        // Explicit arg overrides the active table.
        assert_eq!(resolve_table(Some("Orders")).unwrap(), "Orders");

        // Empty/whitespace is treated as unset.
        std::env::set_var(ACTIVE_TABLE_ENV, "   ");
        assert!(active_table().is_none());
        assert!(matches!(
            resolve_table(None),
            Err(DdbError::InvalidUsage(_))
        ));

        std::env::remove_var(ACTIVE_TABLE_ENV);
    }
}
