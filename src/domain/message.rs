//! Human-readable summary of a snapshot's file changes.
//!
//! Shared by the write path (`app::snapshot::commit`, which prefixes this with `vault: ` to
//! build the real git commit message) and the read path (`log`/`show` rendering), so the line
//! `vault log` prints for a commit can never drift from that commit's actual message.

use super::change::{FileChange, FileEventKind};

/// Build the one-line summary for a snapshot's `changes`.
///
/// A single change reads as `"{verb} {path} @ {created_at}"`. A uniform-kind batch reads as
/// `"{verb} {N} files @ {created_at}"`; a batch mixing kinds (e.g. one modify, one delete) reads
/// as `"change {N} files @ {created_at}"` rather than overclaiming a single verb.
#[must_use]
pub fn snapshot_message(changes: &[FileChange], created_at: &str) -> String {
    match changes {
        [only] => single_change_message(only, created_at),
        _ => format!(
            "{} {} files @ {created_at}",
            batch_verb(changes),
            changes.len()
        ),
    }
}

fn single_change_message(change: &FileChange, created_at: &str) -> String {
    format!(
        "{} {} @ {created_at}",
        verb_for(change.kind),
        change.rel.as_str()
    )
}

fn batch_verb(changes: &[FileChange]) -> &'static str {
    let first = changes[0].kind;
    if changes.iter().all(|c| c.kind == first) {
        verb_for(first)
    } else {
        "change"
    }
}

/// Human verb for a single file-event kind.
#[must_use]
pub const fn verb_for(kind: FileEventKind) -> &'static str {
    match kind {
        FileEventKind::Create | FileEventKind::Modify => "update",
        FileEventKind::Delete => "delete",
        FileEventKind::Restore => "restore",
    }
}

/// Recover the `created_at` a `snapshot_message`-built commit message ends with — the inverse of
/// `snapshot_message`. Used by `vault reindex` to recover the exact timestamp a live `meta.db`
/// held, which can be more precise than the commit's own committer time (see
/// `app::reindex`/`GitStore::create_head_commit`). Returns `None` when `message` doesn't contain
/// the `" @ "` separator `snapshot_message` always inserts — e.g. a commit in a `.vault/.git`
/// mutated by something other than vault itself.
#[must_use]
pub fn parse_created_at(message: &str) -> Option<&str> {
    message.rsplit_once(" @ ").map(|(_, created_at)| created_at)
}

/// Recover the verb a single-change commit's message opens with (`"update"`, `"delete"`, or
/// `"restore"` — see [`verb_for`]), tolerating the `"vault: "` prefix
/// `app::snapshot::commit` adds to the raw git commit message. Only meaningful for a commit whose
/// diffed tree shows exactly one changed path: `verb_for` maps both `Create` and `Modify` to the
/// same `"update"`, and a batch's uniform-kind verb or mixed-kind `"change"` fallback carries no
/// per-file information this could resolve. Returns `None` for an empty message.
#[must_use]
pub fn parse_single_verb(message: &str) -> Option<&str> {
    message
        .strip_prefix("vault: ")
        .unwrap_or(message)
        .split_whitespace()
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RelPath;

    fn change(path: &str, kind: FileEventKind) -> FileChange {
        FileChange {
            rel: RelPath::parse(path),
            kind,
        }
    }

    #[test]
    fn verb_for_all_kinds() {
        assert_eq!(verb_for(FileEventKind::Create), "update");
        assert_eq!(verb_for(FileEventKind::Modify), "update");
        assert_eq!(verb_for(FileEventKind::Delete), "delete");
        assert_eq!(verb_for(FileEventKind::Restore), "restore");
    }

    #[test]
    fn single_change_reads_verb_path_timestamp() {
        let changes = vec![change("notes.md", FileEventKind::Modify)];
        assert_eq!(
            snapshot_message(&changes, "2026-08-05T12:00:00Z"),
            "update notes.md @ 2026-08-05T12:00:00Z"
        );
    }

    #[test]
    fn uniform_kind_batch_keeps_verb() {
        let changes = vec![
            change("a.md", FileEventKind::Create),
            change("b.md", FileEventKind::Create),
        ];
        assert_eq!(
            snapshot_message(&changes, "2026-08-05T12:00:00Z"),
            "update 2 files @ 2026-08-05T12:00:00Z"
        );
    }

    #[test]
    fn mixed_kind_batch_falls_back_to_change() {
        let changes = vec![
            change("a.md", FileEventKind::Modify),
            change("b.md", FileEventKind::Delete),
        ];
        assert_eq!(
            snapshot_message(&changes, "2026-08-05T12:00:00Z"),
            "change 2 files @ 2026-08-05T12:00:00Z"
        );
    }

    #[test]
    fn parse_created_at_round_trips_through_snapshot_message() {
        let single = vec![change("notes.md", FileEventKind::Modify)];
        let uniform_batch = vec![
            change("a.md", FileEventKind::Create),
            change("b.md", FileEventKind::Create),
        ];
        let mixed_batch = vec![
            change("a.md", FileEventKind::Modify),
            change("b.md", FileEventKind::Delete),
        ];
        for changes in [single, uniform_batch, mixed_batch] {
            let message = snapshot_message(&changes, "2026-08-05T12:00:00Z");
            assert_eq!(parse_created_at(&message), Some("2026-08-05T12:00:00Z"));
        }
    }

    #[test]
    fn parse_created_at_tolerates_the_vault_commit_prefix() {
        let changes = vec![change("notes.md", FileEventKind::Create)];
        let message = format!(
            "vault: {}",
            snapshot_message(&changes, "2026-08-05T12:00:00Z")
        );
        assert_eq!(parse_created_at(&message), Some("2026-08-05T12:00:00Z"));
    }

    #[test]
    fn parse_created_at_returns_none_without_separator() {
        assert_eq!(parse_created_at("not a vault message"), None);
    }

    #[test]
    fn parse_single_verb_recovers_verb_for_every_kind() {
        for kind in [
            FileEventKind::Create,
            FileEventKind::Modify,
            FileEventKind::Delete,
            FileEventKind::Restore,
        ] {
            let changes = vec![change("a.md", kind)];
            let message = format!(
                "vault: {}",
                snapshot_message(&changes, "2026-08-05T12:00:00Z")
            );
            assert_eq!(parse_single_verb(&message), Some(verb_for(kind)));
        }
    }

    #[test]
    fn parse_single_verb_without_vault_prefix() {
        let changes = vec![change("a.md", FileEventKind::Restore)];
        let message = snapshot_message(&changes, "2026-08-05T12:00:00Z");
        assert_eq!(parse_single_verb(&message), Some("restore"));
    }

    #[test]
    fn parse_single_verb_returns_none_for_empty_message() {
        assert_eq!(parse_single_verb(""), None);
    }
}
