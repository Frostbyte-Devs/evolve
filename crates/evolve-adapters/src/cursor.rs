//! Cursor adapter.
//!
//! Cursor doesn't expose a Stop hook, so for v1 we rely on:
//! - `.cursorrules` file for configuration (managed-section bracketed).
//! - A proxy-based signal-capture fallback (see `evolve-proxy`); adapter
//!   consumes proxy-emitted events via `parse_session(SessionLog::ProxyEvent)`.

use crate::signals::{ParsedSignal, SessionLog, SignalKind};
use crate::traits::{Adapter, AdapterDetection, AdapterError};
use async_trait::async_trait;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::AdapterId;
use std::path::{Path, PathBuf};
use tokio::fs;

const MANAGED_START: &str = "<!-- evolve:start -->";
const MANAGED_END: &str = "<!-- evolve:end -->";

/// Cursor integration.
#[derive(Debug, Clone, Default)]
pub struct CursorAdapter;

impl CursorAdapter {
    /// Construct.
    pub fn new() -> Self {
        Self
    }

    fn cursorrules_path(root: &Path) -> PathBuf {
        root.join(".cursorrules")
    }

    /// Render a config into the plain-text snippet for `.cursorrules`.
    pub fn render_managed_section(config: &AgentConfig) -> String {
        let mut out = String::new();
        out.push_str("# Evolve-managed rules\n\n");
        out.push_str(&config.system_prompt_prefix);
        out.push_str("\n\n");
        if !config.behavioral_rules.is_empty() {
            for rule in &config.behavioral_rules {
                out.push_str(&format!("- {rule}\n"));
            }
            out.push('\n');
        }
        out.push_str(&format!("Response style: {:?}\n", config.response_style));
        out
    }
}

#[async_trait]
impl Adapter for CursorAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::new("cursor")
    }

    fn detect(&self, root: &Path) -> AdapterDetection {
        if root.join(".cursorrules").is_file() || root.join(".vscode").is_dir() {
            AdapterDetection::Detected
        } else {
            AdapterDetection::NotDetected
        }
    }

    async fn install(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError> {
        // Writing a managed .cursorrules file is all there is to "install" for Cursor.
        self.apply_config(root, config).await
    }

    async fn apply_config(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError> {
        let path = Self::cursorrules_path(root);
        let existing = if path.is_file() {
            fs::read_to_string(&path).await?
        } else {
            String::new()
        };
        let new_section = Self::render_managed_section(config);
        let updated = replace_managed_section(&existing, &new_section);
        fs::write(&path, updated).await?;
        Ok(())
    }

    async fn parse_session(&self, log: SessionLog) -> Result<Vec<ParsedSignal>, AdapterError> {
        match log {
            SessionLog::ProxyEvent(event) => Ok(parse_proxy_event(&event)),
            _ => Err(AdapterError::Parse(
                "cursor adapter expects ProxyEvent logs".into(),
            )),
        }
    }

    async fn forget(&self, root: &Path) -> Result<(), AdapterError> {
        let path = Self::cursorrules_path(root);
        if path.is_file() {
            let raw = fs::read_to_string(&path).await?;
            let stripped = strip_managed_section(&raw);
            if stripped.trim().is_empty() {
                // If nothing but our section was there, remove the file entirely.
                fs::remove_file(&path).await?;
            } else {
                fs::write(&path, stripped).await?;
            }
        }
        Ok(())
    }
}

fn replace_managed_section(existing: &str, new_body: &str) -> String {
    let block = format!("{MANAGED_START}\n{}\n{MANAGED_END}", new_body.trim_end());
    if let (Some(start), Some(end)) = (existing.find(MANAGED_START), existing.find(MANAGED_END)) {
        if end > start {
            let end_full = end + MANAGED_END.len();
            let mut out = String::new();
            out.push_str(&existing[..start]);
            out.push_str(&block);
            out.push_str(&existing[end_full..]);
            return out;
        }
    }
    let mut out = String::from(existing);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&block);
    out.push('\n');
    out
}

fn strip_managed_section(existing: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(MANAGED_START), existing.find(MANAGED_END)) {
        if end > start {
            let end_full = end + MANAGED_END.len();
            let mut out = String::new();
            out.push_str(&existing[..start]);
            out.push_str(existing[end_full..].trim_start_matches('\n'));
            return out;
        }
    }
    existing.to_string()
}

/// Extract signals from a proxy-emitted event.
/// Schema: `{ "event": "suggestion_accepted" | "suggestion_rejected", "..." }`
fn parse_proxy_event(event: &serde_json::Value) -> Vec<ParsedSignal> {
    let event_type = event.get("event").and_then(|v| v.as_str()).unwrap_or("");
    match event_type {
        "suggestion_accepted" => vec![ParsedSignal {
            kind: SignalKind::Implicit,
            source: "cursor_suggestion_accepted".into(),
            value: 1.0,
            payload_json: None,
        }],
        "suggestion_rejected" => vec![ParsedSignal {
            kind: SignalKind::Implicit,
            source: "cursor_suggestion_rejected".into(),
            value: 0.0,
            payload_json: None,
        }],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_config() -> AgentConfig {
        AgentConfig::default_for("cursor")
    }

    #[tokio::test]
    async fn detect_recognizes_cursorrules() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".cursorrules"), "").unwrap();
        assert_eq!(
            CursorAdapter::new().detect(tmp.path()),
            AdapterDetection::Detected
        );
    }

    #[tokio::test]
    async fn apply_config_writes_managed_section() {
        let tmp = TempDir::new().unwrap();
        CursorAdapter::new()
            .apply_config(tmp.path(), &sample_config())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(".cursorrules")).unwrap();
        assert!(raw.contains(MANAGED_START));
        assert!(raw.contains("Evolve-managed rules"));
    }

    #[tokio::test]
    async fn apply_config_preserves_user_content() {
        let tmp = TempDir::new().unwrap();
        let user = "# my rules\nBe concise.\n";
        std::fs::write(tmp.path().join(".cursorrules"), user).unwrap();
        CursorAdapter::new()
            .apply_config(tmp.path(), &sample_config())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(".cursorrules")).unwrap();
        assert!(raw.contains("Be concise."));
        assert!(raw.contains(MANAGED_START));
    }

    #[tokio::test]
    async fn parse_session_extracts_accept_reject() {
        let accept = serde_json::json!({"event": "suggestion_accepted"});
        let signals = CursorAdapter::new()
            .parse_session(SessionLog::ProxyEvent(accept))
            .await
            .unwrap();
        assert_eq!(signals[0].value, 1.0);

        let reject = serde_json::json!({"event": "suggestion_rejected"});
        let signals = CursorAdapter::new()
            .parse_session(SessionLog::ProxyEvent(reject))
            .await
            .unwrap();
        assert_eq!(signals[0].value, 0.0);
    }

    #[tokio::test]
    async fn forget_removes_managed_section_keeps_user_content() {
        let tmp = TempDir::new().unwrap();
        let user_then_managed =
            format!("# user\nrules\n\n{MANAGED_START}\nmanaged\n{MANAGED_END}\n",);
        std::fs::write(tmp.path().join(".cursorrules"), &user_then_managed).unwrap();
        CursorAdapter::new().forget(tmp.path()).await.unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(".cursorrules")).unwrap();
        assert!(raw.contains("# user"));
        assert!(!raw.contains("managed"));
    }
}
