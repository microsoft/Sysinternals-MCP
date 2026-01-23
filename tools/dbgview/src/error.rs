//! Error types for the dbgview library

use thiserror::Error;

/// Result type alias for dbgview operations
pub type Result<T> = std::result::Result<T, DbgViewError>;

/// Errors that can occur during debug capture operations
#[derive(Debug, Error)]
pub enum DbgViewError {
    #[error("Failed to create kernel object '{name}': {source}")]
    KernelObjectCreation {
        name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Another debugger is already attached")]
    DebuggerAlreadyAttached,

    #[error("Failed to map shared memory: {0}")]
    MemoryMapping(std::io::Error),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid regex pattern: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("Capture not running")]
    CaptureNotRunning,

    #[error("Capture already running")]
    CaptureAlreadyRunning,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Platform not supported - Windows only")]
    PlatformNotSupported,
}
