//! `vault reindex` command.

use std::fmt::Write as _;

use anyhow::Result;
use clap::Args;

use crate::app::reindex::{self, Overwrite, ReindexOutcome};
use crate::cli::support::{run_blocking, Global};

/// Arguments for `vault reindex`.
#[derive(Debug, Args)]
pub struct ReindexArgs {
    /// Overwrite an existing, populated `meta.db`.
    #[arg(long)]
    pub force: bool,
    /// Preview what would be reindexed without writing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Run `vault reindex`.
pub async fn run(global: &Global, args: ReindexArgs) -> Result<()> {
    let vault_path = global.vault_path.clone();
    let (layout, outcome) = if args.dry_run {
        run_blocking(move || reindex::preview(vault_path)).await?
    } else {
        let overwrite = overwrite_from(args.force);
        run_blocking(move || reindex::rebuild(vault_path, overwrite)).await?
    };
    let vault_display = layout.vault_dir.display().to_string();
    print!("{}", render_report(&vault_display, &outcome));
    if global.verbose {
        eprintln!("reindex outcome for {vault_display}: {outcome:?}");
    }
    Ok(())
}

fn overwrite_from(force: bool) -> Overwrite {
    if force {
        Overwrite::Force
    } else {
        Overwrite::Refuse
    }
}

fn render_report(vault_display: &str, outcome: &ReindexOutcome) -> String {
    let mut report = String::new();
    let headline = if outcome.dry_run {
        "Would reindex"
    } else {
        "Reindexed"
    };
    match &outcome.span {
        Some((oldest, newest)) => {
            let commits = outcome.commits;
            let plural = if commits == 1 { "" } else { "s" };
            let _ = writeln!(
                report,
                "{headline} meta.db at {vault_display} ({commits} commit{plural}, {oldest} to {newest})"
            );
        }
        None => {
            let _ = writeln!(
                report,
                "{headline} meta.db at {vault_display} (no commits — empty history)"
            );
        }
    }
    if outcome.dry_run && outcome.existing_snapshot_count > 0 {
        let snapshot_count = outcome.existing_snapshot_count;
        let _ = writeln!(
            report,
            "meta.db already has {snapshot_count} snapshot(s) — pass --force to actually rebuild it"
        );
    }
    if outcome.lossy_timestamps > 0 {
        let lossy_timestamps = outcome.lossy_timestamps;
        let _ = writeln!(
            report,
            "{lossy_timestamps} commit(s) had no vault-formatted timestamp in their message — used .git's own committer time instead"
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(
        commits: usize,
        span: Option<(&str, &str)>,
        lossy_timestamps: usize,
        existing_snapshot_count: i64,
        dry_run: bool,
    ) -> ReindexOutcome {
        ReindexOutcome {
            commits,
            span: span.map(|(o, n)| (o.to_string(), n.to_string())),
            lossy_timestamps,
            existing_snapshot_count,
            dry_run,
        }
    }

    #[test]
    fn reports_a_real_rebuild() {
        let report = render_report(
            "/vault/.vault",
            &outcome(
                2,
                Some(("2026-01-01T00:00:00+00:00", "2026-01-02T00:00:00+00:00")),
                0,
                0,
                false,
            ),
        );
        assert_eq!(
            report,
            "Reindexed meta.db at /vault/.vault (2 commits, 2026-01-01T00:00:00+00:00 to 2026-01-02T00:00:00+00:00)\n"
        );
    }

    #[test]
    fn reports_singular_commit() {
        let report = render_report(
            "/vault/.vault",
            &outcome(
                1,
                Some(("2026-01-01T00:00:00+00:00", "2026-01-01T00:00:00+00:00")),
                0,
                0,
                false,
            ),
        );
        assert!(report.starts_with("Reindexed meta.db at /vault/.vault (1 commit, "));
    }

    #[test]
    fn reports_empty_history() {
        let report = render_report("/vault/.vault", &outcome(0, None, 0, 0, false));
        assert_eq!(
            report,
            "Reindexed meta.db at /vault/.vault (no commits — empty history)\n"
        );
    }

    #[test]
    fn dry_run_notes_force_is_needed() {
        let report = render_report("/vault/.vault", &outcome(3, Some(("a", "b")), 0, 3, true));
        assert!(report.starts_with("Would reindex meta.db at /vault/.vault (3 commits, a to b)\n"));
        assert!(report.contains("meta.db already has 3 snapshot(s) — pass --force"));
    }

    #[test]
    fn dry_run_without_existing_rows_has_no_force_note() {
        let report = render_report("/vault/.vault", &outcome(3, Some(("a", "b")), 0, 0, true));
        assert!(!report.contains("--force"));
    }

    #[test]
    fn reports_lossy_timestamp_count() {
        let report = render_report("/vault/.vault", &outcome(2, Some(("a", "b")), 1, 0, false));
        assert!(report.contains("1 commit(s) had no vault-formatted timestamp"));
    }
}
