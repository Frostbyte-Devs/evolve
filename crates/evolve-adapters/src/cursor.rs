//! Cursor adapter. Placeholder in Phase 6 — fleshed out in Phase 7.

use crate::signals::{ParsedSignal, SessionLog};
use crate::traits::{Adapter, AdapterDetection, AdapterError};
use async_trait::async_trait;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::AdapterId;
use std::path::Path;

/// Cursor integration.
#[derive(Debug, Clone, Default)]
pub struct CursorAdapter;

impl CursorAdapter {
    /// Construct.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Adapter for CursorAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::new("cursor")
    }

    fn detect(&self, _root: &Path) -> AdapterDetection {
        AdapterDetection::NotDetected
    }

    async fn install(&self, _root: &Path, _config: &AgentConfig) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn apply_config(&self, _root: &Path, _config: &AgentConfig) -> Result<(), AdapterError> {
        Ok(())
    }

    async fn parse_session(&self, _log: SessionLog) -> Result<Vec<ParsedSignal>, AdapterError> {
        Ok(Vec::new())
    }

    async fn forget(&self, _root: &Path) -> Result<(), AdapterError> {
        Ok(())
    }
}
