//! Output formatting.
//!
//! Two formats are supported: `human` (default, optimized for interactive
//! terminal use) and `json` (valid, machine-readable, `jq`-friendly).
//!
//! Contract: data goes to **stdout**; diagnostics, summaries, and warnings go
//! to **stderr**. In `json` mode nothing but the JSON payload is ever written
//! to stdout, so `ddb ... --output json | jq .` always works.

pub mod human;
pub mod json;

/// Selected output format.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[value(rename_all = "lower")]
pub enum OutputFormat {
    /// Human-readable, terminal-friendly output (default).
    #[default]
    Human,
    /// Machine-readable JSON.
    Json,
}

impl OutputFormat {
    pub fn is_json(self) -> bool {
        matches!(self, OutputFormat::Json)
    }
}
