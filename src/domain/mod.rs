//! Pure domain types with no I/O dependencies.

pub mod change;
pub mod history;
pub mod message;
pub mod queue;
pub mod rel_path;
pub mod snapshot;
pub mod vault;

pub use change::{FileChange, FileEventKind, PathKind};
pub use history::{CommitReport, FileVersionDiff, SnapshotEntry, TrackedFile};
pub use message::{parse_created_at, parse_single_verb, snapshot_message, verb_for};
pub use queue::{QueuedTask, TaskId, TaskKind};
pub use rel_path::RelPath;
pub use snapshot::{CommitSha, HistoryCommit, SnapshotRecord, SnapshotResult};
pub use vault::{missing_markers, vault_state, VaultLayout, VaultState};
