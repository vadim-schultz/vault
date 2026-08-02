//! CLI output formatting.

use std::fmt;

use crate::app::diff::DiffOutcome;
use crate::app::restore::RestoreOutcome;
use crate::app::status::{DaemonStatus, StatusReport, VaultStatus};
use crate::domain::{SnapshotEntry, TrackedFile};
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

pub fn log_report(entries: &[SnapshotEntry]) -> String {
    if entries.is_empty() {
        return "No snapshots yet.\n".to_string();
    }
    entries.iter().map(log_line).collect()
}

fn log_line(entry: &SnapshotEntry) -> String {
    match &entry.event {
        Some(event) => format!(
            "{} {} {}\n",
            entry.commit_sha.as_str(),
            entry.created_at,
            event.as_str()
        ),
        None => format!("{} {}\n", entry.commit_sha.as_str(), entry.created_at),
    }
}

pub fn list_report(files: &[TrackedFile]) -> String {
    if files.is_empty() {
        return "No tracked files.\n".to_string();
    }
    files.iter().map(list_line).collect()
}

fn list_line(file: &TrackedFile) -> String {
    format!("{}  {}\n", file.path.as_str(), file.last_modified)
}

pub fn restore_report(path: &std::path::Path, dry_run: bool, outcome: &RestoreOutcome) -> String {
    if dry_run {
        return format!("Would restore {} (dry run)", path.display());
    }
    match &outcome.commit_sha {
        Some(sha) => format!(
            "Restored {} ({} bytes, commit {})",
            path.display(),
            outcome.bytes_written,
            sha.as_str()
        ),
        None => format!("{} already matches that version", path.display()),
    }
}

pub fn diff_report(outcome: &DiffOutcome) -> String {
    if outcome.left == outcome.right {
        return "No differences.\n".to_string();
    }
    render_content_diff(outcome)
}

fn render_content_diff(outcome: &DiffOutcome) -> String {
    let Some((left_text, right_text)) = as_utf8_pair(outcome.left.as_ref(), outcome.right.as_ref())
    else {
        return "Binary files differ.\n".to_string();
    };
    similar::TextDiff::from_lines(left_text, right_text)
        .unified_diff()
        .header(&outcome.left_label, &outcome.right_label)
        .to_string()
}

fn as_utf8_pair<'a>(
    left: Option<&'a Vec<u8>>,
    right: Option<&'a Vec<u8>>,
) -> Option<(&'a str, &'a str)> {
    let left = std::str::from_utf8(left.map_or(&[][..], Vec::as_slice)).ok()?;
    let right = std::str::from_utf8(right.map_or(&[][..], Vec::as_slice)).ok()?;
    Some((left, right))
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
    use crate::app::status::DaemonStatus;

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
