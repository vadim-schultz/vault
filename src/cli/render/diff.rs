//! Shared unified-diff rendering for `vault diff`, `log --verbose`, and `show`'s report mode.

/// Inputs for rendering one file's unified diff.
pub struct DiffInput<'a> {
    /// Path used in the binary-file notice (git's `a/<path>` / `b/<path>` wording).
    pub path: &'a str,
    /// Label for the `---` side of the unified-diff header.
    pub left_label: &'a str,
    /// Label for the `+++` side of the unified-diff header.
    pub right_label: &'a str,
    /// Content on the left, or `None` when the path did not exist on that side.
    pub left: Option<&'a [u8]>,
    /// Content on the right, or `None` when the path did not exist on that side.
    pub right: Option<&'a [u8]>,
}

/// Render a unified diff, or git's literal binary-file notice when either side isn't UTF-8.
#[must_use]
pub fn render_content_diff(input: &DiffInput<'_>) -> String {
    let Some((left_text, right_text)) = as_utf8_pair(input.left, input.right) else {
        return format!(
            "Binary files a/{} and b/{} differ\n",
            input.path, input.path
        );
    };
    similar::TextDiff::from_lines(left_text, right_text)
        .unified_diff()
        .header(input.left_label, input.right_label)
        .to_string()
}

fn as_utf8_pair<'a>(left: Option<&'a [u8]>, right: Option<&'a [u8]>) -> Option<(&'a str, &'a str)> {
    let left = std::str::from_utf8(left.unwrap_or(&[])).ok()?;
    let right = std::str::from_utf8(right.unwrap_or(&[])).ok()?;
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_unified_diff_for_text() {
        let out = render_content_diff(&DiffInput {
            path: "notes.md",
            left_label: "left",
            right_label: "right",
            left: Some(b"line1\n"),
            right: Some(b"line2\n"),
        });
        assert!(out.contains("-line1"));
        assert!(out.contains("+line2"));
    }

    #[test]
    fn binary_content_uses_git_literal_wording() {
        let out = render_content_diff(&DiffInput {
            path: "image.png",
            left_label: "left",
            right_label: "right",
            left: Some(&[0xff, 0xfe, 0x00]),
            right: Some(&[0xff, 0xfe, 0x01]),
        });
        assert_eq!(out, "Binary files a/image.png and b/image.png differ\n");
    }
}
