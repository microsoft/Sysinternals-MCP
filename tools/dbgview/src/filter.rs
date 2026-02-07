//! Filtering functionality for debug output

use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ring_buffer::DebugEntry;
use crate::error::{DbgViewError, Result};

/// A set of filters to apply to debug output
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FilterSet {
    /// Include patterns - entry must match at least one (if any specified)
    #[serde(default)]
    pub include_patterns: Vec<String>,
    /// Exclude patterns - matching entries are excluded
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Process name patterns - entry must match at least one (if any specified)
    #[serde(default)]
    pub process_names: Vec<String>,
    /// Specific process IDs to capture from
    #[serde(default)]
    pub process_pids: Vec<u32>,
}

/// Compiled filter set for efficient matching
pub struct CompiledFilters {
    include: Vec<Regex>,
    exclude: Vec<Regex>,
    process_names: Vec<Regex>,
    process_pids: Vec<u32>,
}

impl CompiledFilters {
    /// Compile a FilterSet into regex patterns
    pub fn compile(filter_set: &FilterSet) -> Result<Self> {
        let include = filter_set
            .include_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(DbgViewError::InvalidRegex)?;

        let exclude = filter_set
            .exclude_patterns
            .iter()
            .map(|p| Regex::new(p))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(DbgViewError::InvalidRegex)?;

        let process_names = filter_set
            .process_names
            .iter()
            .map(|p| Regex::new(&format!("(?i){}", p))) // Case-insensitive
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(DbgViewError::InvalidRegex)?;

        Ok(Self {
            include,
            exclude,
            process_names,
            process_pids: filter_set.process_pids.clone(),
        })
    }

    /// Check if an entry matches the filters
    pub fn matches(&self, entry: &DebugEntry) -> bool {
        // Check process PID filter
        if !self.process_pids.is_empty() && !self.process_pids.contains(&entry.pid) {
            return false;
        }

        // Check process name filter
        if !self.process_names.is_empty() {
            let name_matches = self.process_names.iter().any(|re| re.is_match(&entry.process_name));
            if !name_matches {
                return false;
            }
        }

        // Check exclude patterns first
        for re in &self.exclude {
            if re.is_match(&entry.text) {
                return false;
            }
        }

        // Check include patterns (if any specified, must match at least one)
        if !self.include.is_empty() {
            return self.include.iter().any(|re| re.is_match(&entry.text));
        }

        true
    }

    /// Filter a list of entries
    pub fn filter_entries(&self, entries: Vec<DebugEntry>) -> Vec<DebugEntry> {
        entries.into_iter().filter(|e| self.matches(e)).collect()
    }
}

impl Default for CompiledFilters {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            process_names: Vec::new(),
            process_pids: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(text: &str, process_name: &str, pid: u32) -> DebugEntry {
        DebugEntry {
            seq: 1,
            time: 0,
            pid,
            process_name: process_name.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn test_no_filters() {
        let filters = CompiledFilters::default();
        let entry = make_entry("Hello world", "test.exe", 1234);
        assert!(filters.matches(&entry));
    }

    #[test]
    fn test_include_filter() {
        let filter_set = FilterSet {
            include_patterns: vec!["ERROR".to_string()],
            ..Default::default()
        };
        let filters = CompiledFilters::compile(&filter_set).unwrap();
        
        assert!(filters.matches(&make_entry("ERROR: something", "test.exe", 1234)));
        assert!(!filters.matches(&make_entry("INFO: something", "test.exe", 1234)));
    }

    #[test]
    fn test_exclude_filter() {
        let filter_set = FilterSet {
            exclude_patterns: vec!["DEBUG".to_string()],
            ..Default::default()
        };
        let filters = CompiledFilters::compile(&filter_set).unwrap();
        
        assert!(!filters.matches(&make_entry("DEBUG: verbose", "test.exe", 1234)));
        assert!(filters.matches(&make_entry("ERROR: important", "test.exe", 1234)));
    }

    #[test]
    fn test_process_pid_filter() {
        let filter_set = FilterSet {
            process_pids: vec![1234],
            ..Default::default()
        };
        let filters = CompiledFilters::compile(&filter_set).unwrap();
        
        assert!(filters.matches(&make_entry("Hello", "test.exe", 1234)));
        assert!(!filters.matches(&make_entry("Hello", "other.exe", 5678)));
    }

    #[test]
    fn test_process_name_filter() {
        let filter_set = FilterSet {
            process_names: vec!["test".to_string()],
            ..Default::default()
        };
        let filters = CompiledFilters::compile(&filter_set).unwrap();
        
        assert!(filters.matches(&make_entry("Hello", "test.exe", 1234)));
        assert!(filters.matches(&make_entry("Hello", "TEST.EXE", 1234))); // Case insensitive
        assert!(!filters.matches(&make_entry("Hello", "other.exe", 5678)));
    }
}
