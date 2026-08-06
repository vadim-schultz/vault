//! Rendering shared across `diff`, `log`, and `show` — unified diffs and diffstats.

pub mod diff;
pub mod diffstat;

pub use diff::{render_content_diff, DiffInput};
pub use diffstat::{render_diffstat, DiffStatInput};

use crate::domain::FileVersionDiff;

/// Render every file's full unified diff back-to-back — `log --verbose`'s and `show`'s body.
pub fn render_full_diffs(files: &[FileVersionDiff]) -> String {
    files.iter().map(render_one_diff).collect()
}

fn render_one_diff(file: &FileVersionDiff) -> String {
    let path = file.path.as_str();
    render_content_diff(&DiffInput {
        path,
        left_label: &format!("a/{path}"),
        right_label: &format!("b/{path}"),
        left: file.previous.as_deref(),
        right: file.current.as_deref(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RelPath;

    #[test]
    fn concatenates_each_file_diff_with_no_separator() {
        let files = vec![
            FileVersionDiff {
                path: RelPath::parse("a.md"),
                previous: Some(b"x\n".to_vec()),
                current: Some(b"y\n".to_vec()),
            },
            FileVersionDiff {
                path: RelPath::parse("b.md"),
                previous: None,
                current: Some(b"z\n".to_vec()),
            },
        ];
        let out = render_full_diffs(&files);
        assert!(out.contains("a/a.md"));
        assert!(out.contains("a/b.md"));
        assert!(out.contains("-x"));
        assert!(out.contains("+z"));
    }
}
