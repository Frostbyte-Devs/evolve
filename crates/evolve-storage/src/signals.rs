//! Repository for the `signals` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{ConfigId, SessionId, SignalId};
use uuid::Uuid;

/// Source-of-truth for signal categorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// User explicitly graded the session (`evolve good/bad/thumbs`).
    Explicit,
    /// Inferred from adapter session log.
    Implicit,
}

impl SignalKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Implicit => "implicit",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "explicit" => Self::Explicit,
            "implicit" => Self::Implicit,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown signal kind {other:?}").into(),
                )));
            }
        })
    }
}

/// One fitness signal contributed to a session.
#[derive(Debug, Clone)]
pub struct Signal {
    /// Signal identity.
    pub id: SignalId,
    /// Owning session.
    pub session_id: SessionId,
    /// Explicit (user-provided) vs implicit (inferred).
    pub kind: SignalKind,
    /// Short tag for the source (e.g., `tests_passed`, `user_clear`).
    pub source: String,
    /// Normalized score in `[0.0, 1.0]`.
    pub value: f64,
    /// When the signal was recorded.
    pub recorded_at: DateTime<Utc>,
    /// Optional opaque JSON metadata. MUST NOT contain source code.
    pub payload_json: Option<String>,
}

/// Raw tuple shape returned by `SELECT`s on `signals`. Factored out to keep
/// call sites under `clippy::type_complexity`.
type SignalRow = (String, String, String, String, f64, String, Option<String>);

/// Repository for `signals`.
#[derive(Debug, Clone)]
pub struct SignalRepo<'a> {
    storage: &'a Storage,
}

impl<'a> SignalRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new signal. Rejects payloads that look like source code
    /// (privacy invariant; see design doc Section 7).
    pub async fn insert(&self, s: &Signal) -> Result<(), StorageError> {
        if let Some(payload) = s.payload_json.as_deref()
            && looks_like_source_code(payload)
        {
            return Err(StorageError::PayloadRejected(
                "payload contains code-like content",
            ));
        }
        sqlx::query(
            "INSERT INTO signals
                (id, session_id, kind, source, value, recorded_at, payload_json)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.id.to_string())
        .bind(s.session_id.to_string())
        .bind(s.kind.as_str())
        .bind(&s.source)
        .bind(s.value)
        .bind(s.recorded_at.to_rfc3339())
        .bind(s.payload_json.as_deref())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// All signals recorded for a session.
    pub async fn list_for_session(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<Signal>, StorageError> {
        let rows: Vec<SignalRow> = sqlx::query_as(
            "SELECT id, session_id, kind, source, value, recorded_at, payload_json
             FROM signals
             WHERE session_id = ?
             ORDER BY recorded_at ASC",
        )
        .bind(session_id.to_string())
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_signal).collect()
    }

    /// All signals recorded for sessions that used a given config.
    pub async fn list_for_config(&self, config_id: ConfigId) -> Result<Vec<Signal>, StorageError> {
        let rows: Vec<SignalRow> = sqlx::query_as(
            "SELECT sig.id, sig.session_id, sig.kind, sig.source, sig.value,
                    sig.recorded_at, sig.payload_json
             FROM signals sig
             JOIN sessions s ON s.id = sig.session_id
             WHERE s.config_id = ?
             ORDER BY sig.recorded_at ASC",
        )
        .bind(config_id.to_string())
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_signal).collect()
    }
}

/// Simple heuristic: reject payloads that contain tokens common in source code.
/// Intentionally conservative — false positives are preferable to leaking code.
fn looks_like_source_code(payload: &str) -> bool {
    const BANNED: &[&str] = &[
        "fn ",
        "def ",
        "class ",
        "function ",
        "=>",
        "import ",
        "#include",
        "public class",
        "console.log",
        "println!",
        "SELECT ",
        "INSERT INTO",
    ];
    BANNED.iter().any(|needle| payload.contains(needle))
}

fn row_to_signal(
    (id, session_id, kind, source, value, recorded_at, payload): SignalRow,
) -> Result<Signal, StorageError> {
    Ok(Signal {
        id: SignalId::from_uuid(Uuid::parse_str(&id)?),
        session_id: SessionId::from_uuid(Uuid::parse_str(&session_id)?),
        kind: SignalKind::from_str(&kind)?,
        source,
        value,
        recorded_at: DateTime::parse_from_rfc3339(&recorded_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
        payload_json: payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
    use crate::projects::{Project, ProjectRepo};
    use crate::sessions::{Session, SessionRepo, SessionVariant};
    use evolve_core::agent_config::AgentConfig;
    use evolve_core::ids::{AdapterId, ProjectId};

    async fn seeded() -> (Storage, SessionId, ConfigId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let pid = ProjectId::new();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: pid,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/signals-test".into(),
                name: "g".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        let cfg = AgentConfigRow {
            id: ConfigId::new(),
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: 1,
            payload: AgentConfig::default_for("claude-code"),
            created_at: Utc::now(),
        };
        AgentConfigRepo::new(&storage).insert(&cfg).await.unwrap();

        let sid = SessionId::new();
        SessionRepo::new(&storage)
            .insert(&Session {
                id: sid,
                project_id: pid,
                experiment_id: None,
                variant: SessionVariant::Champion,
                config_id: cfg.id,
                started_at: Utc::now(),
                ended_at: Utc::now(),
                adapter_session_ref: None,
            })
            .await
            .unwrap();
        (storage, sid, cfg.id)
    }

    #[tokio::test]
    async fn insert_then_list_for_session_roundtrips() {
        let (storage, sid, _cfg) = seeded().await;
        let repo = SignalRepo::new(&storage);
        let sig = Signal {
            id: SignalId::new(),
            session_id: sid,
            kind: SignalKind::Implicit,
            source: "tests_passed".into(),
            value: 1.0,
            recorded_at: Utc::now(),
            payload_json: Some("{\"exit_code\":0}".into()),
        };
        repo.insert(&sig).await.unwrap();
        let got = repo.list_for_session(sid).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].source, "tests_passed");
        assert_eq!(got[0].value, 1.0);
    }

    #[tokio::test]
    async fn list_for_config_joins_via_sessions() {
        let (storage, sid, cfg) = seeded().await;
        let repo = SignalRepo::new(&storage);
        for (src, val) in [("a", 1.0), ("b", 0.0)] {
            repo.insert(&Signal {
                id: SignalId::new(),
                session_id: sid,
                kind: SignalKind::Explicit,
                source: src.into(),
                value: val,
                recorded_at: Utc::now(),
                payload_json: None,
            })
            .await
            .unwrap();
        }
        let got = repo.list_for_config(cfg).await.unwrap();
        assert_eq!(got.len(), 2);
    }

    #[tokio::test]
    async fn payload_json_never_contains_code_like_content() {
        let (storage, sid, _cfg) = seeded().await;
        let repo = SignalRepo::new(&storage);
        let err = repo
            .insert(&Signal {
                id: SignalId::new(),
                session_id: sid,
                kind: SignalKind::Implicit,
                source: "suspicious".into(),
                value: 0.5,
                recorded_at: Utc::now(),
                payload_json: Some("fn main() { let x = 1; }".into()),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::PayloadRejected(_)));
    }

    #[tokio::test]
    async fn insert_rejects_value_outside_unit_interval() {
        let (storage, sid, _cfg) = seeded().await;
        let repo = SignalRepo::new(&storage);
        let err = repo
            .insert(&Signal {
                id: SignalId::new(),
                session_id: sid,
                kind: SignalKind::Implicit,
                source: "bad_value".into(),
                value: 1.5,
                recorded_at: Utc::now(),
                payload_json: None,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, StorageError::Sqlx(sqlx::Error::Database(_))));
    }
}
