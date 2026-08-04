//! `vault status` command.

use std::fmt;

use anyhow::Result;

use crate::app::status::{self, DaemonStatus, QueueStatus, StatusReport, VaultStatus};
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
        if let Some(queue) = &self.queue {
            write!(f, "{queue}")?;
        }
        write!(f, "Vaults: {}", self.vaults.len())?;
        for vault in &self.vaults {
            write!(f, "\n{vault}")?;
        }
        Ok(())
    }
}

impl fmt::Display for QueueStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Queue: {} pending", self.tasks.len())?;
        for task in &self.tasks {
            writeln!(
                f,
                "  #{} {} [{}] attempts={}",
                task.id, task.kind, task.lane, task.attempts
            )?;
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

    #[test]
    fn status_renders_queue_snapshot() {
        let report = StatusReport {
            daemon: DaemonStatus {
                running: true,
                service_state: ServiceState::Unsupported,
                heartbeat: None,
                heartbeat_age_secs: None,
            },
            queue: Some(QueueStatus {
                updated_at: "2026-08-04T10:00:00Z".to_string(),
                tasks: vec![status::QueueTaskStatus {
                    id: 7,
                    kind: "reconcile_walk".to_string(),
                    lane: "default".to_string(),
                    attempts: 0,
                }],
            }),
            vaults: vec![],
        };
        let output = format!("{report}");
        assert!(output.contains("Queue: 1 pending"));
        assert!(output.contains("#7 reconcile_walk [default] attempts=0"));
    }
}
