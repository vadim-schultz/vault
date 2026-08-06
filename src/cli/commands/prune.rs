//! `vault prune` command.

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::Result;

use crate::adapters::TomlRegistry;
use crate::app::prune;
use crate::cli::support::run_blocking;

/// Run `vault prune`.
pub async fn run() -> Result<()> {
    let removed = run_blocking(|| prune::prune(&TomlRegistry)).await?;
    print!("{}", render_report(&removed));
    Ok(())
}

fn render_report(removed: &[PathBuf]) -> String {
    if removed.is_empty() {
        return "No missing vaults to prune.\n".to_string();
    }
    let mut report = format!("Removed {} missing vault(s):\n", removed.len());
    for root in removed {
        let _ = writeln!(report, "  {}", root.display());
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_report_reports_no_missing_vaults() {
        assert_eq!(render_report(&[]), "No missing vaults to prune.\n");
    }

    #[test]
    fn render_report_lists_removed_roots() {
        let removed = vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
        assert_eq!(
            render_report(&removed),
            "Removed 2 missing vault(s):\n  /tmp/a\n  /tmp/b\n"
        );
    }
}
