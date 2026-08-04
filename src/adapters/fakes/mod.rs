//! Test fakes for ports.

mod clock;
mod meta_index;
mod object_store;
mod registry;
mod service;

pub use clock::FixedClock;
pub use meta_index::InMemoryMetaIndex;
pub use object_store::InMemoryObjectStore;
pub use registry::InMemoryRegistry;
pub use service::RecordingServiceManager;
