//! Session management for debug capture

use parking_lot::RwLock;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::capture::DebugCapture;
use crate::error::{DbgViewError, Result};
use crate::filter::{CompiledFilters, FilterSet};
use crate::ring_buffer::{DebugEntry, SharedRingBuffer};

/// Session status information
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionStatus {
    /// Session ID
    pub id: String,
    /// Session name
    pub name: String,
    /// Current filter configuration
    pub filters: FilterSet,
    /// Number of pending (unread) entries
    pub pending_count: u64,
    /// Whether capture is currently active
    pub capture_active: bool,
}

/// A capture session with its own cursor and filters
pub struct Session {
    /// Unique session ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Current filter set
    filters: RwLock<FilterSet>,
    /// Compiled filters for efficient matching
    compiled_filters: RwLock<CompiledFilters>,
    /// Read cursor (next sequence number to read)
    cursor: AtomicU64,
    /// Reference to shared ring buffer
    buffer: SharedRingBuffer,
}

impl Session {
    /// Create a new session
    pub fn new(id: String, name: String, buffer: SharedRingBuffer) -> Self {
        let cursor = buffer.current_seq();
        Self {
            id,
            name,
            filters: RwLock::new(FilterSet::default()),
            compiled_filters: RwLock::new(CompiledFilters::default()),
            cursor: AtomicU64::new(cursor),
            buffer,
        }
    }

    /// Set filters for this session
    pub fn set_filters(&self, filters: FilterSet) -> Result<()> {
        let compiled = CompiledFilters::compile(&filters)?;
        *self.filters.write() = filters;
        *self.compiled_filters.write() = compiled;
        Ok(())
    }

    /// Get current filters
    pub fn get_filters(&self) -> FilterSet {
        self.filters.read().clone()
    }

    /// Get filtered output entries
    pub fn get_output(&self, limit: usize) -> Vec<DebugEntry> {
        let cursor = self.cursor.load(Ordering::SeqCst);
        let (entries, new_cursor) = self.buffer.get_entries_from(cursor, limit * 10); // Fetch more to account for filtering
        
        let compiled = self.compiled_filters.read();
        let filtered: Vec<DebugEntry> = entries
            .into_iter()
            .filter(|e| compiled.matches(e))
            .take(limit)
            .collect();

        // Update cursor based on what we actually consumed from buffer
        if !filtered.is_empty() {
            let last_seq = filtered.last().map(|e| e.seq + 1).unwrap_or(cursor);
            self.cursor.store(last_seq.max(cursor), Ordering::SeqCst);
        } else if new_cursor > cursor {
            // No matches but we scanned entries, update cursor
            self.cursor.store(new_cursor, Ordering::SeqCst);
        }

        filtered
    }

    /// Clear session (move cursor to current position)
    pub fn clear(&self) {
        let current = self.buffer.current_seq();
        self.cursor.store(current, Ordering::SeqCst);
    }

    /// Get number of pending entries (approximate, unfiltered)
    pub fn pending_count(&self) -> u64 {
        let cursor = self.cursor.load(Ordering::SeqCst);
        let current = self.buffer.current_seq();
        current.saturating_sub(cursor)
    }

    /// Get session status
    pub fn status(&self, capture_active: bool) -> SessionStatus {
        SessionStatus {
            id: self.id.clone(),
            name: self.name.clone(),
            filters: self.get_filters(),
            pending_count: self.pending_count(),
            capture_active,
        }
    }
}

/// Manages multiple capture sessions
pub struct SessionManager {
    /// Active sessions
    sessions: RwLock<HashMap<String, Arc<Session>>>,
    /// Shared ring buffer
    buffer: SharedRingBuffer,
    /// Debug capture instance
    capture: RwLock<Option<DebugCapture>>,
    /// Session ID counter
    next_id: AtomicU64,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(buffer: SharedRingBuffer) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            buffer,
            capture: RwLock::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// Ensure capture is running (lazy start)
    fn ensure_capture_running(&self) -> Result<()> {
        let mut capture = self.capture.write();
        if capture.is_none() {
            let mut new_capture = DebugCapture::new(self.buffer.clone())?;
            new_capture.start()?;
            *capture = Some(new_capture);
        }
        Ok(())
    }

    /// Check if capture is active
    pub fn is_capture_active(&self) -> bool {
        self.capture.read().as_ref().map(|c| c.is_running()).unwrap_or(false)
    }

    /// Create a new session
    pub fn create_session(&self, name: Option<String>) -> Result<Arc<Session>> {
        // Start capture on first session
        self.ensure_capture_running()?;

        let id = format!("session_{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let name = name.unwrap_or_else(|| id.clone());
        let session = Arc::new(Session::new(id.clone(), name, self.buffer.clone()));
        
        self.sessions.write().insert(id, session.clone());
        Ok(session)
    }

    /// Get a session by ID
    pub fn get_session(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .read()
            .get(id)
            .cloned()
            .ok_or_else(|| DbgViewError::SessionNotFound(id.to_string()))
    }

    /// Destroy a session
    pub fn destroy_session(&self, id: &str) -> Result<()> {
        let removed = self.sessions.write().remove(id);
        if removed.is_some() {
            Ok(())
        } else {
            Err(DbgViewError::SessionNotFound(id.to_string()))
        }
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Vec<SessionStatus> {
        let capture_active = self.is_capture_active();
        self.sessions
            .read()
            .values()
            .map(|s| s.status(capture_active))
            .collect()
    }

    /// Stop capture and cleanup
    pub fn shutdown(&self) {
        if let Some(mut capture) = self.capture.write().take() {
            let _ = capture.stop();
        }
        self.sessions.write().clear();
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
