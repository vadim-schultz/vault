//! Recording service manager fake.

use std::sync::Mutex;

use crate::error::VaultError;
use crate::ports::{ServiceManager, ServiceState};

/// Records service manager start calls.
pub struct RecordingServiceManager {
    pub starts: Mutex<usize>,
}

impl Default for RecordingServiceManager {
    fn default() -> Self {
        Self {
            starts: Mutex::new(0),
        }
    }
}

impl ServiceManager for RecordingServiceManager {
    fn start(&self) -> Result<(), VaultError> {
        *self.starts.lock().map_err(|_| VaultError::TaskPanicked)? += 1;
        Ok(())
    }

    fn state(&self) -> ServiceState {
        ServiceState::Stopped
    }
}
