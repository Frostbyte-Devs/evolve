//! Universal `AgentConfig`: the unit that Evolve actually evolves.
//!
//! Each project has a current "champion" config and optionally an active "challenger".
//! When `P(challenger > champion) > 0.95` (Bayesian posterior over session outcomes),
//! the challenger is promoted.
//!
//! The struct is intentionally narrow at the universal level: only fields that *every*
//! adapter can usefully consume some interpretation of. Tool-specific knobs (Claude Code
//! hook configs, Cursor file globs, Aider edit-format selection) live in [`AgentConfig::extensions`]
//! as opaque per-adapter JSON blobs that the adapter itself knows how to unpack.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

/// The unit Evolve evolves. Each version is stored once in SQLite and referenced
/// by [`ConfigId`](crate::ids::ConfigId).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    /// Free-form prefix prepended to the agent's effective system prompt.
    pub system_prompt_prefix: String,
    /// Preferred model. Adapters interpret this against their own model catalog.
    pub model_pref: ModelPref,
    /// Behavioral rules the agent is asked to follow (e.g., "always run tests after edits").
    pub behavioral_rules: BTreeSet<String>,
    /// Tools/permissions the agent is allowed to use. Interpretation is per-adapter.
    pub tool_permissions: BTreeSet<String>,
    /// Whether the agent is asked to be terse, normal, or verbose in its responses.
    pub response_style: ResponseStyle,
    /// Per-tool extension fields. Keyed by adapter id. Opaque blob each adapter understands.
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// Preferred model. Adapters map this onto their own model catalogs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ModelPref {
    /// Anthropic Claude Opus tier.
    ClaudeOpus,
    /// Anthropic Claude Sonnet tier.
    ClaudeSonnet,
    /// Anthropic Claude Haiku tier.
    ClaudeHaiku,
    /// OpenAI GPT-4o.
    Gpt4o,
    /// OpenAI GPT-4o-mini.
    Gpt4oMini,
    /// Local Ollama model identified by tag (e.g., `"qwen2.5:7b"`).
    Ollama(String),
    /// Adapter picks whatever cheap model is available.
    AnyCheap,
}

/// How verbose / how much narration the agent should produce.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStyle {
    /// Minimal narration, code-first.
    Terse,
    /// Default — balanced explanation.
    Normal,
    /// Detailed, walks through reasoning.
    Verbose,
}

impl AgentConfig {
    /// Construct a sane starting `AgentConfig` for the given adapter.
    ///
    /// Accepts the adapter id as `&str` to avoid coupling this module to the adapter
    /// crate; in practice callers use `adapter_id.as_str()`.
    pub fn default_for(adapter_id: &str) -> Self {
        let (prefix, rules, perms, model) = match adapter_id {
            "claude-code" => (
                "You are working alongside the user in their codebase. Prefer small, \
                 verifiable edits over speculative refactors. Run tests after changes \
                 when feasible.",
                [
                    "always run tests after structural edits",
                    "ask before deleting files",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<_>>(),
                ["bash", "edit", "read", "grep", "glob"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<BTreeSet<_>>(),
                ModelPref::ClaudeSonnet,
            ),
            "cursor" => (
                "Generate suggestions that fit the surrounding code style. Prefer \
                 minimal diffs that keep the user in flow.",
                [
                    "match existing code style",
                    "do not invent new APIs without justification",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<_>>(),
                ["edit", "read"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<BTreeSet<_>>(),
                ModelPref::AnyCheap,
            ),
            "aider" => (
                "Apply edits as small, atomic git commits with conventional commit \
                 messages. Run lint and tests before considering a change complete.",
                [
                    "one logical change per commit",
                    "use conventional commit messages",
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<_>>(),
                ["edit", "read", "shell"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<BTreeSet<_>>(),
                ModelPref::ClaudeSonnet,
            ),
            _ => (
                "You are a careful, helpful coding assistant.",
                BTreeSet::new(),
                BTreeSet::new(),
                ModelPref::AnyCheap,
            ),
        };

        AgentConfig {
            system_prompt_prefix: prefix.to_string(),
            model_pref: model,
            behavioral_rules: rules,
            tool_permissions: perms,
            response_style: ResponseStyle::Normal,
            extensions: BTreeMap::new(),
        }
    }

    /// Stable hash used as a cache key and to detect "did anything change" cheaply.
    ///
    /// Hashes the canonical JSON form rather than the in-memory layout so that
    /// reordering of `BTreeMap`/`BTreeSet` (canonical) and field reordering on
    /// non-canonical inputs do not affect the result.
    pub fn fingerprint(&self) -> u64 {
        let json = serde_json::to_string(self).expect("AgentConfig serializes");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        json.hash(&mut hasher);
        hasher.finish()
    }

    /// Read a typed extension blob registered under `key`.
    pub fn extension<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.extensions
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Insert or replace a typed extension blob under `key`.
    pub fn set_extension<T: Serialize>(&mut self, key: &str, value: &T) -> serde_json::Result<()> {
        let v = serde_json::to_value(value)?;
        self.extensions.insert(key.to_string(), v);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_serde_json() {
        let cfg = AgentConfig::default_for("claude-code");
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn fingerprint_stable_across_clones() {
        let cfg = AgentConfig::default_for("claude-code");
        let h1 = cfg.fingerprint();
        let h2 = cfg.clone().fingerprint();
        assert_eq!(h1, h2);
    }

    #[test]
    fn fingerprint_changes_when_prefix_changes() {
        let mut cfg = AgentConfig::default_for("claude-code");
        let h_before = cfg.fingerprint();
        cfg.system_prompt_prefix.push_str(" Extra clause.");
        let h_after = cfg.fingerprint();
        assert_ne!(h_before, h_after);
    }

    #[test]
    fn fingerprint_changes_when_model_changes() {
        let mut cfg = AgentConfig::default_for("claude-code");
        let h_before = cfg.fingerprint();
        cfg.model_pref = ModelPref::ClaudeOpus;
        let h_after = cfg.fingerprint();
        assert_ne!(h_before, h_after);
    }

    #[test]
    fn fingerprint_changes_when_a_rule_is_added() {
        let mut cfg = AgentConfig::default_for("claude-code");
        let h_before = cfg.fingerprint();
        cfg.behavioral_rules
            .insert("never edit .env files".to_string());
        let h_after = cfg.fingerprint();
        assert_ne!(h_before, h_after);
    }

    #[test]
    fn default_for_known_adapters_is_non_empty() {
        for adapter in ["claude-code", "cursor", "aider"] {
            let cfg = AgentConfig::default_for(adapter);
            assert!(
                !cfg.system_prompt_prefix.is_empty(),
                "{adapter} default has empty system_prompt_prefix",
            );
        }
    }

    #[test]
    fn default_for_unknown_adapter_returns_safe_fallback() {
        let cfg = AgentConfig::default_for("never-heard-of-it");
        assert!(!cfg.system_prompt_prefix.is_empty());
        assert!(cfg.behavioral_rules.is_empty());
        assert!(cfg.tool_permissions.is_empty());
    }

    #[test]
    fn extension_roundtrip_with_typed_payload() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct CursorExt {
            cursorrules_extra: String,
            file_glob_overrides: Vec<String>,
        }

        let mut cfg = AgentConfig::default_for("cursor");
        let ext = CursorExt {
            cursorrules_extra: "no inline scripts".to_string(),
            file_glob_overrides: vec!["**/*.tsx".to_string()],
        };
        cfg.set_extension("cursor", &ext).unwrap();

        let back: CursorExt = cfg.extension("cursor").unwrap();
        assert_eq!(ext, back);
    }

    #[test]
    fn extension_returns_none_when_key_absent() {
        let cfg = AgentConfig::default_for("claude-code");
        let value: Option<String> = cfg.extension("does-not-exist");
        assert!(value.is_none());
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::collection::{btree_map, btree_set};
    use proptest::prelude::*;

    fn arb_model_pref() -> impl Strategy<Value = ModelPref> {
        prop_oneof![
            Just(ModelPref::ClaudeOpus),
            Just(ModelPref::ClaudeSonnet),
            Just(ModelPref::ClaudeHaiku),
            Just(ModelPref::Gpt4o),
            Just(ModelPref::Gpt4oMini),
            Just(ModelPref::AnyCheap),
            "[a-z][a-z0-9.:-]{2,15}".prop_map(ModelPref::Ollama),
        ]
    }

    fn arb_response_style() -> impl Strategy<Value = ResponseStyle> {
        prop_oneof![
            Just(ResponseStyle::Terse),
            Just(ResponseStyle::Normal),
            Just(ResponseStyle::Verbose),
        ]
    }

    fn arb_agent_config() -> impl Strategy<Value = AgentConfig> {
        (
            "[ -~]{0,200}",                               // system_prompt_prefix
            arb_model_pref(),                             // model_pref
            btree_set("[a-z ]{1,30}", 0..6),              // behavioral_rules
            btree_set("[a-z_]{1,15}", 0..6),              // tool_permissions
            arb_response_style(),                         // response_style
            btree_map("[a-z]{1,10}", any::<i32>(), 0..3), // extensions (typed as i32 for simplicity)
        )
            .prop_map(|(prefix, model, rules, perms, style, ext_raw)| {
                let extensions = ext_raw
                    .into_iter()
                    .map(|(k, v)| (k, serde_json::Value::from(v)))
                    .collect();
                AgentConfig {
                    system_prompt_prefix: prefix,
                    model_pref: model,
                    behavioral_rules: rules,
                    tool_permissions: perms,
                    response_style: style,
                    extensions,
                }
            })
    }

    proptest! {
        #[test]
        fn json_roundtrip_is_identity(cfg in arb_agent_config()) {
            let json = serde_json::to_string(&cfg).unwrap();
            let back: AgentConfig = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(cfg, back);
        }

        #[test]
        fn fingerprint_invariant_under_json_roundtrip(cfg in arb_agent_config()) {
            let json = serde_json::to_string(&cfg).unwrap();
            let back: AgentConfig = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(cfg.fingerprint(), back.fingerprint());
        }

        #[test]
        fn equal_configs_have_equal_fingerprints(cfg in arb_agent_config()) {
            let twin = cfg.clone();
            prop_assert_eq!(cfg.fingerprint(), twin.fingerprint());
        }
    }
}
