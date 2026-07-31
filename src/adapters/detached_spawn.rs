//! Detached-spawn service manager fallback.

use crate::daemon;
use crate::error::VaultError;
use crate::ports::{ServiceManager, ServiceState};

/// Spawn a detached `vault daemon` child when no service manager is available.
pub struct DetachedSpawnService;

impl ServiceManager for DetachedSpawnService {
    fn start(&self) -> Result<(), VaultError> {
        daemon::spawn_detached()
    }

    fn state(&self) -> ServiceState {
        ServiceState::Unsupported
    }
}
