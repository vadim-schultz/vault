//! Conditional git repack housekeeping (loose-object and pack thresholds).

mod fs;
mod marker;
mod repack;

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::config::GcConfig;
use crate::domain::VaultLayout;
use crate::error::VaultError;

pub use fs::{count_objects, ObjectCounts};
pub use marker::{read_marker, HousekeepingMarker, RepackRecord};
pub use repack::{repack, RepackOutcome};

use marker::{last_repack_at, repack_record_from_outcome, write_marker};

/// Status returned by [`maybe_run`] and surfaced in `vault status`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HousekeepingStatus {
    /// Current loose/pack counts.
    pub counts: ObjectCounts,
    /// Last repack record from the marker, if any.
    pub last_repack: Option<RepackRecord>,
    /// Whether a repack ran during this check.
    pub repack_ran: bool,
}

/// Return whether housekeeping thresholds are exceeded.
#[must_use]
pub fn is_due(
    counts: ObjectCounts,
    last_repack_at: Option<DateTime<Utc>>,
    thresholds: &GcConfig,
    now: DateTime<Utc>,
) -> bool {
    if counts.loose > thresholds.loose_object_limit {
        return true;
    }
    if counts.packs > thresholds.pack_limit {
        return true;
    }
    let Some(last) = last_repack_at else {
        return false;
    };
    let max_age =
        chrono::Duration::seconds(i64::try_from(thresholds.max_age_secs).unwrap_or(i64::MAX));
    now.signed_duration_since(last) > max_age
}

/// Run housekeeping when thresholds are exceeded; always refresh the marker.
///
/// # Errors
///
/// Returns [`VaultError`] when counts, repack, or marker I/O fail.
pub fn maybe_run(
    layout: &VaultLayout,
    gc_config: &GcConfig,
) -> Result<HousekeepingStatus, VaultError> {
    let check = housekeeping_check(layout)?;
    if !is_due(check.counts, check.last_repack_at, gc_config, check.now) {
        return finish_without_repack(layout, &check);
    }
    finish_with_repack(layout, &check)
}

/// Collect live counts and marker history for `vault status`.
///
/// # Errors
///
/// Returns [`VaultError`] when object counts or the marker cannot be read.
pub fn status_for(layout: &VaultLayout) -> Result<HousekeepingStatus, VaultError> {
    let counts = count_objects(&layout.git_dir_path())?;
    let marker = read_marker(&layout.vault_dir)?;
    Ok(HousekeepingStatus {
        counts,
        last_repack: marker.last_repack,
        repack_ran: false,
    })
}

struct HousekeepingCheck {
    git_dir: PathBuf,
    counts: ObjectCounts,
    marker: HousekeepingMarker,
    last_repack_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
}

fn housekeeping_check(layout: &VaultLayout) -> Result<HousekeepingCheck, VaultError> {
    let git_dir = layout.git_dir_path();
    let counts = count_objects(&git_dir)?;
    let marker = read_marker(&layout.vault_dir)?;
    Ok(HousekeepingCheck {
        git_dir,
        counts,
        last_repack_at: last_repack_at(&marker),
        marker,
        now: Utc::now(),
    })
}

fn finish_without_repack(
    layout: &VaultLayout,
    check: &HousekeepingCheck,
) -> Result<HousekeepingStatus, VaultError> {
    let last_repack = check.marker.last_repack.clone();
    persist_marker(layout, check, check.counts, last_repack.clone())?;
    Ok(HousekeepingStatus {
        counts: check.counts,
        last_repack,
        repack_ran: false,
    })
}

fn finish_with_repack(
    layout: &VaultLayout,
    check: &HousekeepingCheck,
) -> Result<HousekeepingStatus, VaultError> {
    let outcome = repack(&check.git_dir)?;
    let counts = count_objects(&check.git_dir)?;
    let record = repack_record_from_outcome(&outcome);
    persist_marker(layout, check, counts, Some(record.clone()))?;
    Ok(HousekeepingStatus {
        counts,
        last_repack: Some(record),
        repack_ran: true,
    })
}

fn persist_marker(
    layout: &VaultLayout,
    check: &HousekeepingCheck,
    counts: ObjectCounts,
    last_repack: Option<RepackRecord>,
) -> Result<(), VaultError> {
    write_marker(
        &layout.vault_dir,
        &HousekeepingMarker {
            checked_at: check.now.to_rfc3339(),
            counts,
            last_repack,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::TimeZone;
    use tempfile::TempDir;

    use super::*;
    use crate::adapters::GixObjectStore;
    use crate::config::VaultConfig;
    use crate::domain::{FileChange, FileEventKind, RelPath, VaultLayout};
    use crate::ports::ObjectStore;
    use crate::storage;

    #[test]
    fn is_due_respects_each_threshold() {
        let thresholds = GcConfig {
            loose_object_limit: 10,
            pack_limit: 3,
            max_age_secs: 3600,
        };
        let now = Utc.with_ymd_and_hms(2026, 8, 4, 12, 0, 0).unwrap();
        let recent = now - chrono::Duration::minutes(30);

        assert!(!is_due(
            ObjectCounts {
                loose: 10,
                packs: 3
            },
            Some(recent),
            &thresholds,
            now
        ));
        assert!(is_due(
            ObjectCounts {
                loose: 11,
                packs: 0
            },
            Some(recent),
            &thresholds,
            now
        ));
        assert!(is_due(
            ObjectCounts { loose: 0, packs: 4 },
            Some(recent),
            &thresholds,
            now
        ));
        assert!(is_due(
            ObjectCounts { loose: 0, packs: 0 },
            Some(now - chrono::Duration::seconds(3601)),
            &thresholds,
            now
        ));
        assert!(!is_due(
            ObjectCounts { loose: 5, packs: 1 },
            None,
            &thresholds,
            now
        ));
    }

    fn init_vault(dir: &TempDir) -> VaultLayout {
        let layout = VaultLayout::from_worktree(dir.path().to_path_buf());
        fs::create_dir_all(&layout.vault_dir).expect("mkdir vault");
        storage::git::init(&layout.git_dir_path(), &layout.worktree).expect("git init");
        storage::sqlite::init_meta_db(&layout.meta_db_path()).expect("sqlite init");
        VaultConfig::defaults()
            .write_to(&layout.config_path())
            .expect("write config");
        fs::write(layout.readme_path(), b"test").expect("readme");
        layout
    }

    #[test]
    fn maybe_run_repacks_when_loose_limit_low() {
        let dir = TempDir::new().expect("tempdir");
        let layout = init_vault(&dir);
        let store = GixObjectStore::open(&layout).expect("open store");
        for i in 0..3 {
            let rel = format!("f-{i}.md");
            fs::write(layout.worktree.join(&rel), b"x").expect("write");
            let changes = vec![FileChange {
                rel: RelPath::parse(&rel),
                kind: FileEventKind::Create,
            }];
            store.commit(&changes, "seed").expect("commit");
        }

        let gc = GcConfig {
            loose_object_limit: 1,
            ..GcConfig::default()
        };
        let status = maybe_run(&layout, &gc).expect("maybe_run");
        assert!(status.repack_ran);
        assert_eq!(status.counts.loose, 0);
        assert_eq!(status.counts.packs, 1);

        let status = maybe_run(&layout, &gc).expect("maybe_run again");
        assert!(!status.repack_ran);
    }
}
