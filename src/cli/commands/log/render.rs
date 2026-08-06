//! `vault log` rendering — `git log --stat` shape by default, `git log -p` under `--verbose`.

use crate::cli::render::{render_diffstat, render_full_diffs, DiffStatInput};
use crate::domain::{CommitReport, FileVersionDiff};

/// Render every commit report, newest first, with a blank line between commits.
pub fn render_report(reports: &[CommitReport], verbose: bool) -> String {
    if reports.is_empty() {
        return "No snapshots yet.\n".to_string();
    }
    reports
        .iter()
        .map(|report| render_commit(report, verbose))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_commit(report: &CommitReport, verbose: bool) -> String {
    let body = if verbose {
        render_full_diffs(&report.files)
    } else {
        render_diffstat_block(&report.files)
    };
    format!("{}\n{body}", report.message)
}

fn render_diffstat_block(files: &[FileVersionDiff]) -> String {
    let inputs: Vec<DiffStatInput<'_>> = files
        .iter()
        .map(|f| DiffStatInput {
            path: f.path.as_str(),
            previous: f.previous.as_deref(),
            current: f.current.as_deref(),
        })
        .collect();
    render_diffstat(&inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RelPath;

    fn report(
        message: &str,
        path: &str,
        previous: Option<&[u8]>,
        current: Option<&[u8]>,
    ) -> CommitReport {
        CommitReport {
            message: message.to_string(),
            files: vec![FileVersionDiff {
                path: RelPath::parse(path),
                previous: previous.map(<[u8]>::to_vec),
                current: current.map(<[u8]>::to_vec),
            }],
        }
    }

    #[test]
    fn empty_reports_say_no_snapshots() {
        assert_eq!(render_report(&[], false), "No snapshots yet.\n");
    }

    #[test]
    fn default_view_has_no_commit_sha_and_shows_diffstat() {
        let reports = vec![report(
            "update notes.md @ 2026-08-05T12:58:27+00:00",
            "notes.md",
            Some(b"line1\n"),
            Some(b"line2\n"),
        )];
        let out = render_report(&reports, false);
        assert!(out.starts_with("update notes.md @ 2026-08-05T12:58:27+00:00\n"));
        assert!(out.contains(" notes.md | 2 +-\n"));
        assert!(!out.contains("-line1"));
    }

    #[test]
    fn verbose_view_shows_full_diff_hunks() {
        let reports = vec![report(
            "update notes.md @ 2026-08-05T12:58:27+00:00",
            "notes.md",
            Some(b"line1\n"),
            Some(b"line2\n"),
        )];
        let out = render_report(&reports, true);
        assert!(out.contains("-line1"));
        assert!(out.contains("+line2"));
    }

    #[test]
    fn blank_line_separates_commits() {
        let reports = vec![
            report("update a.md @ t2", "a.md", Some(b"x\n"), Some(b"y\n")),
            report("update b.md @ t1", "b.md", None, Some(b"z\n")),
        ];
        let out = render_report(&reports, false);
        assert!(out.contains("insertion(+), 1 deletion(-)\n\nupdate b.md"));
    }
}
