//! Process listing utilities

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sysinfo::{System, Pid};

/// Information about a running process
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// Parent process ID (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<u32>,
}

/// List running processes, optionally filtered by name pattern
pub fn list_processes(name_filter: Option<&str>) -> Vec<ProcessInfo> {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let filter_lower = name_filter.map(|s| s.to_lowercase());

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            let name = process.name().to_string_lossy().into_owned();
            
            // Apply name filter if specified
            if let Some(ref filter) = filter_lower {
                if !name.to_lowercase().contains(filter) {
                    return None;
                }
            }

            Some(ProcessInfo {
                pid: pid.as_u32(),
                name,
                parent_pid: process.parent().map(|p| p.as_u32()),
            })
        })
        .collect();

    // Sort by PID for consistent output
    processes.sort_by_key(|p| p.pid);
    processes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_processes() {
        let processes = list_processes(None);
        // Should at least have our own process
        assert!(!processes.is_empty());
    }

    #[test]
    fn test_list_processes_filtered() {
        // This test process should be running
        let processes = list_processes(Some("cargo"));
        // May or may not find cargo depending on how tests are run
        // Just ensure it doesn't crash
        let _ = processes;
    }
}
