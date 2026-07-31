//! OS service manager adapters for the singleton daemon.

mod constants;
mod systemd;
mod unsupported;

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

/// Install and start the singleton vault daemon.
pub trait ServiceManager: Send + Sync {
    /// Install the service unit when missing.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Service`] on failure.
    fn install(&self) -> Result<(), VaultError>;

    /// Ensure the daemon service is running.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::Service`] on failure.
    fn ensure_running(&self) -> Result<(), VaultError>;

    /// Return the current service state.
    fn state(&self) -> ServiceState;
}

/// Return the service manager for the current platform.
#[must_use]
pub fn for_current_platform() -> Box<dyn ServiceManager> {
    if systemd::is_available() {
        Box::new(systemd::SystemdService)
    } else {
        Box::new(unsupported::UnsupportedService)
    }
}
