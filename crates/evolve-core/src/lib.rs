//! evolve-core: engine for evolutionary computation on LLM agents.
//!
//! See the workspace README and `docs/plans/2026-04-23-evolve-validation-design.md`
//! for the full architecture.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        assert_eq!(2 + 2, 4);
    }
}
