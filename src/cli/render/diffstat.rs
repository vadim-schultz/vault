//! Diffstat rendering shared by `vault log`'s default view and `vault show`'s report mode.

use std::fmt::Write as _;

use similar::{ChangeTag, TextDiff};

/// One file's before/after content for a diffstat line.
pub struct DiffStatInput<'a> {
    /// Path relative to the vault worktree.
    pub path: &'a str,
    /// Content before this change, or `None` when the path did not exist yet.
    pub previous: Option<&'a [u8]>,
    /// Content after this change, or `None` when the path was deleted.
    pub current: Option<&'a [u8]>,
}

enum FileStat {
    Text { insertions: usize, deletions: usize },
    Binary,
}

/// Render the diffstat block (one line per file, then a totals line), `git --stat` style.
#[must_use]
pub fn render_diffstat(files: &[DiffStatInput<'_>]) -> String {
    let stats: Vec<(&str, FileStat)> = files.iter().map(|f| (f.path, file_stat(f))).collect();
    let mut out = String::new();
    for (path, stat) in &stats {
        out.push_str(&render_file_line(path, stat));
    }
    let (insertions, deletions) = totals(&stats);
    out.push_str(&render_summary_line(stats.len(), insertions, deletions));
    out
}

fn totals(stats: &[(&str, FileStat)]) -> (usize, usize) {
    stats
        .iter()
        .fold((0, 0), |(ins, del), (_, stat)| match stat {
            FileStat::Text {
                insertions,
                deletions,
            } => (ins + insertions, del + deletions),
            FileStat::Binary => (ins, del),
        })
}

fn file_stat(input: &DiffStatInput<'_>) -> FileStat {
    let Some((previous, current)) = as_utf8_pair(input.previous, input.current) else {
        return FileStat::Binary;
    };
    let diff = TextDiff::from_lines(previous, current);
    let mut insertions = 0;
    let mut deletions = 0;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => insertions += 1,
            ChangeTag::Delete => deletions += 1,
            ChangeTag::Equal => {}
        }
    }
    FileStat::Text {
        insertions,
        deletions,
    }
}

fn as_utf8_pair<'a>(
    previous: Option<&'a [u8]>,
    current: Option<&'a [u8]>,
) -> Option<(&'a str, &'a str)> {
    let previous = std::str::from_utf8(previous.unwrap_or(&[])).ok()?;
    let current = std::str::from_utf8(current.unwrap_or(&[])).ok()?;
    Some((previous, current))
}

fn render_file_line(path: &str, stat: &FileStat) -> String {
    match stat {
        FileStat::Binary => format!(" {path} | Bin\n"),
        FileStat::Text {
            insertions,
            deletions,
        } => {
            let bar = "+".repeat(*insertions) + &"-".repeat(*deletions);
            format!(" {path} | {} {bar}\n", insertions + deletions)
        }
    }
}

fn render_summary_line(file_count: usize, insertions: usize, deletions: usize) -> String {
    let mut line = format!("{} changed", pluralize(file_count, "file", "files"));
    if insertions > 0 {
        let _ = write!(
            line,
            ", {}",
            pluralize(insertions, "insertion(+)", "insertions(+)")
        );
    }
    if deletions > 0 {
        let _ = write!(
            line,
            ", {}",
            pluralize(deletions, "deletion(-)", "deletions(-)")
        );
    }
    line.push('\n');
    line
}

fn pluralize(count: usize, singular: &str, plural: &str) -> String {
    let word = if count == 1 { singular } else { plural };
    format!("{count} {word}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_insertion_and_deletion() {
        let files = [DiffStatInput {
            path: "notes.md",
            previous: Some(b"line1\n"),
            current: Some(b"line2\n"),
        }];
        let out = render_diffstat(&files);
        assert_eq!(
            out,
            " notes.md | 2 +-\n1 file changed, 1 insertion(+), 1 deletion(-)\n"
        );
    }

    #[test]
    fn insertion_only_omits_deletion_clause() {
        let files = [DiffStatInput {
            path: "notes.md",
            previous: None,
            current: Some(b"line1\n"),
        }];
        let out = render_diffstat(&files);
        assert_eq!(out, " notes.md | 1 +\n1 file changed, 1 insertion(+)\n");
    }

    #[test]
    fn deletion_only_omits_insertion_clause() {
        let files = [DiffStatInput {
            path: "draft.md",
            previous: Some(b"line1\n"),
            current: None,
        }];
        let out = render_diffstat(&files);
        assert_eq!(out, " draft.md | 1 -\n1 file changed, 1 deletion(-)\n");
    }

    #[test]
    fn multi_file_pluralizes_files_and_totals() {
        let files = [
            DiffStatInput {
                path: "a.md",
                previous: Some(b"a\nb\n"),
                current: Some(b"a\nb\nc\n"),
            },
            DiffStatInput {
                path: "b.md",
                previous: Some(b"x\n"),
                current: None,
            },
        ];
        let out = render_diffstat(&files);
        assert_eq!(
            out,
            " a.md | 1 +\n b.md | 1 -\n2 files changed, 1 insertion(+), 1 deletion(-)\n"
        );
    }

    #[test]
    fn binary_file_shown_as_bin() {
        let files = [DiffStatInput {
            path: "image.png",
            previous: Some(&[0xff, 0xfe]),
            current: Some(&[0xff, 0xfd]),
        }];
        let out = render_diffstat(&files);
        assert_eq!(out, " image.png | Bin\n1 file changed\n");
    }
}
