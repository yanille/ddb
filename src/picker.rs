//! Interactive table picker.
//!
//! Uses a fuzzy-filterable select prompt. The prompt UI is drawn on **stderr**
//! (dialoguer's default terminal), so the selected table name — printed to
//! stdout by the caller — stays clean and capturable by the shell wrapper.

use std::io::IsTerminal;

use dialoguer::FuzzySelect;

use crate::error::DdbError;

/// True when a human is running interactively: stdin, stdout, and stderr are all
/// TTYs. The shell wrapper passes the selection back through a temp file (see
/// [`crate::active::ACTIVATE_FILE_ENV`]), so the binary keeps a real terminal on
/// all three streams; piping any of them (e.g. `ddb tables | cat`) disables the
/// picker and falls back to the plain list.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}

/// When a selection was made without the shell integration active (no activation
/// file), returns a one-line hint telling the user how to enable auto-activation,
/// with a manual fallback. Suppressed when stdout is not a terminal (scripts).
pub fn activation_hint(selection: &str) -> Option<String> {
    if std::io::stdout().is_terminal() {
        Some(format!(
            "'{selection}' not activated: add  eval \"$(ddb shell-init zsh)\"  to \
             your shell rc for auto-activation, or run  export DDB_TABLE={selection}"
        ))
    } else {
        None
    }
}

/// Index of `active` within `names`, for pre-selecting the current table.
pub fn default_index(names: &[String], active: Option<&str>) -> Option<usize> {
    let active = active?;
    names.iter().position(|n| n == active)
}

/// Present a fuzzy picker over `names`, pre-selecting `active` if present.
///
/// Returns `Ok(Some(name))` on selection, `Ok(None)` if the user cancels
/// (Esc/Ctrl-C) or the list is empty. Must only be called when [`is_interactive`]
/// is true.
pub fn select_table(names: &[String], active: Option<&str>) -> Result<Option<String>, DdbError> {
    if names.is_empty() {
        return Ok(None);
    }
    let mut prompt = FuzzySelect::new()
        .with_prompt("table (type to filter · Enter to activate · Esc to cancel)")
        .items(names);
    if let Some(index) = default_index(names, active) {
        prompt = prompt.default(index);
    }
    match prompt.interact_opt() {
        Ok(Some(index)) => Ok(names.get(index).cloned()),
        Ok(None) => Ok(None),
        Err(e) => Err(DdbError::Service(format!("table picker failed: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_index_finds_active() {
        let names = vec!["Users".to_string(), "Orders".to_string()];
        assert_eq!(default_index(&names, Some("Orders")), Some(1));
        assert_eq!(default_index(&names, Some("Missing")), None);
        assert_eq!(default_index(&names, None), None);
    }

    #[test]
    fn empty_names_yields_no_selection() {
        assert_eq!(select_table(&[], None).unwrap(), None);
    }
}
