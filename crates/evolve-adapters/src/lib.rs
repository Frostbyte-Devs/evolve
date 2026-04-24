//! Adapter trait + per-tool integrations.
//!
//! Each adapter knows how to detect, install into, configure, and parse session
//! logs from one AI coding tool (Claude Code, Cursor, Aider).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod aider;
pub mod claude_code;
pub mod cursor;
pub mod registry;
pub mod signals;
pub mod traits;

pub use aider::AiderAdapter;
pub use claude_code::ClaudeCodeAdapter;
pub use cursor::CursorAdapter;
pub use registry::AdapterRegistry;
pub use signals::{ParsedSignal, SessionLog, SignalKind};
pub use traits::{Adapter, AdapterDetection, AdapterError};
