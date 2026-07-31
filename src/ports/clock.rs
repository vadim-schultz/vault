//! Injectable wall clock.

use chrono::{DateTime, Utc};

/// Wall-clock abstraction for testable timestamps.
pub trait Clock: Send + Sync {
    /// Return the current UTC time.
    fn now(&self) -> DateTime<Utc>;
}
