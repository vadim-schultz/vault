//! `vault status` report assembly (re-exports use-case types).

pub use crate::app::status::{
    report_default as report, DaemonStatus, QueueStatus, StatusReport, VaultStatus,
};
