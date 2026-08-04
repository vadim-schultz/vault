//! Concrete adapter implementations.

pub mod detached_spawn;
pub mod fs_probe;
pub mod gix;
pub mod noop_service;
pub mod queue;
pub mod sqlite;
pub mod system_clock;
pub mod systemd;
pub mod toml_registry;

#[cfg(test)]
pub mod fakes;

pub use detached_spawn::DetachedSpawnService;
pub use fs_probe::probe_path;
pub use gix::GixObjectStore;
pub use noop_service::NoopService;
pub use queue::InMemoryQueueStore;
pub use sqlite::SqliteMetaIndex;
pub use system_clock::SystemClock;
pub use systemd::SystemdService;
pub use toml_registry::TomlRegistry;
