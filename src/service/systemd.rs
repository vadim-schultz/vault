//! systemd user unit for the singleton vault daemon.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::daemon;
use crate::error::VaultError;
use crate::service::constants::{SYSTEMD_UNIT_FILE, SYSTEMD_USER_UNIT_REL_PATH};
use crate::service::{ServiceManager, ServiceState};

/// systemd user service manager.
pub struct SystemdService;

impl SystemdService {
    /// Return the unit file path.
    #[must_use]
    pub fn unit_path() -> PathBuf {
        directories::UserDirs::new().map_or_else(
            || PathBuf::from(SYSTEMD_UNIT_FILE),
            |dirs| dirs.home_dir().join(SYSTEMD_USER_UNIT_REL_PATH),
        )
    }

    /// Render the unit file contents.
    #[must_use]
    pub fn unit_contents(exe: &std::path::Path) -> String {
        format!(
            "[Unit]\n\
             Description=Vault document watcher\n\
             After=network.target\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart={} daemon --foreground\n\
             Restart=on-failure\n\n\
             [Install]\n\
             WantedBy=default.target\n",
            exe.display()
        )
    }
}

impl ServiceManager for SystemdService {
    fn install(&self) -> Result<(), VaultError> {
        let path = Self::unit_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let exe = std::env::current_exe()?;
        fs::write(&path, Self::unit_contents(&exe))?;
        run_systemctl(&["--user", "daemon-reload"])?;
        run_systemctl(&["--user", "enable", SYSTEMD_UNIT_FILE])?;
        Ok(())
    }

    fn ensure_running(&self) -> Result<(), VaultError> {
        if daemon::is_running() {
            return Ok(());
        }
        self.install()?;
        run_systemctl(&["--user", "start", SYSTEMD_UNIT_FILE])
    }

    fn state(&self) -> ServiceState {
        if !Self::is_available() {
            return ServiceState::Unsupported;
        }
        let output = Command::new("systemctl")
            .args(["--user", "is-active", SYSTEMD_UNIT_FILE])
            .output()
            .ok();
        match output
            .as_ref()
            .and_then(|o| String::from_utf8(o.stdout.clone()).ok())
        {
            Some(status) if status.trim() == "active" => ServiceState::Running,
            _ => ServiceState::Stopped,
        }
    }
}

impl SystemdService {
    /// Return whether systemd user services are available.
    #[must_use]
    pub fn is_available() -> bool {
        if Command::new("systemctl").arg("--version").output().is_err() {
            return false;
        }
        std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| {
            let private = std::path::Path::new(&dir).join("systemd/private");
            private.exists()
        })
    }
}

fn run_systemctl(args: &[&str]) -> Result<(), VaultError> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .map_err(|e| VaultError::Service(format!("failed to run systemctl: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(VaultError::Service(format!(
        "systemctl {} failed: {stderr}",
        args.join(" ")
    )))
}

/// Return whether systemd user services are available.
#[must_use]
pub fn is_available() -> bool {
    SystemdService::is_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_file_contains_exec_start() {
        let contents = SystemdService::unit_contents(std::path::Path::new("/usr/bin/vault"));
        assert!(contents.contains("ExecStart=/usr/bin/vault daemon --foreground"));
        assert!(contents.contains("Restart=on-failure"));
    }
}
