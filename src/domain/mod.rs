//! Pure domain types with no I/O dependencies.

pub mod change;
pub mod rel_path;
pub mod snapshot;
pub mod vault;

pub use change::{FileChange, FileEventKind};
pub use rel_path::RelPath;
pub use snapshot::{CommitSha, SnapshotRecord, SnapshotResult};
pub use vault::{vault_state, VaultLayout, VaultState};
