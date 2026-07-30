//! Automatic version history — init once, retrieve any version later.

pub mod cli;
pub mod config;
pub mod error;
pub mod init;
pub mod paths;
pub mod storage;

pub use cli::run;
