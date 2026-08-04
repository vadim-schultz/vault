//! Pure domain types with no I/O dependencies.

pub mod change;
pub mod history;
pub mod queue;
pub mod rel_path;
pub mod snapshot;
pub mod vault;

pub use change::{FileChange, FileEventKind, PathKind};
pub use history::{SnapshotEntry, TrackedFile};
pub use queue::{QueuedTask, TaskId, TaskKind};
pub use rel_path::RelPath;
pub use snapshot::{CommitSha, SnapshotRecord, SnapshotResult};
pub use vault::{vault_state, VaultLayout, VaultState};
