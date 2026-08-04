//! Composition root for CLI commands.
//!
//! This is the only module under `cli/` allowed to name concrete adapter types — every command
//! reaches storage and the clock through [`Stores`] and [`clock`] instead of importing
//! `crate::adapters` directly.

use crate::adapters::{GixObjectStore, SqliteMetaIndex, SystemClock};
use crate::domain::VaultLayout;
use crate::error::VaultError;

/// Object store and metadata index for a vault, opened together.
pub struct Stores {
    /// Git object store.
    pub object_store: GixObjectStore,
    /// `SQLite` metadata index.
    pub meta_index: SqliteMetaIndex,
}

impl Stores {
    /// Open both stores for `layout`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when either store cannot be opened.
    pub fn open(layout: &VaultLayout) -> Result<Self, VaultError> {
        Ok(Self {
            object_store: GixObjectStore::open(layout)?,
            meta_index: SqliteMetaIndex::open(layout.meta_db_path())?,
        })
    }
}

/// Open only the metadata index, for commands that never touch the object store.
///
/// # Errors
///
/// Returns [`VaultError`] when the index cannot be opened.
pub fn open_meta_index(layout: &VaultLayout) -> Result<SqliteMetaIndex, VaultError> {
    SqliteMetaIndex::open(layout.meta_db_path())
}

/// The production wall clock, for commands (like `restore`) that need one.
#[must_use]
pub fn clock() -> SystemClock {
    SystemClock
}
