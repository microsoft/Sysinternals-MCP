//! Tests for the filter module

use dbgview::{CompiledFilters, DebugEntry, FilterSet};

// ── Helpers ───────────────────────────────────────────────

fn entry(text: &str, process_name: &str, pid: u32) -> DebugEntry {
    DebugEntry {
        seq: 1,
        time: 0,
        pid,
        process_name: process_name.to_string(),
        text: text.to_string(),
    }
}

// ── No filters (pass-through) ─────────────────────────────

#[test]
fn no_filters_matches_everything() {
    let filters = CompiledFilters::default();
    assert!(filters.matches(&entry("anything", "any.exe", 1)));
    assert!(filters.matches(&entry("", "other.exe", 9999)));
}

#[test]
fn empty_filter_set_compiles_and_matches_all() {
    let fs = FilterSet::default();
    let compiled = CompiledFilters::compile(&fs).unwrap();
    assert!(compiled.matches(&entry("hello", "test.exe", 1)));
}

// ── Include patterns ──────────────────────────────────────

#[test]
fn include_pattern_matches() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("ERROR: something broke", "a.exe", 1)));
    assert!(f.matches(&entry("got an ERROR here", "a.exe", 1)));
    assert!(!f.matches(&entry("INFO: all good", "a.exe", 1)));
}

#[test]
fn include_pattern_any_match_is_sufficient() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR".to_string(), "WARN".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("ERROR: bad", "a.exe", 1)));
    assert!(f.matches(&entry("WARN: meh", "a.exe", 1)));
    assert!(!f.matches(&entry("INFO: fine", "a.exe", 1)));
}

#[test]
fn include_pattern_regex() {
    let fs = FilterSet {
        include_patterns: vec!["err(or)?".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("error occurred", "a.exe", 1)));
    assert!(f.matches(&entry("err happened", "a.exe", 1)));
    assert!(!f.matches(&entry("all good", "a.exe", 1)));
}

// ── Exclude patterns ──────────────────────────────────────

#[test]
fn exclude_pattern_rejects() {
    let fs = FilterSet {
        exclude_patterns: vec!["DEBUG".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(!f.matches(&entry("DEBUG: verbose stuff", "a.exe", 1)));
    assert!(f.matches(&entry("ERROR: important", "a.exe", 1)));
}

#[test]
fn exclude_takes_precedence_over_include() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        exclude_patterns: vec!["ignore-me".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("ERROR: real problem", "a.exe", 1)));
    assert!(!f.matches(&entry("ERROR: ignore-me", "a.exe", 1))); // excluded wins
    assert!(!f.matches(&entry("INFO: something", "a.exe", 1)));  // not included
}

#[test]
fn multiple_exclude_patterns() {
    let fs = FilterSet {
        exclude_patterns: vec!["DEBUG".to_string(), "TRACE".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(!f.matches(&entry("DEBUG: x", "a.exe", 1)));
    assert!(!f.matches(&entry("TRACE: y", "a.exe", 1)));
    assert!(f.matches(&entry("INFO: z", "a.exe", 1)));
}

// ── Process PID filter ────────────────────────────────────

#[test]
fn pid_filter_allows_matching_pid() {
    let fs = FilterSet {
        process_pids: vec![1234],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("hello", "a.exe", 1234)));
    assert!(!f.matches(&entry("hello", "a.exe", 5678)));
}

#[test]
fn multiple_pids() {
    let fs = FilterSet {
        process_pids: vec![100, 200, 300],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("x", "a.exe", 100)));
    assert!(f.matches(&entry("x", "a.exe", 200)));
    assert!(f.matches(&entry("x", "a.exe", 300)));
    assert!(!f.matches(&entry("x", "a.exe", 999)));
}

// ── Process name filter ───────────────────────────────────

#[test]
fn process_name_filter_case_insensitive() {
    let fs = FilterSet {
        process_names: vec!["test".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("x", "test.exe", 1)));
    assert!(f.matches(&entry("x", "TEST.EXE", 1)));
    assert!(f.matches(&entry("x", "Test.Exe", 1)));
    assert!(!f.matches(&entry("x", "other.exe", 1)));
}

#[test]
fn process_name_regex() {
    let fs = FilterSet {
        process_names: vec!["my.*app".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("x", "my-cool-app.exe", 1)));
    assert!(f.matches(&entry("x", "myapp.exe", 1)));
    assert!(!f.matches(&entry("x", "other.exe", 1)));
}

#[test]
fn multiple_process_names_any_match() {
    let fs = FilterSet {
        process_names: vec!["chrome".to_string(), "firefox".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("x", "chrome.exe", 1)));
    assert!(f.matches(&entry("x", "firefox.exe", 1)));
    assert!(!f.matches(&entry("x", "edge.exe", 1)));
}

// ── Combined filters ──────────────────────────────────────

#[test]
fn pid_and_include_combined() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        process_pids: vec![1234],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("ERROR: bad", "a.exe", 1234)));     // both match
    assert!(!f.matches(&entry("ERROR: bad", "a.exe", 9999)));    // wrong PID
    assert!(!f.matches(&entry("INFO: ok", "a.exe", 1234)));      // wrong text
}

#[test]
fn process_name_and_exclude_combined() {
    let fs = FilterSet {
        exclude_patterns: vec!["VERBOSE".to_string()],
        process_names: vec!["myapp".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    assert!(f.matches(&entry("ERROR: x", "myapp.exe", 1)));
    assert!(!f.matches(&entry("VERBOSE: x", "myapp.exe", 1)));  // excluded
    assert!(!f.matches(&entry("ERROR: x", "other.exe", 1)));    // wrong process
}

#[test]
fn all_filters_combined() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR|WARN".to_string()],
        exclude_patterns: vec!["ignore".to_string()],
        process_names: vec!["target".to_string()],
        process_pids: vec![42],
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    // Must pass all: correct PID, correct process name, match include, not match exclude
    assert!(f.matches(&entry("ERROR: real", "target.exe", 42)));
    assert!(f.matches(&entry("WARN: real", "target.exe", 42)));
    assert!(!f.matches(&entry("ERROR: ignore this", "target.exe", 42))); // excluded
    assert!(!f.matches(&entry("INFO: real", "target.exe", 42)));          // not included
    assert!(!f.matches(&entry("ERROR: real", "other.exe", 42)));          // wrong name
    assert!(!f.matches(&entry("ERROR: real", "target.exe", 99)));         // wrong PID
}

// ── filter_entries ────────────────────────────────────────

#[test]
fn filter_entries_batch() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        ..Default::default()
    };
    let f = CompiledFilters::compile(&fs).unwrap();

    let entries = vec![
        entry("ERROR: bad", "a.exe", 1),
        entry("INFO: ok", "a.exe", 1),
        entry("ERROR: worse", "a.exe", 1),
        entry("DEBUG: verbose", "a.exe", 1),
    ];

    let filtered = f.filter_entries(entries);
    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].text, "ERROR: bad");
    assert_eq!(filtered[1].text, "ERROR: worse");
}

#[test]
fn filter_entries_empty_input() {
    let f = CompiledFilters::default();
    let filtered = f.filter_entries(vec![]);
    assert!(filtered.is_empty());
}

// ── Invalid regex ─────────────────────────────────────────

#[test]
fn invalid_include_regex_returns_error() {
    let fs = FilterSet {
        include_patterns: vec!["[invalid".to_string()],
        ..Default::default()
    };
    assert!(CompiledFilters::compile(&fs).is_err());
}

#[test]
fn invalid_exclude_regex_returns_error() {
    let fs = FilterSet {
        exclude_patterns: vec!["(unclosed".to_string()],
        ..Default::default()
    };
    assert!(CompiledFilters::compile(&fs).is_err());
}

#[test]
fn invalid_process_name_regex_returns_error() {
    let fs = FilterSet {
        process_names: vec!["***".to_string()],
        ..Default::default()
    };
    assert!(CompiledFilters::compile(&fs).is_err());
}

// ── FilterSet serialization ───────────────────────────────

#[test]
fn filter_set_serializes_round_trip() {
    let fs = FilterSet {
        include_patterns: vec!["ERROR".to_string()],
        exclude_patterns: vec!["DEBUG".to_string()],
        process_names: vec!["myapp".to_string()],
        process_pids: vec![1234, 5678],
    };

    let json = serde_json::to_string(&fs).unwrap();
    let deserialized: FilterSet = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.include_patterns, fs.include_patterns);
    assert_eq!(deserialized.exclude_patterns, fs.exclude_patterns);
    assert_eq!(deserialized.process_names, fs.process_names);
    assert_eq!(deserialized.process_pids, fs.process_pids);
}

#[test]
fn filter_set_deserializes_with_defaults() {
    let json = r#"{}"#;
    let fs: FilterSet = serde_json::from_str(json).unwrap();
    assert!(fs.include_patterns.is_empty());
    assert!(fs.exclude_patterns.is_empty());
    assert!(fs.process_names.is_empty());
    assert!(fs.process_pids.is_empty());
}
