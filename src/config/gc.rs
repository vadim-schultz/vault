//! Git housekeeping thresholds in `config.toml` (`[gc]`).

use serde::{Deserialize, Serialize};

/// Git housekeeping thresholds in `config.toml` (`[gc]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcConfig {
    /// Loose object count above which a repack is due (matches `gc.auto`, default 6700).
    #[serde(default = "GcConfig::default_loose_object_limit")]
    pub loose_object_limit: usize,
    /// Packfile count above which a repack is due (matches `gc.autopacklimit`, default 50).
    #[serde(default = "GcConfig::default_pack_limit")]
    pub pack_limit: usize,
    /// Seconds since the last repack after which a repack is due (weekly cadence).
    #[serde(default = "GcConfig::default_max_age_secs")]
    pub max_age_secs: u64,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            loose_object_limit: Self::DEFAULT_LOOSE_OBJECT_LIMIT,
            pack_limit: Self::DEFAULT_PACK_LIMIT,
            max_age_secs: Self::DEFAULT_MAX_AGE_SECS,
        }
    }
}

impl GcConfig {
    /// Default loose-object threshold (`gc.auto`).
    pub const DEFAULT_LOOSE_OBJECT_LIMIT: usize = 6700;

    /// Default packfile threshold (`gc.autopacklimit`).
    pub const DEFAULT_PACK_LIMIT: usize = 50;

    /// Default maximum seconds between repacks (7 days).
    pub const DEFAULT_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;

    const fn default_loose_object_limit() -> usize {
        Self::DEFAULT_LOOSE_OBJECT_LIMIT
    }

    const fn default_pack_limit() -> usize {
        Self::DEFAULT_PACK_LIMIT
    }

    const fn default_max_age_secs() -> u64 {
        Self::DEFAULT_MAX_AGE_SECS
    }

    /// Return the configured max age as a [`std::time::Duration`].
    #[must_use]
    pub fn max_age_duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.max_age_secs)
    }
}
