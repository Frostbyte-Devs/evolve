//! Aider adapter.
//!
//! Uses `aider.conf.yml` as the configuration surface and a git post-commit
//! hook for session signals. Sessions are identified by the commit SHA.

use crate::signals::{ParsedSignal, SessionLog, SignalKind};
use crate::traits::{Adapter, AdapterDetection, AdapterError};
use async_trait::async_trait;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::AdapterId;
use std::path::{Path, PathBuf};
use tokio::fs;

const MANAGED_START: &str = "# evolve:start";
const MANAGED_END: &str = "# evolve:end";
const HOOK_MARKER: &str = "evolve record-aider";

/// Aider integration.
#[derive(Debug, Clone, Default)]
pub struct AiderAdapter;

impl AiderAdapter {
    /// Construct.
    pub fn new() -> Self {
        Self
    }

    fn conf_path(root: &Path) -> PathBuf {
        root.join("aider.conf.yml")
    }

    fn post_commit_hook_path(root: &Path) -> PathBuf {
        root.join(".git").join("hooks").join("post-commit")
    }

    /// Render a config as the YAML commentary that goes inside the managed section.
    pub fn render_managed_section(config: &AgentConfig) -> String {
        let mut out = String::new();
        out.push_str("# System prompt prefix:\n");
        for line in config.system_prompt_prefix.lines() {
            out.push_str(&format!("#   {line}\n"));
        }
        if !config.behavioral_rules.is_empty() {
            out.push_str("# Behavioral rules:\n");
            for rule in &config.behavioral_rules {
                out.push_str(&format!("#   - {rule}\n"));
            }
        }
        out.push_str(&format!("# Response style: {:?}\n", config.response_style));
        out.push_str(&format!("# Model preference: {:?}\n", config.model_pref));
        out
    }
}

#[async_trait]
impl Adapter for AiderAdapter {
    fn id(&self) -> AdapterId {
        AdapterId::new("aider")
    }

    fn detect(&self, root: &Path) -> AdapterDetection {
        if root.join("aider.conf.yml").is_file()
            || root.join(".aider.conf.yml").is_file()
            || root.join(".aider.tags.cache.v3").exists()
        {
            AdapterDetection::Detected
        } else {
            AdapterDetection::NotDetected
        }
    }

    async fn install(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError> {
        // Write managed section into aider.conf.yml.
        self.apply_config(root, config).await?;

        // Install post-commit hook if .git/hooks exists.
        let hooks_dir = root.join(".git").join("hooks");
        if !hooks_dir.is_dir() {
            // Not a git repo — skip hook install but not an error.
            return Ok(());
        }
        let hook_path = Self::post_commit_hook_path(root);
        let existing = if hook_path.is_file() {
            fs::read_to_string(&hook_path).await?
        } else {
            "#!/bin/sh\n".to_string()
        };
        if existing.contains(HOOK_MARKER) {
            return Ok(());
        }
        let mut new_hook = existing.clone();
        if !new_hook.ends_with('\n') {
            new_hook.push('\n');
        }
        new_hook.push_str("# evolve:hook-start\n");
        new_hook.push_str(&format!("{HOOK_MARKER} HEAD >/dev/null 2>&1 || true\n"));
        new_hook.push_str("# evolve:hook-end\n");
        fs::write(&hook_path, new_hook).await?;
        Ok(())
    }

    async fn apply_config(&self, root: &Path, config: &AgentConfig) -> Result<(), AdapterError> {
        let path = Self::conf_path(root);
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
        let (_sha, project_root) = match log {
            SessionLog::GitCommit { sha, project_root } => (sha, project_root),
            _ => {
                return Err(AdapterError::Parse(
                    "aider adapter expects GitCommit log".into(),
                ));
            }
        };

        // No baseline signal: emitting a "neutral 0.5" for every commit was
        // misleading — it pulled the posterior towards 0.5 indefinitely for
        // users who didn't configure test-cmd / lint-cmd, so experiments
        // would Hold forever near indifference. Empty signal vec means the
        // session aggregates to 0.5 (neutral prior) once, which is correct
        // semantics for "no information".
        let mut signals = Vec::new();

        if let Some(root) = project_root.as_deref() {
            let cmds = read_aider_cmds(root).await.unwrap_or_default();
            let to = resolve_timeout(&cmds);
            if let Some(test_cmd) = cmds.test_cmd.as_deref() {
                signals.push(run_and_signal(root, test_cmd, "aider_tests", to).await);
            }
            if let Some(lint_cmd) = cmds.lint_cmd.as_deref() {
                signals.push(run_and_signal(root, lint_cmd, "aider_lint", to).await);
            }
        }
        Ok(signals)
    }

    async fn forget(&self, root: &Path) -> Result<(), AdapterError> {
        // Strip managed section from aider.conf.yml.
        let conf = Self::conf_path(root);
        if conf.is_file() {
            let raw = fs::read_to_string(&conf).await?;
            let stripped = strip_managed_section(&raw);
            if stripped.trim().is_empty() {
                fs::remove_file(&conf).await?;
            } else {
                fs::write(&conf, stripped).await?;
            }
        }
        // Remove hook block.
        let hook = Self::post_commit_hook_path(root);
        if hook.is_file() {
            let raw = fs::read_to_string(&hook).await?;
            let stripped = raw
                .lines()
                .filter(|line| !line.contains(HOOK_MARKER) && !line.contains("evolve:hook-"))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(&hook, stripped).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
struct AiderCmds {
    test_cmd: Option<String>,
    lint_cmd: Option<String>,
    /// Per-project timeout override (seconds). `None` means use the
    /// `EVOLVE_AIDER_TIMEOUT_SECS` env var, falling back to 300s.
    timeout_secs: Option<u64>,
}

const DEFAULT_AIDER_TIMEOUT_SECS: u64 = 300;

/// Read `test-cmd:` and `lint-cmd:` and `evolve-timeout-secs:` values from
/// `aider.conf.yml`. Minimal YAML parsing — looks only for `key: value` at
/// column 0.
async fn read_aider_cmds(root: &Path) -> Option<AiderCmds> {
    let conf = root.join("aider.conf.yml");
    if !conf.is_file() {
        return None;
    }
    let raw = fs::read_to_string(&conf).await.ok()?;
    let mut out = AiderCmds::default();
    for line in raw.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("test-cmd:") {
            out.test_cmd = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = trimmed.strip_prefix("lint-cmd:") {
            out.lint_cmd = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = trimmed.strip_prefix("evolve-timeout-secs:") {
            if let Ok(n) = rest.trim().parse::<u64>() {
                out.timeout_secs = Some(n);
            }
        }
    }
    Some(out)
}

fn resolve_timeout(cmds: &AiderCmds) -> u64 {
    if let Some(t) = cmds.timeout_secs {
        return t;
    }
    if let Ok(s) = std::env::var("EVOLVE_AIDER_TIMEOUT_SECS") {
        if let Ok(n) = s.parse::<u64>() {
            return n;
        }
    }
    DEFAULT_AIDER_TIMEOUT_SECS
}

/// Run `cmd` in `root` and return a signal based on exit code.
/// Timeout: `evolve-timeout-secs:` in aider.conf.yml, else
/// `EVOLVE_AIDER_TIMEOUT_SECS` env var, else 300s. Timeouts count as error.
async fn run_and_signal(
    root: &Path,
    cmd: &str,
    source_tag: &str,
    timeout_secs: u64,
) -> ParsedSignal {
    use tokio::process::Command;
    use tokio::time::{Duration, timeout};

    let output = timeout(
        Duration::from_secs(timeout_secs),
        if cfg!(windows) {
            Command::new("cmd")
                .arg("/C")
                .arg(cmd)
                .current_dir(root)
                .output()
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(root)
                .output()
        },
    )
    .await;

    let (value, source) = match output {
        Ok(Ok(o)) if o.status.success() => (1.0, format!("{source_tag}_passed")),
        Ok(Ok(_)) => (0.0, format!("{source_tag}_failed")),
        Ok(Err(_)) | Err(_) => (0.0, format!("{source_tag}_error")),
    };
    ParsedSignal {
        kind: SignalKind::Implicit,
        source,
        value,
        payload_json: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_config() -> AgentConfig {
        AgentConfig::default_for("aider")
    }

    #[tokio::test]
    async fn detect_recognizes_aider_conf() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("aider.conf.yml"), "model: gpt-4\n").unwrap();
        assert_eq!(
            AiderAdapter::new().detect(tmp.path()),
            AdapterDetection::Detected
        );
    }

    #[tokio::test]
    async fn apply_config_writes_managed_section() {
        let tmp = TempDir::new().unwrap();
        AiderAdapter::new()
            .apply_config(tmp.path(), &sample_config())
            .await
            .unwrap();
        let raw = std::fs::read_to_string(tmp.path().join("aider.conf.yml")).unwrap();
        assert!(raw.contains(MANAGED_START));
        assert!(raw.contains("Response style"));
    }

    #[tokio::test]
    async fn install_writes_post_commit_hook_when_git_repo() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git").join("hooks")).unwrap();
        AiderAdapter::new()
            .install(tmp.path(), &sample_config())
            .await
            .unwrap();
        let hook =
            std::fs::read_to_string(tmp.path().join(".git").join("hooks").join("post-commit"))
                .unwrap();
        assert!(hook.contains(HOOK_MARKER));
    }

    #[tokio::test]
    async fn install_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git").join("hooks")).unwrap();
        let adapter = AiderAdapter::new();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let once =
            std::fs::read_to_string(tmp.path().join(".git").join("hooks").join("post-commit"))
                .unwrap();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        let twice =
            std::fs::read_to_string(tmp.path().join(".git").join("hooks").join("post-commit"))
                .unwrap();
        assert_eq!(once, twice);
    }

    #[tokio::test]
    async fn parse_session_with_no_root_emits_no_signals() {
        let signals = AiderAdapter::new()
            .parse_session(SessionLog::GitCommit {
                sha: "abc123".into(),
                project_root: None,
            })
            .await
            .unwrap();
        // Without a project root we can't run test/lint cmds. Emit nothing
        // rather than a misleading 0.5 baseline.
        assert!(signals.is_empty());
    }

    #[tokio::test]
    async fn parse_session_runs_test_cmd_when_configured() {
        let tmp = TempDir::new().unwrap();
        let ok_cmd = if cfg!(windows) { "exit 0" } else { "true" };
        std::fs::write(
            tmp.path().join("aider.conf.yml"),
            format!("test-cmd: {ok_cmd}\n"),
        )
        .unwrap();
        let signals = AiderAdapter::new()
            .parse_session(SessionLog::GitCommit {
                sha: "deadbeef".into(),
                project_root: Some(tmp.path().to_path_buf()),
            })
            .await
            .unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].source, "aider_tests_passed");
        assert_eq!(signals[0].value, 1.0);
    }

    #[tokio::test]
    async fn parse_session_emits_failed_signal_when_test_cmd_fails() {
        let tmp = TempDir::new().unwrap();
        let fail_cmd = if cfg!(windows) { "exit 1" } else { "false" };
        std::fs::write(
            tmp.path().join("aider.conf.yml"),
            format!("test-cmd: {fail_cmd}\n"),
        )
        .unwrap();
        let signals = AiderAdapter::new()
            .parse_session(SessionLog::GitCommit {
                sha: "c0ffee".into(),
                project_root: Some(tmp.path().to_path_buf()),
            })
            .await
            .unwrap();
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].source, "aider_tests_failed");
        assert_eq!(signals[0].value, 0.0);
    }

    #[tokio::test]
    async fn parse_session_with_no_test_cmd_emits_no_signals() {
        let tmp = TempDir::new().unwrap();
        // aider.conf.yml exists but has no test-cmd / lint-cmd.
        std::fs::write(tmp.path().join("aider.conf.yml"), "model: gpt-4\n").unwrap();
        let signals = AiderAdapter::new()
            .parse_session(SessionLog::GitCommit {
                sha: "abc".into(),
                project_root: Some(tmp.path().to_path_buf()),
            })
            .await
            .unwrap();
        assert!(signals.is_empty());
    }

    #[tokio::test]
    async fn forget_removes_managed_section_and_hook() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git").join("hooks")).unwrap();
        let adapter = AiderAdapter::new();
        adapter.install(tmp.path(), &sample_config()).await.unwrap();
        adapter.forget(tmp.path()).await.unwrap();
        let hook =
            std::fs::read_to_string(tmp.path().join(".git").join("hooks").join("post-commit"))
                .unwrap();
        assert!(!hook.contains(HOOK_MARKER));
        // aider.conf.yml was created by install with only our section, so forget removes it
        assert!(!tmp.path().join("aider.conf.yml").exists());
    }
}
