//! OS service manager adapters for the singleton daemon.

pub mod constants;

pub use crate::adapters::{DetachedSpawnService, NoopService, SystemdService};
pub use crate::ports::{ServiceManager, ServiceState};
