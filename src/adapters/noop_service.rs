//! No-op service manager for tests and `VAULT_NO_SERVICE`.

use crate::error::VaultError;
use crate::ports::{ServiceManager, ServiceState};

/// Service manager that never starts anything.
pub struct NoopService;

impl ServiceManager for NoopService {
    fn start(&self) -> Result<(), VaultError> {
        Ok(())
    }

    fn state(&self) -> ServiceState {
        ServiceState::Unsupported
    }
}
