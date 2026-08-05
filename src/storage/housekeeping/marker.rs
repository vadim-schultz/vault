//! Housekeeping marker persistence in `.vault/housekeeping.json`.

use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::VaultError;
use crate::paths::HOUSEKEEPING_FILE;

use super::fs::ObjectCounts;
use super::repack::RepackOutcome;

/// Persisted record of the last repack run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepackRecord {
    /// When the repack finished (UTC RFC3339).
    pub ran_at: String,
    /// Objects written into the new pack.
    pub objects_packed: usize,
    /// Loose object files removed after verification.
    pub loose_removed: usize,
    /// Total bytes under `.git/` before the repack.
    pub bytes_before: u64,
    /// Total bytes under `.git/` after cleanup.
    pub bytes_after: u64,
}

/// Persisted housekeeping marker in `.vault/housekeeping.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HousekeepingMarker {
    /// When counts were last checked.
    pub checked_at: String,
    /// Live counts at `checked_at`.
    pub counts: ObjectCounts,
    /// Last repack outcome, if any.
    pub last_repack: Option<RepackRecord>,
}

/// Read the housekeeping marker, returning defaults when absent.
///
/// # Errors
///
/// Returns [`VaultError`] when the marker file exists but cannot be parsed.
pub fn read_marker(vault_dir: &Path) -> Result<HousekeepingMarker, VaultError> {
    let path = vault_dir.join(HOUSEKEEPING_FILE);
    if !path.is_file() {
        return Ok(default_marker());
    }
    let contents = fs::read_to_string(path)?;
    let marker: HousekeepingMarker = serde_json::from_str(&contents)?;
    Ok(marker)
}

pub(crate) fn write_marker(
    vault_dir: &Path,
    marker: &HousekeepingMarker,
) -> Result<(), VaultError> {
    let path = vault_dir.join(HOUSEKEEPING_FILE);
    let contents = serde_json::to_string_pretty(marker)?;
    fs::write(path, contents)?;
    Ok(())
}

pub(crate) fn default_marker() -> HousekeepingMarker {
    HousekeepingMarker {
        checked_at: String::new(),
        counts: ObjectCounts { loose: 0, packs: 0 },
        last_repack: None,
    }
}

pub(crate) fn repack_record_from_outcome(outcome: &RepackOutcome) -> RepackRecord {
    RepackRecord {
        ran_at: outcome.ran_at.to_rfc3339(),
        objects_packed: outcome.objects_packed,
        loose_removed: outcome.loose_removed,
        bytes_before: outcome.bytes_before,
        bytes_after: outcome.bytes_after,
    }
}

pub(crate) fn last_repack_at(marker: &HousekeepingMarker) -> Option<DateTime<Utc>> {
    marker
        .last_repack
        .as_ref()
        .and_then(|record| DateTime::parse_from_rfc3339(&record.ran_at).ok())
        .map(|dt| dt.with_timezone(&Utc))
}
