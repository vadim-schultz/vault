//! Automatic version history — init once, retrieve any version later.

pub mod cli;
pub mod config;
pub mod daemon;
pub mod error;
pub mod ignore;
pub mod init;
pub mod paths;
pub mod registry;
pub mod service;
pub mod snapshot;
pub mod status;
pub mod storage;
pub mod walk;
pub mod watcher;

pub use cli::run;
