//! Port traits (dependency inversion boundaries).

pub mod clock;
pub mod meta_index;
pub mod object_store;
pub mod registry;
pub mod service;

pub use clock::Clock;
pub use meta_index::MetaIndex;
pub use object_store::ObjectStore;
pub use registry::RegistryStore;
pub use service::{ServiceManager, ServiceState};
