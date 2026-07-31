#![allow(clippy::missing_errors_doc)]

//! OS service manager port.

use crate::error::VaultError;

/// Whether the daemon service is installed and active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    /// Service is running.
    Running,
    /// Service is installed but not running.
    Stopped,
    /// No service manager integration on this platform.
    Unsupported,
}

/// Start the singleton vault daemon via the OS service manager.
pub trait ServiceManager: Send + Sync {
    /// Start the daemon service (install unit if needed).
    fn start(&self) -> Result<(), VaultError>;

    /// Return the current service state.
    fn state(&self) -> ServiceState;
}
