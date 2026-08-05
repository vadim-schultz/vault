//! `vault status` command rendering.

use std::fmt;

use crate::app::status::{
    DaemonStatus, QueueStatus, StatusReport, VaultHousekeepingStatus, VaultStatus,
};
use crate::ports::ServiceState;

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
        writeln!(
            f,
            "  {} [{state}] last snapshot: {snapshot}",
            self.root.display()
        )?;
        if let Some(housekeeping) = &self.housekeeping {
            writeln!(f, "    housekeeping: {housekeeping}")?;
        }
        if !self.oversized.is_empty() {
            writeln!(f, "    oversized ({} not tracked):", self.oversized.len())?;
            for path in &self.oversized {
                writeln!(f, "      {}", path.as_str())?;
            }
        }
        Ok(())
    }
}

impl fmt::Display for VaultHousekeepingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} loose objects, {} pack",
            self.counts.loose, self.counts.packs
        )?;
        match &self.last_repack {
            Some(record) => {
                let reclaimed = record.bytes_before.saturating_sub(record.bytes_after);
                write!(
                    f,
                    " (last repack {}: packed {} objects, reclaimed ~{})",
                    record.ran_at,
                    record.objects_packed,
                    format_bytes(reclaimed)
                )
            }
            None => write!(f, " (never repacked)"),
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes >= MIB {
        format!("{} MB", bytes / MIB)
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} B")
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
    use crate::app::status;

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
    fn vault_status_renders_oversized_block_when_non_empty() {
        let status = VaultStatus {
            root: std::path::PathBuf::from("/vault"),
            registered_at: "2026-08-05T00:00:00Z".to_string(),
            last_snapshot: None,
            root_exists: true,
            housekeeping: None,
            oversized: vec![crate::domain::RelPath::parse("huge.bin")],
        };
        let output = format!("{status}");
        assert!(output.contains("oversized (1 not tracked):"));
        assert!(output.contains("huge.bin"));
    }

    #[test]
    fn vault_status_omits_oversized_block_when_empty() {
        let status = VaultStatus {
            root: std::path::PathBuf::from("/vault"),
            registered_at: "2026-08-05T00:00:00Z".to_string(),
            last_snapshot: None,
            root_exists: true,
            housekeeping: None,
            oversized: vec![],
        };
        let output = format!("{status}");
        assert!(!output.contains("oversized"));
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
