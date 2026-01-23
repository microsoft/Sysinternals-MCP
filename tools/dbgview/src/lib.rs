//! Windows Debug Output Capture Library
//!
//! This library provides functionality to capture Windows debug output
//! (OutputDebugString) similar to Sysinternals DebugView.

mod capture;
mod error;
mod filter;
mod process;
mod ring_buffer;
mod session;

pub use capture::DebugCapture;
pub use error::{DbgViewError, Result};
pub use filter::FilterSet;
pub use process::{list_processes, ProcessInfo};
pub use ring_buffer::{DebugEntry, RingBuffer};
pub use session::{Session, SessionManager, SessionStatus};
