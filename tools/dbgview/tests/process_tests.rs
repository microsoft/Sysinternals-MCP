//! Tests for the process listing module

use dbgview::{list_processes, ProcessInfo};

#[test]
fn list_all_processes_returns_nonempty() {
    let processes = list_processes(None);
    assert!(!processes.is_empty(), "Should find at least one running process");
}

#[test]
fn list_processes_sorted_by_pid() {
    let processes = list_processes(None);
    for window in processes.windows(2) {
        assert!(
            window[0].pid <= window[1].pid,
            "Processes should be sorted by PID: {} > {}",
            window[0].pid,
            window[1].pid
        );
    }
}

#[test]
fn list_processes_have_names() {
    let processes = list_processes(None);
    // At least some processes should have real names
    let named: Vec<_> = processes.iter().filter(|p| !p.name.is_empty()).collect();
    assert!(!named.is_empty(), "Some processes should have names");
}

#[test]
fn filter_by_nonexistent_process_returns_empty() {
    let processes = list_processes(Some("zzz_nonexistent_process_xyz_999"));
    assert!(processes.is_empty());
}

#[test]
fn filter_is_case_insensitive() {
    // Find a process, then search for it with different case
    let all = list_processes(None);
    if let Some(first) = all.first() {
        let name_upper = first.name.to_uppercase();
        let filtered = list_processes(Some(&name_upper));
        assert!(
            !filtered.is_empty(),
            "Case-insensitive filter for '{}' should find '{}'",
            name_upper,
            first.name
        );
    }
}

#[test]
fn filter_is_substring_match() {
    // Find a process with a name longer than 3 chars, search by substring
    let all = list_processes(None);
    if let Some(proc) = all.iter().find(|p| p.name.len() > 3) {
        let substring = &proc.name[..3];
        let filtered = list_processes(Some(substring));
        assert!(
            !filtered.is_empty(),
            "Substring '{}' should match '{}'",
            substring,
            proc.name
        );
    }
}

#[test]
fn process_info_has_pid() {
    let processes = list_processes(None);
    // All should have non-zero PIDs (except maybe System Idle on Windows, pid 0)
    let nonzero: Vec<_> = processes.iter().filter(|p| p.pid > 0).collect();
    assert!(!nonzero.is_empty());
}

#[test]
fn process_info_serializes() {
    let info = ProcessInfo {
        pid: 1234,
        name: "test.exe".to_string(),
        parent_pid: Some(5678),
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("\"pid\":1234"));
    assert!(json.contains("\"name\":\"test.exe\""));
    assert!(json.contains("\"parent_pid\":5678"));

    let roundtrip: ProcessInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(roundtrip.pid, 1234);
    assert_eq!(roundtrip.parent_pid, Some(5678));
}

#[test]
fn process_info_skips_none_parent_pid() {
    let info = ProcessInfo {
        pid: 1,
        name: "test.exe".to_string(),
        parent_pid: None,
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(!json.contains("parent_pid"), "None parent_pid should be skipped in serialization");
}
