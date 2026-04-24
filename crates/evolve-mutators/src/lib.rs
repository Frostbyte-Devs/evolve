//! Mutation operators: take an [`AgentConfig`] and produce a varied "challenger".
//!
//! Each generation, [`MutatorPicker`] chooses ONE mutator probabilistically and
//! applies it. The result differs from the parent in exactly one axis.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use evolve_core::agent_config::{AgentConfig, ModelPref, ResponseStyle};
use evolve_llm::LlmClient;
use rand::Rng;
use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;
use thiserror::Error;

/// Mutator-specific error.
#[derive(Debug, Error)]
pub enum MutationError {
    /// The underlying LLM call failed.
    #[error("llm: {0}")]
    Llm(#[from] evolve_llm::LlmError),
    /// Parent config had no mutable surface for this operator.
    #[error("no mutable surface: {0}")]
    NoMutableSurface(&'static str),
}

/// Ambient context passed to every mutator invocation.
pub struct MutationCtx<'a> {
    /// LLM used by [`LlmRewriteMutator`]; other mutators ignore it.
    pub llm: &'a dyn LlmClient,
    /// Seeded RNG. Pass the same seed for reproducible mutations in tests.
    pub rng: &'a mut ChaCha8Rng,
}

/// Takes an [`AgentConfig`], returns a varied challenger.
#[async_trait]
pub trait Mutator: Send + Sync {
    /// Produce a challenger derived from `parent`.
    async fn mutate(
        &self,
        parent: &AgentConfig,
        ctx: &mut MutationCtx<'_>,
    ) -> Result<AgentConfig, MutationError>;

    /// Stable short name for logging and the picker's weight table.
    fn name(&self) -> &'static str;
}

/// Curated pool of behavioral rules the [`BehavioralRulesMutator`] draws from.
const RULE_POOL: &[&str] = &[
    "always run tests after structural edits",
    "ask before deleting files",
    "prefer small, verifiable edits over speculative refactors",
    "match existing code style",
    "do not invent new APIs without justification",
    "one logical change per commit",
    "use conventional commit messages",
    "never edit .env files",
    "confirm before installing new dependencies",
    "state the bug, show the fix, stop",
    "no speculative features",
    "three similar lines is better than a premature abstraction",
    "prefer editing over rewriting whole files",
    "run lint before considering a change complete",
    "avoid docstrings on code that did not change",
    "never skip hooks unless the user explicitly requests it",
    "never force-push to main",
    "restore unexpected uncommitted state, do not delete it",
    "investigate unfamiliar branches before discarding",
    "prefer the simplest working solution",
    "do not narrate your internal deliberation",
    "read the file before modifying it",
    "use offset/limit on reads for large files",
    "batch independent tool calls in parallel",
    "prefer bash for simple file existence checks",
    "summarize before proceeding when tool output is long",
    "do not create documentation unless explicitly requested",
    "match user's preferred commit message style",
    "stop after completing the requested task",
    "no sycophantic openers or trailing summaries",
];

/// Per-adapter catalog of models the [`ModelPrefMutator`] can swap to.
fn model_neighbors(current: &ModelPref) -> Vec<ModelPref> {
    use ModelPref::*;
    match current {
        ClaudeOpus | ClaudeSonnet | ClaudeHaiku => {
            vec![ClaudeOpus, ClaudeSonnet, ClaudeHaiku]
        }
        Gpt4o | Gpt4oMini => vec![Gpt4o, Gpt4oMini],
        Ollama(_) | AnyCheap => vec![
            AnyCheap,
            Ollama("qwen2.5-coder:7b".into()),
            Ollama("llama3.1:8b".into()),
        ],
    }
}

/// Per-adapter pool of tool permissions the [`ToolPermissionsMutator`] toggles.
const PERMISSION_POOL: &[&str] = &[
    "bash",
    "edit",
    "read",
    "grep",
    "glob",
    "shell",
    "web_fetch",
    "subagent",
];

/// Asks the LLM to propose a small variation of the system prompt prefix.
pub struct LlmRewriteMutator;

#[async_trait]
impl Mutator for LlmRewriteMutator {
    async fn mutate(
        &self,
        parent: &AgentConfig,
        ctx: &mut MutationCtx<'_>,
    ) -> Result<AgentConfig, MutationError> {
        let prompt = format!(
            "You are helping evolve a coding assistant's system prompt. The CURRENT prefix is:\n\
             ---\n{}\n---\n\
             Suggest a SMALL variation (1-2 clauses changed, or one clause added). \
             Output ONLY the new prefix, no prose, no quotes, no explanation.",
            parent.system_prompt_prefix,
        );
        let completion = ctx.llm.complete(&prompt, 400).await?;
        let text = completion.text.trim().to_string();
        if text.is_empty() || text == parent.system_prompt_prefix {
            return Err(MutationError::NoMutableSurface(
                "llm returned empty or identical prefix",
            ));
        }
        let mut child = parent.clone();
        child.system_prompt_prefix = text;
        Ok(child)
    }

    fn name(&self) -> &'static str {
        "llm_rewrite"
    }
}

/// Adds, removes, or rephrases ONE behavioral rule from the curated pool.
pub struct BehavioralRulesMutator;

#[async_trait]
impl Mutator for BehavioralRulesMutator {
    async fn mutate(
        &self,
        parent: &AgentConfig,
        ctx: &mut MutationCtx<'_>,
    ) -> Result<AgentConfig, MutationError> {
        let mut child = parent.clone();
        // Decide: add (50%), remove (30%), or swap (20%)
        let roll: f64 = ctx.rng.r#gen();
        if roll < 0.5 || child.behavioral_rules.is_empty() {
            // Add a rule from the pool that isn't already present.
            let fresh: Vec<&&str> = RULE_POOL
                .iter()
                .filter(|r| !child.behavioral_rules.contains(**r))
                .collect();
            if let Some(pick) = fresh.choose(ctx.rng) {
                child.behavioral_rules.insert((**pick).to_string());
                return Ok(child);
            }
            return Err(MutationError::NoMutableSurface(
                "rule pool exhausted for this config",
            ));
        }
        if roll < 0.8 {
            // Remove a random rule.
            let existing: Vec<String> = child.behavioral_rules.iter().cloned().collect();
            if let Some(to_remove) = existing.choose(ctx.rng) {
                child.behavioral_rules.remove(to_remove);
                return Ok(child);
            }
        }
        // Swap one rule for a pool rule not already present.
        let existing: Vec<String> = child.behavioral_rules.iter().cloned().collect();
        if let Some(to_remove) = existing.choose(ctx.rng) {
            let fresh: Vec<&&str> = RULE_POOL
                .iter()
                .filter(|r| !child.behavioral_rules.contains(**r))
                .collect();
            if let Some(to_add) = fresh.choose(ctx.rng) {
                child.behavioral_rules.remove(to_remove);
                child.behavioral_rules.insert((**to_add).to_string());
                return Ok(child);
            }
        }
        Err(MutationError::NoMutableSurface("could not rephrase a rule"))
    }

    fn name(&self) -> &'static str {
        "behavioral_rules"
    }
}

/// Cycles response style to a neighbor of the current one.
pub struct ResponseStyleMutator;

#[async_trait]
impl Mutator for ResponseStyleMutator {
    async fn mutate(
        &self,
        parent: &AgentConfig,
        ctx: &mut MutationCtx<'_>,
    ) -> Result<AgentConfig, MutationError> {
        let mut child = parent.clone();
        let options: Vec<ResponseStyle> = [
            ResponseStyle::Terse,
            ResponseStyle::Normal,
            ResponseStyle::Verbose,
        ]
        .into_iter()
        .filter(|s| *s != parent.response_style)
        .collect();
        let pick = options
            .choose(ctx.rng)
            .ok_or(MutationError::NoMutableSurface(
                "no alternative response style",
            ))?;
        child.response_style = *pick;
        Ok(child)
    }

    fn name(&self) -> &'static str {
        "response_style"
    }
}

/// Swaps model preference to a neighboring model within the adapter's pool.
pub struct ModelPrefMutator;

#[async_trait]
impl Mutator for ModelPrefMutator {
    async fn mutate(
        &self,
        parent: &AgentConfig,
        ctx: &mut MutationCtx<'_>,
    ) -> Result<AgentConfig, MutationError> {
        let mut child = parent.clone();
        let neighbors: Vec<ModelPref> = model_neighbors(&parent.model_pref)
            .into_iter()
            .filter(|m| *m != parent.model_pref)
            .collect();
        let pick = neighbors
            .choose(ctx.rng)
            .ok_or(MutationError::NoMutableSurface(
                "no neighboring model in the pool",
            ))?;
        child.model_pref = pick.clone();
        Ok(child)
    }

    fn name(&self) -> &'static str {
        "model_pref"
    }
}

/// Toggles one tool permission (add if absent, remove if present).
pub struct ToolPermissionsMutator;

#[async_trait]
impl Mutator for ToolPermissionsMutator {
    async fn mutate(
        &self,
        parent: &AgentConfig,
        ctx: &mut MutationCtx<'_>,
    ) -> Result<AgentConfig, MutationError> {
        let mut child = parent.clone();
        let pick = PERMISSION_POOL
            .choose(ctx.rng)
            .ok_or(MutationError::NoMutableSurface("permission pool empty"))?;
        if child.tool_permissions.contains(*pick) {
            child.tool_permissions.remove(*pick);
        } else {
            child.tool_permissions.insert((*pick).to_string());
        }
        Ok(child)
    }

    fn name(&self) -> &'static str {
        "tool_permissions"
    }
}

/// Weighted random selection of a mutator.
///
/// Default weights: llm_rewrite=50, behavioral_rules=15, response_style=15,
/// model_pref=10, tool_permissions=10.
pub struct MutatorPicker {
    entries: Vec<(Box<dyn Mutator>, u32)>,
}

impl Default for MutatorPicker {
    fn default() -> Self {
        Self {
            entries: vec![
                (Box::new(LlmRewriteMutator), 50),
                (Box::new(BehavioralRulesMutator), 15),
                (Box::new(ResponseStyleMutator), 15),
                (Box::new(ModelPrefMutator), 10),
                (Box::new(ToolPermissionsMutator), 10),
            ],
        }
    }
}

impl MutatorPicker {
    /// Construct with a custom set of (mutator, weight) entries.
    pub fn new(entries: Vec<(Box<dyn Mutator>, u32)>) -> Self {
        Self { entries }
    }

    /// Pick one mutator at random, weighted.
    pub fn pick(&self, rng: &mut ChaCha8Rng) -> &dyn Mutator {
        let total: u32 = self.entries.iter().map(|(_, w)| *w).sum();
        let mut threshold = rng.gen_range(0..total);
        for (mutator, weight) in &self.entries {
            if threshold < *weight {
                return mutator.as_ref();
            }
            threshold -= *weight;
        }
        // Unreachable — threshold < total by construction.
        self.entries[0].0.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evolve_llm::{CompletionResult, LlmError, TokenUsage};
    use rand::SeedableRng;

    /// Mock LLM that returns a pre-baked response.
    #[derive(Debug)]
    struct MockLlm {
        response: String,
    }

    #[async_trait]
    impl LlmClient for MockLlm {
        async fn complete(
            &self,
            _prompt: &str,
            _max_tokens: u32,
        ) -> Result<CompletionResult, LlmError> {
            Ok(CompletionResult {
                text: self.response.clone(),
                usage: TokenUsage {
                    input: 10,
                    output: 10,
                },
            })
        }

        fn model_id(&self) -> &str {
            "mock"
        }
    }

    fn rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(42)
    }

    fn parent() -> AgentConfig {
        AgentConfig::default_for("claude-code")
    }

    #[tokio::test]
    async fn llm_rewrite_changes_only_prefix() {
        let llm = MockLlm {
            response: "A completely different prefix proposed by the mock.".to_string(),
        };
        let mut r = rng();
        let mut ctx = MutationCtx {
            llm: &llm,
            rng: &mut r,
        };
        let p = parent();
        let child = LlmRewriteMutator.mutate(&p, &mut ctx).await.unwrap();
        assert_ne!(child.system_prompt_prefix, p.system_prompt_prefix);
        assert_eq!(child.model_pref, p.model_pref);
        assert_eq!(child.behavioral_rules, p.behavioral_rules);
    }

    #[tokio::test]
    async fn behavioral_rules_changes_only_rules() {
        let llm = MockLlm {
            response: "".into(),
        };
        let mut r = rng();
        let mut ctx = MutationCtx {
            llm: &llm,
            rng: &mut r,
        };
        let p = parent();
        let child = BehavioralRulesMutator.mutate(&p, &mut ctx).await.unwrap();
        assert_ne!(child.behavioral_rules, p.behavioral_rules);
        assert_eq!(child.system_prompt_prefix, p.system_prompt_prefix);
        assert_eq!(child.model_pref, p.model_pref);
    }

    #[tokio::test]
    async fn response_style_changes_only_style() {
        let llm = MockLlm {
            response: "".into(),
        };
        let mut r = rng();
        let mut ctx = MutationCtx {
            llm: &llm,
            rng: &mut r,
        };
        let p = parent();
        let child = ResponseStyleMutator.mutate(&p, &mut ctx).await.unwrap();
        assert_ne!(child.response_style, p.response_style);
        assert_eq!(child.system_prompt_prefix, p.system_prompt_prefix);
    }

    #[tokio::test]
    async fn model_pref_changes_only_model() {
        let llm = MockLlm {
            response: "".into(),
        };
        let mut r = rng();
        let mut ctx = MutationCtx {
            llm: &llm,
            rng: &mut r,
        };
        let p = parent();
        let child = ModelPrefMutator.mutate(&p, &mut ctx).await.unwrap();
        assert_ne!(child.model_pref, p.model_pref);
    }

    #[tokio::test]
    async fn tool_permissions_toggles_one_permission() {
        let llm = MockLlm {
            response: "".into(),
        };
        let mut r = rng();
        let mut ctx = MutationCtx {
            llm: &llm,
            rng: &mut r,
        };
        let p = parent();
        let child = ToolPermissionsMutator.mutate(&p, &mut ctx).await.unwrap();
        // Exactly one permission differs.
        let added: Vec<_> = child
            .tool_permissions
            .difference(&p.tool_permissions)
            .collect();
        let removed: Vec<_> = p
            .tool_permissions
            .difference(&child.tool_permissions)
            .collect();
        assert_eq!(added.len() + removed.len(), 1);
    }

    #[tokio::test]
    async fn picker_respects_weights_over_many_samples() {
        let picker = MutatorPicker::default();
        let mut r = rng();
        let mut counts = std::collections::HashMap::<&str, u32>::new();
        for _ in 0..1000 {
            let m = picker.pick(&mut r);
            *counts.entry(m.name()).or_insert(0) += 1;
        }
        // llm_rewrite has 50/100 weight → expect ~500 picks (±10%).
        let llm = *counts.get("llm_rewrite").unwrap_or(&0);
        assert!(
            (420..=580).contains(&llm),
            "llm_rewrite count {llm} out of expected band for weight=50",
        );
    }

    #[tokio::test]
    async fn picker_is_deterministic_under_seed() {
        let picker = MutatorPicker::default();
        let mut r1 = rng();
        let mut r2 = rng();
        let names1: Vec<_> = (0..20).map(|_| picker.pick(&mut r1).name()).collect();
        let names2: Vec<_> = (0..20).map(|_| picker.pick(&mut r2).name()).collect();
        assert_eq!(names1, names2);
    }
}
