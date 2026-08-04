//! `vault status` command.

use std::fmt;

use anyhow::Result;

use crate::app::status::{self, DaemonStatus, StatusReport, VaultStatus};
use crate::cli::support::run_blocking;
use crate::ports::ServiceState;

/// Run `vault status`.
pub async fn run() -> Result<()> {
    let report = run_blocking(status::report_default).await?;
    println!("{report}");
    Ok(())
}

impl fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.running {
            let pid = self
                .heartbeat
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |h| h.pid.to_string());
            writeln!(f, "Daemon: running (pid {pid})")?;
        } else {
            writeln!(f, "Daemon: stopped")?;
        }
        writeln!(f, "Service: {}", service_state_label(self.service_state))?;
        if let Some(age) = self.heartbeat_age_secs {
            writeln!(f, "Heartbeat age: {age}s")?;
        }
        Ok(())
    }
}

impl fmt::Display for VaultStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.last_snapshot.as_deref().unwrap_or("never");
        let state = if self.root_exists { "ok" } else { "missing" };
        write!(
            f,
            "  {} [{state}] last snapshot: {snapshot}",
            self.root.display()
        )
    }
}

impl fmt::Display for StatusReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.daemon)?;
        write!(f, "Vaults: {}", self.vaults.len())?;
        for vault in &self.vaults {
            write!(f, "\n{vault}")?;
        }
        Ok(())
    }
}

fn service_state_label(state: ServiceState) -> &'static str {
    match state {
        ServiceState::Running => "running",
        ServiceState::Stopped => "stopped",
        ServiceState::Unsupported => "unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_renders_service_state() {
        let status = DaemonStatus {
            running: false,
            service_state: ServiceState::Unsupported,
            heartbeat: None,
            heartbeat_age_secs: None,
        };
        let output = format!("{status}");
        assert!(output.contains("Service: unsupported"));
    }
}
