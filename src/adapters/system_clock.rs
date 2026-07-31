//! System wall clock adapter.

use chrono::{DateTime, Utc};

use crate::ports::Clock;

/// Production clock using `chrono::Utc::now()`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
