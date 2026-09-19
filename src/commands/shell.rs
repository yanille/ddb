//! `ddb shell-init <shell>` — print shell integration for the venv-style active
//! table.
//!
//! The emitted snippet defines a `ddb` shell **function** that wraps the binary.
//! For `use`/`tables` run interactively it captures the selected table (printed
//! to stdout by the binary) and exports it as `DDB_TABLE` in the current shell;
//! `deactivate` unsets it; everything else passes through. It also prepends
//! `(ddb:<table>)` to the prompt while a table is active, like a Python
//! virtualenv's `(env)`.

use crate::cli::ShellKind;

use super::Rendered;

pub fn run(shell: ShellKind) -> Rendered {
    let script = match shell {
        ShellKind::Zsh => ZSH,
        ShellKind::Bash => BASH,
    };
    Rendered::stdout(script.trim_end().to_string())
}

const ZSH: &str = r#"# ddb shell integration (zsh). Add to ~/.zshrc:
#   eval "$(ddb shell-init zsh)"
ddb() {
  case "$1" in
    use|tables)
      if [ -t 0 ] && [ -t 1 ] && [ -t 2 ]; then
        local _ddb_f
        _ddb_f="$(mktemp -t ddb.XXXXXX)" || { command ddb "$@"; return; }
        DDB_ACTIVATE_FILE="$_ddb_f" command ddb "$@"
        local _ddb_status=$?
        local _ddb_sel
        _ddb_sel="$(cat "$_ddb_f" 2>/dev/null)"
        rm -f "$_ddb_f"
        if [ -n "$_ddb_sel" ]; then
          export DDB_TABLE="$_ddb_sel"
        fi
        return $_ddb_status
      else
        command ddb "$@"
      fi
      ;;
    deactivate)
      unset DDB_TABLE
      ;;
    *)
      command ddb "$@"
      ;;
  esac
}

_ddb_prompt() {
  if [ -n "$DDB_TABLE" ]; then
    print -n "(ddb:$DDB_TABLE) "
  fi
}

# Re-source-safe: only add the prefix if PROMPT does not already contain it.
if [[ "$PROMPT" != *_ddb_prompt* ]]; then
  setopt prompt_subst
  PROMPT='$(_ddb_prompt)'"$PROMPT"
fi
"#;

const BASH: &str = r#"# ddb shell integration (bash). Add to ~/.bashrc:
#   eval "$(ddb shell-init bash)"
ddb() {
  case "$1" in
    use|tables)
      if [ -t 0 ] && [ -t 1 ] && [ -t 2 ]; then
        local _ddb_f
        _ddb_f="$(mktemp -t ddb.XXXXXX)" || { command ddb "$@"; return; }
        DDB_ACTIVATE_FILE="$_ddb_f" command ddb "$@"
        local _ddb_status=$?
        local _ddb_sel
        _ddb_sel="$(cat "$_ddb_f" 2>/dev/null)"
        rm -f "$_ddb_f"
        if [ -n "$_ddb_sel" ]; then
          export DDB_TABLE="$_ddb_sel"
        fi
        return $_ddb_status
      else
        command ddb "$@"
      fi
      ;;
    deactivate)
      unset DDB_TABLE
      ;;
    *)
      command ddb "$@"
      ;;
  esac
}

_ddb_prompt() {
  if [ -n "$DDB_TABLE" ]; then
    printf '(ddb:%s) ' "$DDB_TABLE"
  fi
}

# Re-source-safe: only add the prefix if PS1 does not already contain it.
if [[ "$PS1" != *_ddb_prompt* ]]; then
  PS1='$(_ddb_prompt)'"$PS1"
fi
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_snippet_has_wrapper_and_prompt() {
        let out = run(ShellKind::Zsh).stdout;
        assert!(out.contains("ddb() {"));
        assert!(out.contains("command ddb \"$@\""));
        assert!(out.contains("export DDB_TABLE="));
        assert!(out.contains("unset DDB_TABLE"));
        assert!(out.contains("_ddb_prompt"));
        assert!(out.contains("setopt prompt_subst"));
    }

    #[test]
    fn bash_snippet_has_wrapper_and_prompt() {
        let out = run(ShellKind::Bash).stdout;
        assert!(out.contains("ddb() {"));
        assert!(out.contains("command ddb \"$@\""));
        assert!(out.contains("export DDB_TABLE="));
        assert!(out.contains("unset DDB_TABLE"));
        assert!(out.contains("PS1="));
    }
}
