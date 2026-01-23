//! Ring buffer for storing captured debug entries

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Default ring buffer capacity (number of entries)
pub const DEFAULT_BUFFER_SIZE: usize = 100_000;

/// A single captured debug output entry
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DebugEntry {
    /// Sequence number (monotonically increasing)
    pub seq: u64,
    /// Timestamp in Windows FILETIME format (100-nanosecond intervals since 1601)
    pub time: u64,
    /// Process ID that emitted the debug output
    pub pid: u32,
    /// Process name (cached at capture time)
    pub process_name: String,
    /// The debug message text
    pub text: String,
}

/// Thread-safe ring buffer for debug entries
pub struct RingBuffer {
    /// The actual buffer storage
    entries: RwLock<Vec<Option<DebugEntry>>>,
    /// Current write position
    write_pos: AtomicU64,
    /// Next sequence number to assign
    next_seq: AtomicU64,
    /// Buffer capacity
    capacity: usize,
    /// Process name cache (PID -> name)
    process_cache: RwLock<HashMap<u32, String>>,
}

impl RingBuffer {
    /// Create a new ring buffer with the specified capacity
    pub fn new(capacity: usize) -> Self {
        let mut entries = Vec::with_capacity(capacity);
        entries.resize_with(capacity, || None);

        Self {
            entries: RwLock::new(entries),
            write_pos: AtomicU64::new(0),
            next_seq: AtomicU64::new(1),
            capacity,
            process_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default capacity, respecting DBGVIEW_BUFFER_SIZE env var
    pub fn with_default_capacity() -> Self {
        let capacity = std::env::var("DBGVIEW_BUFFER_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_BUFFER_SIZE);
        Self::new(capacity)
    }

    /// Push a new entry into the buffer
    pub fn push(&self, pid: u32, text: String) {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let time = get_filetime();
        let process_name = self.get_process_name(pid);

        let entry = DebugEntry {
            seq,
            time,
            pid,
            process_name,
            text,
        };

        let pos = self.write_pos.fetch_add(1, Ordering::SeqCst) as usize % self.capacity;
        let mut entries = self.entries.write();
        entries[pos] = Some(entry);
    }

    /// Get the current sequence number (next to be written)
    pub fn current_seq(&self) -> u64 {
        self.next_seq.load(Ordering::SeqCst)
    }

    /// Get entries starting from a given sequence number
    /// Returns (entries, new_cursor) where new_cursor is the next seq to read from
    pub fn get_entries_from(&self, start_seq: u64, limit: usize) -> (Vec<DebugEntry>, u64) {
        let current_seq = self.next_seq.load(Ordering::SeqCst);
        
        // If start_seq is ahead of current, nothing to return
        if start_seq >= current_seq {
            return (Vec::new(), start_seq);
        }

        // Calculate the oldest available sequence
        let total_written = current_seq - 1;
        let oldest_available = if total_written >= self.capacity as u64 {
            total_written - self.capacity as u64 + 1
        } else {
            1
        };

        // Adjust start if it's too old
        let actual_start = start_seq.max(oldest_available);

        let entries = self.entries.read();
        let mut result = Vec::new();
        let mut last_seq = actual_start;

        for seq in actual_start..current_seq {
            if result.len() >= limit {
                break;
            }
            let pos = ((seq - 1) % self.capacity as u64) as usize;
            if let Some(entry) = &entries[pos] {
                if entry.seq == seq {
                    result.push(entry.clone());
                    last_seq = seq + 1;
                }
            }
        }

        (result, last_seq)
    }

    /// Get or cache process name for a PID
    fn get_process_name(&self, pid: u32) -> String {
        // Check cache first
        {
            let cache = self.process_cache.read();
            if let Some(name) = cache.get(&pid) {
                return name.clone();
            }
        }

        // Look up process name
        let name = lookup_process_name(pid);

        // Cache it
        {
            let mut cache = self.process_cache.write();
            // Limit cache size
            if cache.len() > 10000 {
                cache.clear();
            }
            cache.insert(pid, name.clone());
        }

        name
    }

    /// Clear the process name cache
    pub fn clear_process_cache(&self) {
        self.process_cache.write().clear();
    }
}

/// Get current time as Windows FILETIME
fn get_filetime() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    // Windows FILETIME epoch is January 1, 1601
    // UNIX epoch is January 1, 1970
    // Difference is 11644473600 seconds
    const FILETIME_UNIX_DIFF: u64 = 11644473600;
    
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    
    let seconds = duration.as_secs() + FILETIME_UNIX_DIFF;
    let nanos = duration.subsec_nanos() as u64;
    
    // FILETIME is in 100-nanosecond intervals
    seconds * 10_000_000 + nanos / 100
}

/// Look up process name by PID using sysinfo
fn lookup_process_name(pid: u32) -> String {
    use sysinfo::{System, Pid};
    
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    
    sys.process(Pid::from_u32(pid))
        .map(|p| p.name().to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("<{}>", pid))
}

/// Shared reference to the ring buffer
pub type SharedRingBuffer = Arc<RingBuffer>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_basic() {
        let buffer = RingBuffer::new(10);
        
        buffer.push(1234, "Hello".to_string());
        buffer.push(1234, "World".to_string());
        
        let (entries, cursor) = buffer.get_entries_from(1, 100);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "Hello");
        assert_eq!(entries[1].text, "World");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn test_ring_buffer_wrap() {
        let buffer = RingBuffer::new(5);
        
        for i in 0..10 {
            buffer.push(1234, format!("Message {}", i));
        }
        
        // Only last 5 should be available
        let (entries, _) = buffer.get_entries_from(1, 100);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].text, "Message 5");
        assert_eq!(entries[4].text, "Message 9");
    }
}
