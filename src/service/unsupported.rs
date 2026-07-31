//! Fallback when no OS service manager is available.

use crate::daemon;
use crate::error::VaultError;
use crate::service::{ServiceManager, ServiceState};

/// Service manager stub for unsupported platforms and CI.
pub struct UnsupportedService;

impl ServiceManager for UnsupportedService {
    fn install(&self) -> Result<(), VaultError> {
        Ok(())
    }

    fn ensure_running(&self) -> Result<(), VaultError> {
        if daemon::is_running() {
            return Ok(());
        }
        daemon::spawn_detached()
    }

    fn state(&self) -> ServiceState {
        ServiceState::Unsupported
    }
}
