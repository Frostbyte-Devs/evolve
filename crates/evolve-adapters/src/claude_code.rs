//! Claude Code adapter.

use crate::signals::{ParsedSignal, SessionLog, SignalKind};
use crate::traits::{Adapter, AdapterDetection, AdapterError};
use async_trait::async_trait;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::AdapterId;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use tokio::fs;

const MANAGED_START: &str = "<!-- evolve:start -->";
const MANAGED_END: &str = "<!-- evolve:end -->";
const HOOK_MARKER: &str = "evolve record-claude-code";

/// Claude Code integration.
#[derive(Debug, Clone, Default)]
pub struct ClaudeCodeAdapter;

impl ClaudeCodeAdapter {
    /// Construct.
    pub fn new() -> Self {
        Self
    }

    fn settings_path(root: &Path) -> PathBuf {
        root.join(".claude").join("settings.json")
    }

    fn claude_md_path(root: &Path) -> PathBuf {
        root.join("CLAUDE.md")
    }

    /// Public for tests: build the `Stop` hook object inserted into settings.json.
    pub fn stop_hook_entry() -> Value {
        serde_json::json!({
            "type": "command",
            "command": HOOK_MARKER,
        })
    }

    /// Render a config into the markdown snippet that goes inside
    /// the managed section of CLAUDE.md.
    pub fn render_managed_section(config: &AgentConfig) -> String {
        let mut out = String::new();
        out.push_str("# Evolve-managed configuration\n\n");
        out.push_str("## System prompt prefix\n\n");
        out.push_str(&config.system_prompt_prefix);
        out.push_str("\n\n");
        if !config.behavioral_rules.is_empty() {
            out.push_str("## Behavioral rules\n\n");
            for rule in &config.behavioral_rules {
                out.push_str(&format!("- {rule}\n"));
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "## Response style\n\n{:?}\n\n",
            config.response_style
        ));
        out.push_str(&format!("## Model preference\n\n{:?}\n", config.model_pref));
        out
    }
}

#[async_trait]
impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::new("claude-code")
    }

    fn detect(&self, root: &Path) -> AdapterDetection {
        if root.join(".claude").is_dir()
            || root.join("CLAUDE.md").is_file()
            || root.join(".claude").join("settings.json").is_file()
        {
            AdapterDetection::Detected
        } else {
            AdapterDetection::NotDetected
        }
    }

    async fn install(&self, root: &Path, _config: &AgentConfig) -> Result<(), AdapterError> {
        let settings_path = Self::settings_path(root);
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut settings: Value = if settings_path.is_file() {
            let raw = fs::read_to_string(&settings_path).await?;
            if raw.trim().is_empty() {
                Value::Object(Map::new())
            } else {
                serde_json::from_str(&raw)?
            }
        } else {
            Value::Object(Map::new())
        };

        // Idempotency: scan hooks.Stop[] for an entry whose command contains
        // our marker. If present, skip.
        let hooks = settings
            .as_object_mut()
            .expect("settings is an object")
            .entry("hooks".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let hooks_obj = hooks
            .as_object_mut()
            .ok_or_else(|| AdapterError::Parse("hooks is not an object".into()))?;
        let stop = hooks_obj
            .entry("Stop".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        let stop_arr = stop
            .as_array_mut()
            .ok_or_else(|| AdapterError::Parse("hooks.Stop is not an array".into()))?;

        let already = stop_arr.iter().any(|entry| {
            entry
                .get("command")
                .and_then(|c| c.as_str())
                .map(|s| s.contains(HOOK_MARKER))
                .unwrap_or(false)
        });
        if !already {
            stop_arr.push(Self::stop_hook_entry());
        }

        let rendered = serde_json::to_string_pretty(&settings)?;
        fs::write(&settings_path, rendered).await?;
        Ok(())
    }

    async fn apply_config(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError> {
        let path = Self::claude_md_path(root);
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
        let path = match log {
            SessionLog::Transcript(p) => p,
            _ => return Err(AdapterError::Parse("expected Transcript log".into())),
        };
        let raw = fs::read_to_string(&path).await?;
        Ok(parse_transcript_lines(&raw))
    }

    async fn forget(&self, root: &Path) -> Result<(), AdapterError> {
        // Remove hook from settings.json (keep the file if other hooks exist).
        let settings_path = Self::settings_path(root);
        if settings_path.is_file() {
            let raw = fs::read_to_string(&settings_path).await?;
            if !raw.trim().is_empty() {
                let mut settings: Value = serde_json::from_str(&raw)?;
                if let Some(stop) = settings
                    .get_mut("hooks")
                    .and_then(|h| h.get_mut("Stop"))
                    .and_then(|s| s.as_array_mut())
                {
                    stop.retain(|entry| {
                        entry
                            .get("command")
                            .and_then(|c| c.as_str())
                            .map(|s| !s.contains(HOOK_MARKER))
                            .unwrap_or(true)
                    });
                }
                fs::write(&settings_path, serde_json::to_string_pretty(&settings)?).await?;
            }
        }

        // Strip managed section from CLAUDE.md.
        let md_path = Self::claude_md_path(root);
        if md_path.is_file() {
            let raw = fs::read_to_string(&md_path).await?;
            let stripped = strip_managed_section(&raw);
            fs::write(&md_path, stripped).await?;
        }
        Ok(())
    }
}

/// Replace (or insert) the managed section inside `existing`, returning the new content.
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
    // No existing markers — append.
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

/// Remove the managed section entirely, leaving the rest of the file.
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

/// Parse a Claude Code transcript (JSONL) into signals.
///
/// Each line is a JSON event. We recognize:
/// - `{ "type": "user", "text": "/clear" }` → `user_clear` signal (0.0)
/// - `{ "type": "user", "text": "<feedback>" }` matching regex → `user_feedback`
/// - `{ "type": "tool_use", "tool": "bash", ..., "exit_code": 0 }` → `tests_passed` if test-like command
fn parse_transcript_lines(raw: &str) -> Vec<ParsedSignal> {
    use regex::Regex;

    let negative = Regex::new(r"(?i)\b(redo|wrong|no,? that|try again|undo)\b").unwrap();
    let positive = Regex::new(r"(?i)\b(thanks|perfect|looks good|lgtm|nice)\b").unwrap();
    let test_cmd =
        Regex::new(r"(?i)\b(cargo test|pytest|npm test|jest|go test|cargo check|cargo clippy)\b")
            .unwrap();

    let mut signals = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(event): Result<Value, _> = serde_json::from_str(line) else {
            continue;
        };
        let kind = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match kind {
            "user" => {
                let text = event.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.trim() == "/clear" {
                    signals.push(ParsedSignal {
                        kind: SignalKind::Implicit,
                        source: "user_clear".into(),
                        value: 0.0,
                        payload_json: None,
                    });
                    continue;
                }
                if negative.is_match(text) {
                    signals.push(ParsedSignal {
                        kind: SignalKind::Implicit,
                        source: "user_feedback_negative".into(),
                        value: 0.3,
                        payload_json: None,
                    });
                }
                if positive.is_match(text) {
                    signals.push(ParsedSignal {
                        kind: SignalKind::Implicit,
                        source: "user_feedback_positive".into(),
                        value: 0.9,
                        payload_json: None,
                    });
                }
            }
            "tool_use" => {
                let tool = event.get("tool").and_then(|v| v.as_str()).unwrap_or("");
                if tool != "bash" {
                    continue;
                }
                let cmd = event.get("command").and_then(|v| v.as_str()).unwrap_or("");
                if !test_cmd.is_match(cmd) {
                    continue;
                }
                let exit = event
                    .get("exit_code")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(-1);
                signals.push(ParsedSignal {
                    kind: SignalKind::Implicit,
                    source: if exit == 0 {
                        "tests_passed".into()
                    } else {
                        "tests_failed".into()
                    },
                    value: if exit == 0 { 1.0 } else { 0.0 },
                    payload_json: None,
                });
            }
            _ => {}
        }
    }
    signals
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_config() -> AgentConfig {
        AgentConfig::default_for("claude-code")
    }

    #[tokio::test]
    async fn detect_recognizes_claude_md() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("CLAUDE.md"), "# test").unwrap();
        let adapter = ClaudeCodeAdapter::new();
        assert_eq!(adapter.detect(tmp.path()), AdapterDetection::Detected);
    }

    #[tokio::test]
    async fn detect_returns_not_detected_for_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        assert_eq!(adapter.detect(tmp.path()), AdapterDetection::NotDetected);
    }

    #[tokio::test]
    async fn install_adds_stop_hook_to_fresh_settings() {
        let tmp = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let raw =
            std::fs::read_to_string(tmp.path().join(".claude").join("settings.json")).unwrap();
        assert!(raw.contains(HOOK_MARKER));
    }

    #[tokio::test]
    async fn install_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let first =
            std::fs::read_to_string(tmp.path().join(".claude").join("settings.json")).unwrap();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let second =
            std::fs::read_to_string(tmp.path().join(".claude").join("settings.json")).unwrap();
        assert_eq!(
            first, second,
            "second install must not change settings.json"
        );
    }

    #[tokio::test]
    async fn install_preserves_unrelated_settings() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join(".claude");
        std::fs::create_dir_all(&dir).unwrap();
        let existing = r#"{"theme":"dark","permissions":{"allow":["Bash"]}}"#;
        std::fs::write(dir.join("settings.json"), existing).unwrap();
        let adapter = ClaudeCodeAdapter::new();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let raw = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(raw.contains("\"theme\""));
        assert!(raw.contains("\"permissions\""));
        assert!(raw.contains(HOOK_MARKER));
    }

    #[tokio::test]
    async fn apply_config_writes_managed_section_between_markers() {
        let tmp = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        adapter
            .apply_config(tmp.path(), &sample_config())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(raw.contains(MANAGED_START));
        assert!(raw.contains(MANAGED_END));
        assert!(raw.contains("System prompt prefix"));
    }

    #[tokio::test]
    async fn apply_config_preserves_user_content_outside_markers() {
        let tmp = TempDir::new().unwrap();
        let user_content = "# My own CLAUDE.md\n\nImportant project notes.\n";
        std::fs::write(tmp.path().join("CLAUDE.md"), user_content).unwrap();
        let adapter = ClaudeCodeAdapter::new();
        adapter
            .apply_config(tmp.path(), &sample_config())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(raw.contains("Important project notes."));
        assert!(raw.contains(MANAGED_START));
    }

    #[tokio::test]
    async fn apply_config_replaces_existing_managed_section() {
        let tmp = TempDir::new().unwrap();
        let initial =
            format!("# Keep\n\n{MANAGED_START}\nold content\n{MANAGED_END}\n\n# Also keep\n",);
        std::fs::write(tmp.path().join("CLAUDE.md"), &initial).unwrap();
        let adapter = ClaudeCodeAdapter::new();
        adapter
            .apply_config(tmp.path(), &sample_config())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(tmp.path().join("CLAUDE.md")).unwrap();
        assert!(!raw.contains("old content"));
        assert!(raw.contains("# Keep"));
        assert!(raw.contains("# Also keep"));
    }

    #[tokio::test]
    async fn forget_removes_hook_but_keeps_other_hooks() {
        let tmp = TempDir::new().unwrap();
        let adapter = ClaudeCodeAdapter::new();
        // Seed with a foreign hook + evolve hook.
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let path = tmp.path().join(".claude").join("settings.json");
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut settings: Value = serde_json::from_str(&raw).unwrap();
        settings["hooks"]["Stop"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"type":"command","command":"other-thing"}));
        std::fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();

        adapter.forget(tmp.path()).await.unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(!after.contains(HOOK_MARKER));
        assert!(after.contains("other-thing"));
    }

    #[tokio::test]
    async fn forget_strips_managed_section_preserves_user_text() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("CLAUDE.md");
        let content = format!("# User\n\n{MANAGED_START}\nmanaged\n{MANAGED_END}\n\n# Tail\n",);
        std::fs::write(&path, &content).unwrap();
        ClaudeCodeAdapter::new().forget(tmp.path()).await.unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# User"));
        assert!(after.contains("# Tail"));
        assert!(!after.contains("managed"));
    }

    // ----- transcript parsing -----

    fn jsonl(events: &[&str]) -> String {
        events.join("\n")
    }

    #[tokio::test]
    async fn parse_session_detects_user_clear() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("t.jsonl");
        std::fs::write(&path, jsonl(&[r#"{"type":"user","text":"/clear"}"#])).unwrap();
        let signals = ClaudeCodeAdapter::new()
            .parse_session(SessionLog::Transcript(path))
            .await
            .unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].source, "user_clear");
        assert_eq!(signals[0].value, 0.0);
    }

    #[tokio::test]
    async fn parse_session_detects_test_pass_and_fail() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("t.jsonl");
        std::fs::write(
            &path,
            jsonl(&[
                r#"{"type":"tool_use","tool":"bash","command":"cargo test","exit_code":0}"#,
                r#"{"type":"tool_use","tool":"bash","command":"cargo test","exit_code":1}"#,
            ]),
        )
        .unwrap();
        let signals = ClaudeCodeAdapter::new()
            .parse_session(SessionLog::Transcript(path))
            .await
            .unwrap();
        assert_eq!(signals.len(), 2);
        assert_eq!(signals[0].source, "tests_passed");
        assert_eq!(signals[0].value, 1.0);
        assert_eq!(signals[1].source, "tests_failed");
        assert_eq!(signals[1].value, 0.0);
    }

    #[tokio::test]
    async fn parse_session_detects_positive_and_negative_feedback() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("t.jsonl");
        std::fs::write(
            &path,
            jsonl(&[
                r#"{"type":"user","text":"perfect, thanks!"}"#,
                r#"{"type":"user","text":"no, that's wrong, redo"}"#,
            ]),
        )
        .unwrap();
        let signals = ClaudeCodeAdapter::new()
            .parse_session(SessionLog::Transcript(path))
            .await
            .unwrap();
        let sources: Vec<&str> = signals.iter().map(|s| s.source.as_str()).collect();
        assert!(sources.contains(&"user_feedback_positive"));
        assert!(sources.contains(&"user_feedback_negative"));
    }

    #[tokio::test]
    async fn parse_session_ignores_unrelated_bash_commands() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("t.jsonl");
        std::fs::write(
            &path,
            jsonl(&[r#"{"type":"tool_use","tool":"bash","command":"ls -la","exit_code":0}"#]),
        )
        .unwrap();
        let signals = ClaudeCodeAdapter::new()
            .parse_session(SessionLog::Transcript(path))
            .await
            .unwrap();
        assert!(signals.is_empty());
    }

    #[tokio::test]
    async fn parse_session_tolerates_invalid_json_lines() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("t.jsonl");
        std::fs::write(
            &path,
            "not json\n{\"type\":\"user\",\"text\":\"/clear\"}\nalso not json",
        )
        .unwrap();
        let signals = ClaudeCodeAdapter::new()
            .parse_session(SessionLog::Transcript(path))
            .await
            .unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].source, "user_clear");
    }
}
