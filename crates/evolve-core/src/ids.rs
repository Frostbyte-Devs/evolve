//! Strongly-typed identifier newtypes used across the framework.
//!
//! Each entity (project, agent config, experiment, session, signal) gets its own
//! UUID-backed newtype so that mixing them at a call site is a compile error rather
//! than a runtime bug. Adapters identify themselves by a stable string instead of
//! a UUID, since adapter identity is part of the public protocol.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! uuid_id {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generate a fresh random identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Construct from an existing UUID (useful when loading from storage).
            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            /// Borrow the inner UUID.
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

uuid_id!(
    /// Identifies a project that Evolve is managing.
    ProjectId
);
uuid_id!(
    /// Identifies an [`AgentConfig`](crate::agent_config::AgentConfig) row in storage.
    ConfigId
);
uuid_id!(
    /// Identifies an in-flight or completed champion-vs-challenger experiment.
    ExperimentId
);
uuid_id!(
    /// Identifies a single user session as recorded by an adapter hook.
    SessionId
);
uuid_id!(
    /// Identifies a single fitness signal contributed to a session.
    SignalId
);

/// Stable string identifier for a registered adapter (e.g., `"claude-code"`,
/// `"cursor"`, `"aider"`). String-based rather than UUID-based because adapter
/// identity is part of the public protocol — IDs must round-trip across machines
/// and across the wire.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AdapterId(String);

impl AdapterId {
    /// Construct an adapter id from a stable, lowercase, kebab-cased name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Borrow the inner name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AdapterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_id_roundtrips_through_json() {
        let id = ProjectId::new();
        let json = serde_json::to_string(&id).unwrap();
        let back: ProjectId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn distinct_ids_are_not_equal() {
        let a = ProjectId::new();
        let b = ProjectId::new();
        assert_ne!(a, b);
    }

    #[test]
    fn from_uuid_then_as_uuid_roundtrips() {
        let raw = Uuid::new_v4();
        let id = SessionId::from_uuid(raw);
        assert_eq!(id.as_uuid(), &raw);
    }

    #[test]
    fn display_matches_inner_uuid() {
        let raw = Uuid::new_v4();
        let id = ConfigId::from_uuid(raw);
        assert_eq!(format!("{id}"), format!("{raw}"));
    }

    #[test]
    fn adapter_id_roundtrips() {
        let id = AdapterId::new("claude-code");
        let json = serde_json::to_string(&id).unwrap();
        let back: AdapterId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
        assert_eq!(id.as_str(), "claude-code");
    }

    #[test]
    fn adapter_id_display_matches_inner_string() {
        let id = AdapterId::new("aider");
        assert_eq!(format!("{id}"), "aider");
    }

    #[test]
    fn distinct_id_types_are_not_interchangeable() {
        // This is enforced by the type system — there's nothing to assert at runtime.
        // The test exists to document the intent: the compiler MUST reject
        // `let _: ProjectId = ConfigId::new();`. Verified by the fact that this file
        // compiles only without that line.
        let _project = ProjectId::new();
        let _config = ConfigId::new();
    }
}
