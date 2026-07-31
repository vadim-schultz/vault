//! Constants for OS service manager integration.

/// systemd user unit filename.
pub const SYSTEMD_UNIT_FILE: &str = "vault-watcher.service";

/// Relative path from the user home directory to the systemd user unit file.
pub const SYSTEMD_USER_UNIT_REL_PATH: &str = ".config/systemd/user/vault-watcher.service";
