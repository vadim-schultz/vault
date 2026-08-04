//! Fixed clock for deterministic tests.

use chrono::{DateTime, Utc};

use crate::ports::Clock;

/// Fixed clock for deterministic tests.
pub struct FixedClock {
    instant: DateTime<Utc>,
}

impl FixedClock {
    /// Create a clock fixed at `instant`.
    #[must_use]
    pub fn at(instant: DateTime<Utc>) -> Self {
        Self { instant }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.instant
    }
}
