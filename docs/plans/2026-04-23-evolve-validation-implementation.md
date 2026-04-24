# Evolve Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build Evolve v1.0 — a drop-in passive evolution framework for existing AI coding agents (Claude Code, Cursor, Aider). The user installs Evolve, keeps using their tool normally, and Evolve learns what configurations work best for their specific projects via champion/challenger A/B testing with Bayesian promotion. ~$0 idle cost; ~$0.01-$0.10/mo per active project for occasional challenger generation. Local-only, no telemetry.

**Architecture:** Cargo workspace with 7 small crates. `evolve-core` holds AgentConfig types, the schema DSL, mutation operators, Bayesian promotion logic, and signal aggregation. `evolve-storage` is SQLite-only persistence. `evolve-llm` is one minimal client used for occasional challenger generation. `evolve-adapters` ships Claude Code + Cursor + Aider adapters. `evolve-proxy` is the OpenAI-compat fallback for Cursor. `evolve-dashboard` is a bundled SPA. `evolve-cli` is the user-facing surface.

**Tech Stack:** Rust 2024, sqlx (SQLite), reqwest, async-openai (for Ollama only), axum, rust-embed, PyO3 + maturin, napi-rs. See `2026-04-23-evolve-validation-design.md` for full design rationale.

**Total realistic timeline:** 25-35 working days for one full-time engineer. (Smaller than the pre-pivot design because passive evolution is architecturally simpler than population-based optimization — no diversity engine, no population sizing, no batch evaluation, no benchmark runners, no provider matrix, no service mode.)

---

## Status & Pivot Note

This plan replaces the pre-pivot implementation plan (preserved in git at commit `4c1eab2`)
which targeted active-evolution-on-benchmark architecture. After Phase 0 + Task 1.1 of the
old plan, the user redirected the product to passive drop-in evolution. The work
already completed is *preserved and reused*:

| Phase / Task | Status | Notes |
|---|---|---|
| Old Phase 0 (workspace foundation) | ✅ DONE — commits `2318595`..`2fd0009` | Reused as-is |
| Old Task 1.1 (`GenomeSchema` DSL) | ✅ DONE — commit `9b2cf3c` | Reused as-is; the DSL describes the shape of `AgentConfig` |
| Everything from old Task 1.2 onward | Replaced by this plan | Old plan invalidated by pivot |

---

## Phase Index (new architecture)

| Phase | Scope | Est. Days | Status |
|---|---|---|---|
| 0 | Workspace foundation | 1-2 | ✅ DONE |
| 1 | Schema DSL + validation (reuses old Task 1.1; adds validate, AgentConfig type, basic operators) | 3-4 | 🟡 IN PROGRESS (1.1 done) |
| 2 | SQLite storage (`evolve-storage` crate, schema, migrations, repositories) | 2-3 | Pending |
| 3 | Bayesian promotion + signal aggregation in `evolve-core` | 2 | Pending |
| 4 | `evolve-llm` minimal client (Anthropic Haiku + Ollama) | 1 | Pending |
| 5 | Mutation operators (LLM-as-mutator + 4 rule-based mutators) | 2 | Pending |
| 6 | `evolve-adapters` trait + ClaudeCodeAdapter (config, hook install, transcript parser, signal extraction) | 4 | Pending |
| 7 | CursorAdapter (`.cursorrules` rewriter; falls back to proxy for live signals) | 2 | Pending |
| 8 | AiderAdapter (`aider.conf.yml` rewriter + post-commit git hook) | 2 | Pending |
| 9 | `evolve-proxy` (OpenAI-compat HTTP fallback for Cursor and friends) | 2 | Pending |
| 10 | `evolve-cli` (init, record-*, status, good/bad, roll, list, forget) | 2-3 | Pending |
| 11 | `evolve-dashboard` (bundled SPA, axum + rust-embed, champion-vs-challenger UI) | 4-5 | Pending |
| 12 | Python bindings (PyO3 + maturin) | 2 | Pending |
| 13 | TypeScript bindings (napi-rs) | 2 | Pending |
| 14 | Examples + documentation (mdBook, walkthrough per adapter) | 2-3 | Pending |
| 15 | Release pipeline (crates.io + PyPI + npm + GitHub Releases + Docker) | 1-2 | Pending |

**Bite-sized expansion policy:** Phase 1 (the only phase with partial completion) is bite-sized below. Phases 2-15 are structured task lists with file paths, key types, and test signatures — re-invoke `superpowers:writing-plans` to expand any phase to bite-sized form before execution.

---

# PHASE 1 — Schema DSL, Validation, AgentConfig

**Goal:** The `GenomeSchema` DSL types (already built), schema validation, the universal `AgentConfig` struct + per-tool extension hook, and the foundational types (`ProjectId`, `ConfigId`, `ExperimentId`, `SessionId`) used everywhere downstream.

## Task 1.1 — `GenomeSchema` DSL types

✅ **DONE** — commit `9b2cf3c`. `crates/evolve-core/src/schema.rs` defines `FieldSchema` (5 variants), `StringMutatorKind` (LlmRewrite + TemplateSlot), `GenomeSchema { fields, mutation_rates }`, and a serde-roundtrip test.

## Task 1.2 — `GenomeSchema::validate`

**Files:** Modify `crates/evolve-core/src/schema.rs`

**Step 1: Append 5 failing tests to the existing tests module:**
- `validate_rejects_inverted_float_range`
- `validate_rejects_empty_categorical`
- `validate_rejects_mutation_rate_for_unknown_field`
- `validate_rejects_mutation_rate_out_of_range`
- `validate_accepts_well_formed_schema`

**Step 2:** Run tests, confirm compile error (no `validate` method).

**Step 3:** Add `SchemaError` enum (6 variants) and `impl GenomeSchema { pub fn validate() -> Result<(), SchemaError> }` covering: float range inverted, integer range inverted, empty categorical, empty set pool, unknown mutation-rate field, mutation rate out of [0.0, 1.0].

**Step 4:** Run tests — 6 pass.

**Step 5:** `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check` — both clean.

**Step 6:** Commit `feat(schema): add GenomeSchema::validate with explicit SchemaError`.

## Task 1.3 — Foundational ID types

**Files:** Create `crates/evolve-core/src/ids.rs`; add `pub mod ids;` to `lib.rs`.

Define newtype-wrapped `Uuid` for: `ProjectId`, `ConfigId`, `ExperimentId`, `SessionId`, `SignalId`, `AdapterId` (this last is `String`-backed for stability across machines: `"claude-code"`, `"cursor"`, `"aider"`).

Add `uuid` to workspace deps (`uuid = { version = "1", features = ["v4", "serde"] }`).

Tests: each ID newtype roundtrips through serde JSON; Display implements as the inner UUID; PartialEq/Eq/Hash all derived correctly.

## Task 1.4 — `AgentConfig` universal type

**Files:** Create `crates/evolve-core/src/agent_config.rs`; add `pub mod agent_config;` to `lib.rs`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub system_prompt_prefix: String,
    pub model_pref: ModelPref,
    pub behavioral_rules: BTreeSet<String>,
    pub tool_permissions: BTreeSet<String>,
    pub response_style: ResponseStyle,
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelPref {
    ClaudeOpus, ClaudeSonnet, ClaudeHaiku,
    Gpt4o, Gpt4oMini,
    Ollama(String),  // model name
    AnyCheap,        // adapter picks
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStyle { Terse, Normal, Verbose }
```

Methods:
- `AgentConfig::default_for(adapter_id: &str) -> Self` — returns a sane starting point per adapter
- `AgentConfig::fingerprint(&self) -> u64` — stable hash for cache/dedup
- `AgentConfig::extension<T: DeserializeOwned>(&self, key: &str) -> Option<T>` — typed extension reader

Tests: roundtrip serde, fingerprint stable across clones, fingerprint differs when any field changes, default_for returns non-empty values for all 3 v1 adapter ids.

## Task 1.5 — Property tests for AgentConfig

`proptest` invariants in the same file:
- "fingerprint changes whenever ANY field changes" (proptest with arbitrary AgentConfig pairs)
- "serde roundtrip is identity for any AgentConfig"

## Task 1.6 — Phase 1 verification

`cargo test --workspace`, `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo llvm-cov --package evolve-core --summary-only` ≥80%.

**PHASE 1 COMPLETE when all gates pass.**

---

# PHASE 2 — SQLite Storage (`evolve-storage`)

**Goal:** Embedded SQLite at `~/.evolve/evolve.db` with `projects`, `agent_configs`, `experiments`, `sessions`, `signals` tables. Repository pattern over sqlx. Migrations embedded via `sqlx::migrate!` macro.

## Phase 2 Design Decisions (binding for all tasks below)

- **sqlx features:** `runtime-tokio`, `sqlite`, `macros`, `migrate`, `chrono`. *Not* the `uuid` feature — we bind UUIDs as `TEXT` (canonical hyphenated form) for human-readable `sqlite3` CLI inspection.
- **Timestamps:** `chrono::DateTime<Utc>` serialized as ISO 8601 `TEXT`. Add `chrono = { version = "0.4", features = ["serde"] }` to workspace.
- **UUIDs in DB:** stored as `TEXT`. Bind via `id.to_string()`, read via `Uuid::parse_str(...)`.
- **`u64` fingerprint in DB:** stored as `INTEGER` via `fingerprint as i64` bit-cast (two-way unambiguous). Document at the call sites.
- **Connection mode:** `sqlx::SqlitePool` with `create_if_missing(true)` + `foreign_keys = ON` pragma enforced at connect time.
- **In-memory test handle:** `Storage::in_memory_for_tests()` → `SqlitePool` against `sqlite::memory:` with migrations pre-applied. Every repo test uses this.
- **Error type:** crate-local `StorageError` via `thiserror` wrapping `sqlx::Error`, `serde_json::Error`, and `uuid::Error`.
- **Privacy invariant (enforced by Task 2.9 tests):** the `signals.payload_json` column MUST NOT contain code-like content. Tests validate by regex against inserted rows.

## Task 2.1 — Create `evolve-storage` crate skeleton

**Files:**
- Create: `crates/evolve-storage/Cargo.toml`
- Create: `crates/evolve-storage/src/lib.rs`
- Create: `crates/evolve-storage/migrations/` (empty dir with `.gitkeep`)
- Modify: `Cargo.toml` (workspace root — add member + chrono + sqlx to workspace deps)

**Step 1: Add `chrono` and `sqlx` to workspace.dependencies**

Edit the root `Cargo.toml`, inside `[workspace.dependencies]`, append:

```toml
chrono = { version = "0.4", default-features = false, features = ["std", "clock", "serde"] }
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "sqlite", "macros", "migrate", "chrono"] }
tempfile = "3"
```

Also add `"crates/evolve-storage"` to `[workspace] members`.

**Step 2: Create `crates/evolve-storage/Cargo.toml`**

```toml
[package]
name = "evolve-storage"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "SQLite persistence for Evolve (projects, configs, experiments, sessions, signals)"

[dependencies]
evolve-core = { path = "../evolve-core" }
sqlx.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
uuid.workspace = true
thiserror.workspace = true
tracing.workspace = true
async-trait.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
tempfile.workspace = true
```

**Step 3: Create `crates/evolve-storage/src/lib.rs`**

```rust
//! evolve-storage: SQLite persistence for Evolve.
//!
//! Opens a single database at the configured path (default `~/.evolve/evolve.db`),
//! applies embedded migrations, and exposes repository structs per table.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod pool;

pub use error::StorageError;
pub use pool::Storage;
```

**Step 4: Create `crates/evolve-storage/migrations/.gitkeep`** (empty file so the directory is tracked).

**Step 5: Stub `src/error.rs` and `src/pool.rs` so the crate compiles**

`src/error.rs`:
```rust
//! Unified error type for the storage crate.

use thiserror::Error;

/// Errors produced by [`Storage`](crate::Storage) and its repositories.
#[derive(Debug, Error)]
pub enum StorageError {
    /// An underlying sqlx error (connection, query, migration).
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// A JSON serialization error on a payload column.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// A migration error from `sqlx::migrate!`.
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// Failed to parse a UUID from a TEXT column.
    #[error("uuid: {0}")]
    Uuid(#[from] uuid::Error),
}
```

`src/pool.rs`:
```rust
//! Connection pool + migration runner.

use crate::error::StorageError;

/// Handle to the SQLite database. Cheap to clone (wraps a `SqlitePool`).
#[derive(Debug, Clone)]
pub struct Storage {
    pool: sqlx::SqlitePool,
}

impl Storage {
    /// Borrow the underlying pool. Repositories take this by reference.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// Placeholder so the crate compiles before Task 2.3 wires this up.
    #[allow(dead_code)]
    pub(crate) fn from_pool(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }

    /// Silence unused-error warnings until Task 2.3 fills this in.
    #[allow(dead_code)]
    fn _unused_error_shape(_e: StorageError) {}
}
```

**Step 6: Verify**

Run: `cargo check -p evolve-storage`
Expected: compiles clean, no warnings.

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

**Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/evolve-storage
git commit -m "feat(storage): scaffold evolve-storage crate with error type"
```

## Task 2.2 — Initial migration `0001_init.sql`

**Files:**
- Create: `crates/evolve-storage/migrations/0001_init.sql`

**Step 1: Write the migration**

```sql
-- 0001_init.sql: initial schema for evolve-storage
-- All ids stored as TEXT (UUID hyphenated) except adapter_id which is free-form TEXT.
-- All timestamps stored as TEXT (ISO 8601 UTC).

PRAGMA foreign_keys = ON;

CREATE TABLE projects (
    id                   TEXT PRIMARY KEY NOT NULL,
    adapter_id           TEXT NOT NULL,
    root_path            TEXT NOT NULL UNIQUE,
    name                 TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    champion_config_id   TEXT,
    FOREIGN KEY (champion_config_id) REFERENCES agent_configs(id) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE agent_configs (
    id             TEXT PRIMARY KEY NOT NULL,
    project_id     TEXT NOT NULL,
    adapter_id     TEXT NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('champion','challenger','historical')),
    fingerprint    INTEGER NOT NULL,
    payload_json   TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
CREATE INDEX idx_agent_configs_project_role_created
    ON agent_configs(project_id, role, created_at DESC);

CREATE TABLE experiments (
    id                      TEXT PRIMARY KEY NOT NULL,
    project_id              TEXT NOT NULL,
    champion_config_id      TEXT NOT NULL,
    challenger_config_id    TEXT NOT NULL,
    status                  TEXT NOT NULL CHECK (status IN ('running','promoted','aborted','held')),
    traffic_share           REAL NOT NULL CHECK (traffic_share >= 0.0 AND traffic_share <= 1.0),
    started_at              TEXT NOT NULL,
    decided_at              TEXT,
    decision_posterior      REAL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (champion_config_id) REFERENCES agent_configs(id),
    FOREIGN KEY (challenger_config_id) REFERENCES agent_configs(id)
);
-- At most one running experiment per project (partial unique index).
CREATE UNIQUE INDEX uniq_running_experiment_per_project
    ON experiments(project_id)
    WHERE status = 'running';
CREATE INDEX idx_experiments_project_status
    ON experiments(project_id, status);

CREATE TABLE sessions (
    id                      TEXT PRIMARY KEY NOT NULL,
    project_id              TEXT NOT NULL,
    experiment_id           TEXT,
    variant                 TEXT NOT NULL CHECK (variant IN ('champion','challenger')),
    config_id               TEXT NOT NULL,
    started_at              TEXT NOT NULL,
    ended_at                TEXT NOT NULL,
    adapter_session_ref     TEXT,
    FOREIGN KEY (project_id)   REFERENCES projects(id)      ON DELETE CASCADE,
    FOREIGN KEY (experiment_id) REFERENCES experiments(id)  ON DELETE SET NULL,
    FOREIGN KEY (config_id)    REFERENCES agent_configs(id)
);
CREATE INDEX idx_sessions_project_started
    ON sessions(project_id, started_at DESC);
CREATE INDEX idx_sessions_experiment
    ON sessions(experiment_id);

CREATE TABLE signals (
    id              TEXT PRIMARY KEY NOT NULL,
    session_id      TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('explicit','implicit')),
    source          TEXT NOT NULL,
    value           REAL NOT NULL CHECK (value >= 0.0 AND value <= 1.0),
    recorded_at     TEXT NOT NULL,
    payload_json    TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);
CREATE INDEX idx_signals_session ON signals(session_id);
```

**Step 2: Verify the file parses as valid SQLite (sanity check)**

Run: `sqlite3 :memory: < crates/evolve-storage/migrations/0001_init.sql`
Expected: no output, exit code 0.
(If `sqlite3` CLI is unavailable, skip — Task 2.3 will exercise this via sqlx.)

**Step 3: Commit**

```bash
git add crates/evolve-storage/migrations/0001_init.sql
git commit -m "feat(storage): initial migration with projects/configs/experiments/sessions/signals"
```

## Task 2.3 — `Storage::open`, `migrate`, `in_memory_for_tests`

**Files:**
- Modify: `crates/evolve-storage/src/pool.rs`

**Step 1: Write the failing test (append to `pool.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_storage_applies_migrations() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        // After migrations, `projects` table must exist.
        let exists: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='projects'",
        )
        .fetch_one(storage.pool())
        .await
        .unwrap();
        assert_eq!(exists.0, 1);
    }

    #[tokio::test]
    async fn foreign_keys_pragma_is_enabled() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let fk: (i64,) = sqlx::query_as("PRAGMA foreign_keys")
            .fetch_one(storage.pool())
            .await
            .unwrap();
        assert_eq!(fk.0, 1, "foreign_keys pragma must be ON");
    }

    #[tokio::test]
    async fn all_five_tables_exist_after_migration() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let names: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
        )
        .fetch_all(storage.pool())
        .await
        .unwrap();
        let got: Vec<&str> = names.iter().map(|(n,)| n.as_str()).collect();
        for expected in ["agent_configs", "experiments", "projects", "sessions", "signals"] {
            assert!(
                got.contains(&expected),
                "missing table {expected}; got {got:?}",
            );
        }
    }
}
```

**Step 2: Run the tests — expected fail**

Run: `cargo test -p evolve-storage`
Expected: compile error (`Storage::in_memory_for_tests` does not exist).

**Step 3: Implement `Storage::open`, `migrate`, `in_memory_for_tests`**

Replace the body of `pool.rs` above the tests module with:

```rust
//! Connection pool + migration runner.

use crate::error::StorageError;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Handle to the SQLite database. Cheap to clone (wraps a `SqlitePool`).
#[derive(Debug, Clone)]
pub struct Storage {
    pool: sqlx::SqlitePool,
}

impl Storage {
    /// Open the database at `path`, creating it if missing. Applies all
    /// embedded migrations before returning.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let url = format!("sqlite://{}", path.as_ref().display());
        let options = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// Open a fresh in-memory database with all migrations applied.
    /// Useful only in tests.
    pub async fn in_memory_for_tests() -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // single connection for in-memory (per-conn DBs)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// Borrow the underlying pool. Repositories take this by reference.
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }
}
```

**Step 4: Run the tests — expected pass**

Run: `cargo test -p evolve-storage`
Expected: 3 tests pass.

**Step 5: Lint gates**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

**Step 6: Commit**

```bash
git add crates/evolve-storage/src/pool.rs
git commit -m "feat(storage): Storage::open and in-memory test helper with migration runner"
```

## Task 2.4 — `ProjectRepo`

**Files:**
- Create: `crates/evolve-storage/src/projects.rs`
- Modify: `crates/evolve-storage/src/lib.rs` (add `pub mod projects;`)

**Step 1: Write the failing tests in `projects.rs`**

```rust
//! Repository for the `projects` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{AdapterId, ConfigId, ProjectId};
use uuid::Uuid;

/// Row in the `projects` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// Project identity.
    pub id: ProjectId,
    /// Adapter that manages this project ("claude-code", "cursor", "aider").
    pub adapter_id: AdapterId,
    /// Canonical absolute path to the project root.
    pub root_path: String,
    /// Human-readable name (usually the basename of `root_path`).
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Current champion config id; `None` only during `init` before the first
    /// AgentConfig row has been written.
    pub champion_config_id: Option<ConfigId>,
}

/// Repository for `projects`.
#[derive(Debug, Clone)]
pub struct ProjectRepo<'a> {
    storage: &'a Storage,
}

impl<'a> ProjectRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new project row. Caller supplies the id.
    pub async fn insert(&self, project: &Project) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO projects
                (id, adapter_id, root_path, name, created_at, champion_config_id)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(project.id.to_string())
        .bind(project.adapter_id.as_str())
        .bind(&project.root_path)
        .bind(&project.name)
        .bind(project.created_at.to_rfc3339())
        .bind(project.champion_config_id.map(|c| c.to_string()))
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// Fetch by id; returns `Ok(None)` if no row matches.
    pub async fn get_by_id(&self, id: ProjectId) -> Result<Option<Project>, StorageError> {
        let row: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, adapter_id, root_path, name, created_at, champion_config_id
             FROM projects WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_project).transpose()
    }

    /// Fetch by root path.
    pub async fn get_by_root_path(&self, root: &str) -> Result<Option<Project>, StorageError> {
        let row: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, adapter_id, root_path, name, created_at, champion_config_id
             FROM projects WHERE root_path = ?",
        )
        .bind(root)
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_project).transpose()
    }

    /// List all projects, most recently created first.
    pub async fn list(&self) -> Result<Vec<Project>, StorageError> {
        let rows: Vec<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, adapter_id, root_path, name, created_at, champion_config_id
             FROM projects ORDER BY created_at DESC",
        )
        .fetch_all(self.storage.pool())
        .await?;
        rows.into_iter().map(row_to_project).collect()
    }

    /// Delete a project and cascade (configs, experiments, sessions, signals go too).
    pub async fn delete(&self, id: ProjectId) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await?;
        Ok(())
    }

    /// Update the champion config pointer (used when a challenger is promoted).
    pub async fn set_champion(
        &self,
        id: ProjectId,
        config_id: ConfigId,
    ) -> Result<(), StorageError> {
        sqlx::query("UPDATE projects SET champion_config_id = ? WHERE id = ?")
            .bind(config_id.to_string())
            .bind(id.to_string())
            .execute(self.storage.pool())
            .await?;
        Ok(())
    }
}

fn row_to_project(
    (id, adapter_id, root_path, name, created_at, champion): (
        String,
        String,
        String,
        String,
        String,
        Option<String>,
    ),
) -> Result<Project, StorageError> {
    Ok(Project {
        id: ProjectId::from_uuid(Uuid::parse_str(&id)?),
        adapter_id: AdapterId::new(adapter_id),
        root_path,
        name,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
        champion_config_id: champion
            .map(|s| Uuid::parse_str(&s).map(ConfigId::from_uuid))
            .transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(adapter: &str, root: &str) -> Project {
        Project {
            id: ProjectId::new(),
            adapter_id: AdapterId::new(adapter),
            root_path: root.to_string(),
            name: root.rsplit('/').next().unwrap_or(root).to_string(),
            created_at: Utc::now(),
            champion_config_id: None,
        }
    }

    #[tokio::test]
    async fn insert_then_get_by_id_roundtrips() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let p = sample("claude-code", "/tmp/proj-a");
        repo.insert(&p).await.unwrap();
        let back = repo.get_by_id(p.id).await.unwrap().unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(back.adapter_id.as_str(), "claude-code");
        assert_eq!(back.root_path, "/tmp/proj-a");
    }

    #[tokio::test]
    async fn get_by_id_returns_none_when_absent() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        assert!(repo.get_by_id(ProjectId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_by_root_path_finds_inserted_project() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let p = sample("cursor", "/tmp/proj-b");
        repo.insert(&p).await.unwrap();
        let back = repo
            .get_by_root_path("/tmp/proj-b")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(back.id, p.id);
    }

    #[tokio::test]
    async fn list_orders_by_created_at_desc() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let older = Project {
            created_at: Utc::now() - chrono::Duration::hours(1),
            ..sample("aider", "/tmp/older")
        };
        let newer = sample("aider", "/tmp/newer");
        repo.insert(&older).await.unwrap();
        repo.insert(&newer).await.unwrap();
        let rows = repo.list().await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, newer.id);
        assert_eq!(rows[1].id, older.id);
    }

    #[tokio::test]
    async fn root_path_uniqueness_is_enforced() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let a = sample("claude-code", "/tmp/dup");
        let b = sample("cursor", "/tmp/dup");
        repo.insert(&a).await.unwrap();
        let err = repo.insert(&b).await.unwrap_err();
        assert!(
            matches!(err, StorageError::Sqlx(sqlx::Error::Database(_))),
            "expected UNIQUE violation; got {err:?}",
        );
    }

    #[tokio::test]
    async fn delete_removes_the_row() {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let repo = ProjectRepo::new(&storage);
        let p = sample("claude-code", "/tmp/del");
        repo.insert(&p).await.unwrap();
        repo.delete(p.id).await.unwrap();
        assert!(repo.get_by_id(p.id).await.unwrap().is_none());
    }
}
```

**Step 2: Register the module**

Edit `src/lib.rs`, add `pub mod projects;` under the existing `pub mod pool;` line.

**Step 3: Run tests — expected pass**

Run: `cargo test -p evolve-storage`
Expected: all `projects::tests` tests pass plus the 3 from Task 2.3.

**Step 4: Lint gates**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

**Step 5: Commit**

```bash
git add crates/evolve-storage/src/lib.rs crates/evolve-storage/src/projects.rs
git commit -m "feat(storage): ProjectRepo with insert/get/list/delete/set_champion"
```

## Task 2.5 — `AgentConfigRepo`

**Files:**
- Create: `crates/evolve-storage/src/agent_configs.rs`
- Modify: `crates/evolve-storage/src/lib.rs` (add `pub mod agent_configs;`)

**Step 1: Define the row + the three enum discriminants**

```rust
//! Repository for the `agent_configs` table.

use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId};
use uuid::Uuid;

/// The role an AgentConfig plays for its project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRole {
    /// Currently-deployed default.
    Champion,
    /// Variant being A/B-tested against the champion.
    Challenger,
    /// Retired — kept for the promotion log.
    Historical,
}

impl ConfigRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Champion => "champion",
            Self::Challenger => "challenger",
            Self::Historical => "historical",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "champion" => Self::Champion,
            "challenger" => Self::Challenger,
            "historical" => Self::Historical,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown config role {other:?}").into(),
                )))
            }
        })
    }
}

/// A stored AgentConfig row.
#[derive(Debug, Clone)]
pub struct AgentConfigRow {
    /// Row id.
    pub id: ConfigId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Adapter this config targets.
    pub adapter_id: AdapterId,
    /// Role at time of insertion (can be promoted/retired later).
    pub role: ConfigRole,
    /// Stable hash of the payload (from [`AgentConfig::fingerprint`]).
    pub fingerprint: u64,
    /// The config itself.
    pub payload: AgentConfig,
    /// When this row was inserted.
    pub created_at: DateTime<Utc>,
}

/// Repository for `agent_configs`.
#[derive(Debug, Clone)]
pub struct AgentConfigRepo<'a> {
    storage: &'a Storage,
}
```

**Step 2: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Project, ProjectRepo};

    async fn seeded_storage() -> (Storage, ProjectId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let project = Project {
            id: ProjectId::new(),
            adapter_id: AdapterId::new("claude-code"),
            root_path: "/tmp/agent-config-repo-test".into(),
            name: "test".into(),
            created_at: Utc::now(),
            champion_config_id: None,
        };
        ProjectRepo::new(&storage).insert(&project).await.unwrap();
        let pid = project.id;
        (storage, pid)
    }

    fn sample_row(project_id: ProjectId, role: ConfigRole) -> AgentConfigRow {
        let payload = AgentConfig::default_for("claude-code");
        AgentConfigRow {
            id: ConfigId::new(),
            project_id,
            adapter_id: AdapterId::new("claude-code"),
            role,
            fingerprint: payload.fingerprint(),
            payload,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn insert_and_get_by_id_roundtrips_full_payload() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);
        let row = sample_row(pid, ConfigRole::Champion);
        repo.insert(&row).await.unwrap();
        let back = repo.get_by_id(row.id).await.unwrap().unwrap();
        assert_eq!(back.id, row.id);
        assert_eq!(back.role, ConfigRole::Champion);
        assert_eq!(back.fingerprint, row.fingerprint);
        assert_eq!(back.payload, row.payload);
    }

    #[tokio::test]
    async fn latest_for_project_role_returns_most_recent() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);

        let older = AgentConfigRow {
            created_at: Utc::now() - chrono::Duration::hours(2),
            ..sample_row(pid, ConfigRole::Champion)
        };
        let newer = sample_row(pid, ConfigRole::Champion);
        repo.insert(&older).await.unwrap();
        repo.insert(&newer).await.unwrap();

        let latest = repo
            .latest_for_project_role(pid, ConfigRole::Champion)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, newer.id);
    }

    #[tokio::test]
    async fn latest_for_project_role_returns_none_when_no_rows() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);
        assert!(repo
            .latest_for_project_role(pid, ConfigRole::Challenger)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn cascade_delete_removes_configs() {
        let (storage, pid) = seeded_storage().await;
        let repo = AgentConfigRepo::new(&storage);
        let row = sample_row(pid, ConfigRole::Champion);
        repo.insert(&row).await.unwrap();

        ProjectRepo::new(&storage).delete(pid).await.unwrap();
        assert!(repo.get_by_id(row.id).await.unwrap().is_none());
    }
}
```

**Step 3: Run tests — expected fail**

Run: `cargo test -p evolve-storage agent_configs`
Expected: compile errors (methods missing).

**Step 4: Implement the repo**

```rust
impl<'a> AgentConfigRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new config row. Caller owns the id.
    pub async fn insert(&self, row: &AgentConfigRow) -> Result<(), StorageError> {
        let payload_json = serde_json::to_string(&row.payload)?;
        sqlx::query(
            "INSERT INTO agent_configs
                (id, project_id, adapter_id, role, fingerprint, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.to_string())
        .bind(row.project_id.to_string())
        .bind(row.adapter_id.as_str())
        .bind(row.role.as_str())
        .bind(row.fingerprint as i64) // bit-cast u64 -> i64; see Phase 2 design decisions
        .bind(payload_json)
        .bind(row.created_at.to_rfc3339())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// Fetch by id.
    pub async fn get_by_id(&self, id: ConfigId) -> Result<Option<AgentConfigRow>, StorageError> {
        let row: Option<(String, String, String, String, i64, String, String)> = sqlx::query_as(
            "SELECT id, project_id, adapter_id, role, fingerprint, payload_json, created_at
             FROM agent_configs WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_agent_config).transpose()
    }

    /// Return the most recently created row for `(project, role)`.
    pub async fn latest_for_project_role(
        &self,
        project_id: ProjectId,
        role: ConfigRole,
    ) -> Result<Option<AgentConfigRow>, StorageError> {
        let row: Option<(String, String, String, String, i64, String, String)> = sqlx::query_as(
            "SELECT id, project_id, adapter_id, role, fingerprint, payload_json, created_at
             FROM agent_configs
             WHERE project_id = ? AND role = ?
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(project_id.to_string())
        .bind(role.as_str())
        .fetch_optional(self.storage.pool())
        .await?;
        row.map(row_to_agent_config).transpose()
    }
}

fn row_to_agent_config(
    (id, project_id, adapter_id, role, fingerprint, payload_json, created_at): (
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
    ),
) -> Result<AgentConfigRow, StorageError> {
    Ok(AgentConfigRow {
        id: ConfigId::from_uuid(Uuid::parse_str(&id)?),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&project_id)?),
        adapter_id: AdapterId::new(adapter_id),
        role: ConfigRole::from_str(&role)?,
        fingerprint: fingerprint as u64,
        payload: serde_json::from_str(&payload_json)?,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
    })
}
```

**Step 5: Register module, run tests, lint, commit**

Add `pub mod agent_configs;` to `lib.rs`. Run `cargo test -p evolve-storage`. Expect all passing.
Run fmt/clippy. Commit:

```bash
git add crates/evolve-storage/src/lib.rs crates/evolve-storage/src/agent_configs.rs
git commit -m "feat(storage): AgentConfigRepo with role enum and latest-per-role lookup"
```

## Task 2.6 — `ExperimentRepo`

**Files:**
- Create: `crates/evolve-storage/src/experiments.rs`
- Modify: `crates/evolve-storage/src/lib.rs`

**Step 1: Write failing tests including the partial-unique-index invariant**

```rust
//! Repository for the `experiments` table.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
    use crate::projects::{Project, ProjectRepo};
    use chrono::Utc;
    use evolve_core::agent_config::AgentConfig;
    use evolve_core::ids::{AdapterId, ConfigId, ProjectId};

    async fn seeded() -> (Storage, ProjectId, ConfigId, ConfigId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let pid = ProjectId::new();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: pid,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/experiments-test".into(),
                name: "x".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        let champion = AgentConfigRow {
            id: ConfigId::new(),
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: 1,
            payload: AgentConfig::default_for("claude-code"),
            created_at: Utc::now(),
        };
        let challenger = AgentConfigRow {
            id: ConfigId::new(),
            role: ConfigRole::Challenger,
            fingerprint: 2,
            ..champion.clone()
        };
        let cfg = AgentConfigRepo::new(&storage);
        cfg.insert(&champion).await.unwrap();
        cfg.insert(&challenger).await.unwrap();

        (storage, pid, champion.id, challenger.id)
    }

    #[tokio::test]
    async fn insert_and_get_running_returns_the_row() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);
        let exp = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        repo.insert(&exp).await.unwrap();
        let back = repo.get_running_for_project(pid).await.unwrap().unwrap();
        assert_eq!(back.id, exp.id);
    }

    #[tokio::test]
    async fn only_one_running_experiment_per_project_is_allowed() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);

        let first = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        let second = Experiment { id: ExperimentId::new(), ..first.clone() };
        repo.insert(&first).await.unwrap();
        let err = repo.insert(&second).await.unwrap_err();
        assert!(matches!(err, StorageError::Sqlx(sqlx::Error::Database(_))));
    }

    #[tokio::test]
    async fn update_status_to_promoted_sets_decided_at_and_posterior() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);
        let exp = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        repo.insert(&exp).await.unwrap();
        let decided = Utc::now();
        repo.update_status(exp.id, ExperimentStatus::Promoted, Some(decided), Some(0.97))
            .await
            .unwrap();

        let completed = repo.list_completed(pid).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].status, ExperimentStatus::Promoted);
        assert_eq!(completed[0].decision_posterior, Some(0.97));
    }

    #[tokio::test]
    async fn list_completed_excludes_running() {
        let (storage, pid, champ, chall) = seeded().await;
        let repo = ExperimentRepo::new(&storage);
        let exp = Experiment {
            id: ExperimentId::new(),
            project_id: pid,
            champion_config_id: champ,
            challenger_config_id: chall,
            status: ExperimentStatus::Running,
            traffic_share: 0.05,
            started_at: Utc::now(),
            decided_at: None,
            decision_posterior: None,
        };
        repo.insert(&exp).await.unwrap();
        assert!(repo.list_completed(pid).await.unwrap().is_empty());
    }
}
```

**Step 2: Implement `Experiment`, `ExperimentStatus`, `ExperimentRepo`**

```rust
use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{ConfigId, ExperimentId, ProjectId};
use uuid::Uuid;

/// Experiment lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentStatus {
    /// Actively collecting signals.
    Running,
    /// Challenger won; promoted to champion.
    Promoted,
    /// Manually cancelled or superseded.
    Aborted,
    /// Decision reached but champion kept.
    Held,
}

impl ExperimentStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Promoted => "promoted",
            Self::Aborted => "aborted",
            Self::Held => "held",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "running" => Self::Running,
            "promoted" => Self::Promoted,
            "aborted" => Self::Aborted,
            "held" => Self::Held,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown experiment status {other:?}").into(),
                )))
            }
        })
    }
}

/// One champion-vs-challenger experiment row.
#[derive(Debug, Clone)]
pub struct Experiment {
    /// Experiment identity.
    pub id: ExperimentId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Champion config under test.
    pub champion_config_id: ConfigId,
    /// Challenger config under test.
    pub challenger_config_id: ConfigId,
    /// Lifecycle state.
    pub status: ExperimentStatus,
    /// Share of sessions routed to challenger (0..=1).
    pub traffic_share: f64,
    /// When the experiment started.
    pub started_at: DateTime<Utc>,
    /// When the decision was reached (only set for non-Running statuses).
    pub decided_at: Option<DateTime<Utc>>,
    /// P(challenger > champion) at decision time.
    pub decision_posterior: Option<f64>,
}

/// Repository for `experiments`.
#[derive(Debug, Clone)]
pub struct ExperimentRepo<'a> {
    storage: &'a Storage,
}

impl<'a> ExperimentRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new experiment.
    pub async fn insert(&self, exp: &Experiment) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO experiments
                (id, project_id, champion_config_id, challenger_config_id,
                 status, traffic_share, started_at, decided_at, decision_posterior)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(exp.id.to_string())
        .bind(exp.project_id.to_string())
        .bind(exp.champion_config_id.to_string())
        .bind(exp.challenger_config_id.to_string())
        .bind(exp.status.as_str())
        .bind(exp.traffic_share)
        .bind(exp.started_at.to_rfc3339())
        .bind(exp.decided_at.map(|d| d.to_rfc3339()))
        .bind(exp.decision_posterior)
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// Return the single running experiment for a project, if any.
    pub async fn get_running_for_project(
        &self,
        project_id: ProjectId,
    ) -> Result<Option<Experiment>, StorageError> {
        let row: Option<(String, String, String, String, String, f64, String, Option<String>, Option<f64>)> =
            sqlx::query_as(
                "SELECT id, project_id, champion_config_id, challenger_config_id,
                        status, traffic_share, started_at, decided_at, decision_posterior
                 FROM experiments
                 WHERE project_id = ? AND status = 'running'
                 LIMIT 1",
            )
            .bind(project_id.to_string())
            .fetch_optional(self.storage.pool())
            .await?;
        row.map(row_to_experiment).transpose()
    }

    /// List all non-Running experiments for a project, most recent first.
    pub async fn list_completed(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<Experiment>, StorageError> {
        let rows: Vec<(String, String, String, String, String, f64, String, Option<String>, Option<f64>)> =
            sqlx::query_as(
                "SELECT id, project_id, champion_config_id, challenger_config_id,
                        status, traffic_share, started_at, decided_at, decision_posterior
                 FROM experiments
                 WHERE project_id = ? AND status != 'running'
                 ORDER BY decided_at DESC NULLS LAST, started_at DESC",
            )
            .bind(project_id.to_string())
            .fetch_all(self.storage.pool())
            .await?;
        rows.into_iter().map(row_to_experiment).collect()
    }

    /// Update the lifecycle state (and decision timestamp + posterior on terminal transitions).
    pub async fn update_status(
        &self,
        id: ExperimentId,
        status: ExperimentStatus,
        decided_at: Option<DateTime<Utc>>,
        decision_posterior: Option<f64>,
    ) -> Result<(), StorageError> {
        sqlx::query(
            "UPDATE experiments
             SET status = ?, decided_at = ?, decision_posterior = ?
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(decided_at.map(|d| d.to_rfc3339()))
        .bind(decision_posterior)
        .bind(id.to_string())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }
}

#[allow(clippy::type_complexity)]
fn row_to_experiment(
    (id, project_id, champion, challenger, status, traffic_share, started_at, decided_at, posterior): (
        String,
        String,
        String,
        String,
        String,
        f64,
        String,
        Option<String>,
        Option<f64>,
    ),
) -> Result<Experiment, StorageError> {
    Ok(Experiment {
        id: ExperimentId::from_uuid(Uuid::parse_str(&id)?),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&project_id)?),
        champion_config_id: ConfigId::from_uuid(Uuid::parse_str(&champion)?),
        challenger_config_id: ConfigId::from_uuid(Uuid::parse_str(&challenger)?),
        status: ExperimentStatus::from_str(&status)?,
        traffic_share,
        started_at: DateTime::parse_from_rfc3339(&started_at)
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))?
            .with_timezone(&Utc),
        decided_at: decided_at
            .map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))
            })
            .transpose()?,
        decision_posterior: posterior,
    })
}
```

**Note on `NULLS LAST`:** SQLite sorts `NULL` before non-null by default under `DESC`, which is the behavior we want here (rows without `decided_at` yet rank last under DESC). If your SQLite version rejects `NULLS LAST`, drop the clause — default behavior is acceptable.

**Step 3: Register, run tests, lint, commit**

```bash
git add crates/evolve-storage/src/lib.rs crates/evolve-storage/src/experiments.rs
git commit -m "feat(storage): ExperimentRepo with partial-unique 'one running per project' enforcement"
```

## Task 2.7 — `SessionRepo`

**Files:**
- Create: `crates/evolve-storage/src/sessions.rs`
- Modify: `crates/evolve-storage/src/lib.rs`

**Step 1: Write failing tests covering ordering + experiment filtering**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
    use crate::experiments::{Experiment, ExperimentRepo, ExperimentStatus};
    use crate::projects::{Project, ProjectRepo};
    use chrono::Utc;
    use evolve_core::agent_config::AgentConfig;
    use evolve_core::ids::{AdapterId, ConfigId, ExperimentId, ProjectId};

    async fn seeded() -> (Storage, ProjectId, ConfigId, ConfigId, ExperimentId) {
        let storage = Storage::in_memory_for_tests().await.unwrap();
        let pid = ProjectId::new();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: pid,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/sessions-test".into(),
                name: "s".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        let champ = AgentConfigRow {
            id: ConfigId::new(),
            project_id: pid,
            adapter_id: AdapterId::new("claude-code"),
            role: ConfigRole::Champion,
            fingerprint: 1,
            payload: AgentConfig::default_for("claude-code"),
            created_at: Utc::now(),
        };
        let chall = AgentConfigRow {
            id: ConfigId::new(),
            role: ConfigRole::Challenger,
            ..champ.clone()
        };
        let cfg = AgentConfigRepo::new(&storage);
        cfg.insert(&champ).await.unwrap();
        cfg.insert(&chall).await.unwrap();

        let eid = ExperimentId::new();
        ExperimentRepo::new(&storage)
            .insert(&Experiment {
                id: eid,
                project_id: pid,
                champion_config_id: champ.id,
                challenger_config_id: chall.id,
                status: ExperimentStatus::Running,
                traffic_share: 0.1,
                started_at: Utc::now(),
                decided_at: None,
                decision_posterior: None,
            })
            .await
            .unwrap();

        (storage, pid, champ.id, chall.id, eid)
    }

    #[tokio::test]
    async fn list_recent_orders_newest_first_and_respects_limit() {
        let (storage, pid, champ, _chall, _eid) = seeded().await;
        let repo = SessionRepo::new(&storage);

        for i in 0..5 {
            let s = Session {
                id: SessionId::new(),
                project_id: pid,
                experiment_id: None,
                variant: SessionVariant::Champion,
                config_id: champ,
                started_at: Utc::now() - chrono::Duration::minutes(i * 10),
                ended_at: Utc::now() - chrono::Duration::minutes(i * 10) + chrono::Duration::minutes(5),
                adapter_session_ref: Some(format!("transcript-{i}.jsonl")),
            };
            repo.insert(&s).await.unwrap();
        }
        let rows = repo.list_recent(pid, 3).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows[0].started_at >= rows[1].started_at);
        assert!(rows[1].started_at >= rows[2].started_at);
    }

    #[tokio::test]
    async fn list_for_experiment_returns_only_tagged_sessions() {
        let (storage, pid, champ, chall, eid) = seeded().await;
        let repo = SessionRepo::new(&storage);

        let tagged = Session {
            id: SessionId::new(),
            project_id: pid,
            experiment_id: Some(eid),
            variant: SessionVariant::Challenger,
            config_id: chall,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            adapter_session_ref: None,
        };
        let untagged = Session {
            id: SessionId::new(),
            project_id: pid,
            experiment_id: None,
            variant: SessionVariant::Champion,
            config_id: champ,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            adapter_session_ref: None,
        };
        repo.insert(&tagged).await.unwrap();
        repo.insert(&untagged).await.unwrap();

        let got = repo.list_for_experiment(eid).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, tagged.id);
    }
}
```

**Step 2: Implement `Session`, `SessionVariant`, `SessionRepo`**

```rust
use crate::error::StorageError;
use crate::pool::Storage;
use chrono::{DateTime, Utc};
use evolve_core::ids::{ConfigId, ExperimentId, ProjectId, SessionId};
use uuid::Uuid;

/// Which variant was active when this session ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionVariant {
    /// The project's champion config was applied.
    Champion,
    /// The active experiment's challenger config was applied.
    Challenger,
}

impl SessionVariant {
    fn as_str(self) -> &'static str {
        match self {
            Self::Champion => "champion",
            Self::Challenger => "challenger",
        }
    }

    fn from_str(s: &str) -> Result<Self, StorageError> {
        Ok(match s {
            "champion" => Self::Champion,
            "challenger" => Self::Challenger,
            other => {
                return Err(StorageError::Sqlx(sqlx::Error::Decode(
                    format!("unknown session variant {other:?}").into(),
                )))
            }
        })
    }
}

/// One recorded user session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session identity.
    pub id: SessionId,
    /// Owning project.
    pub project_id: ProjectId,
    /// Experiment active when the session started, if any.
    pub experiment_id: Option<ExperimentId>,
    /// Which variant was deployed for this session.
    pub variant: SessionVariant,
    /// The exact config row that was active.
    pub config_id: ConfigId,
    /// Start time.
    pub started_at: DateTime<Utc>,
    /// End time.
    pub ended_at: DateTime<Utc>,
    /// Opaque adapter-specific reference (e.g., transcript filename).
    pub adapter_session_ref: Option<String>,
}

/// Repository for `sessions`.
#[derive(Debug, Clone)]
pub struct SessionRepo<'a> {
    storage: &'a Storage,
}

impl<'a> SessionRepo<'a> {
    /// Construct a new repo borrowing the storage handle.
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Insert a new session row.
    pub async fn insert(&self, s: &Session) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO sessions
                (id, project_id, experiment_id, variant, config_id,
                 started_at, ended_at, adapter_session_ref)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(s.id.to_string())
        .bind(s.project_id.to_string())
        .bind(s.experiment_id.map(|e| e.to_string()))
        .bind(s.variant.as_str())
        .bind(s.config_id.to_string())
        .bind(s.started_at.to_rfc3339())
        .bind(s.ended_at.to_rfc3339())
        .bind(s.adapter_session_ref.as_deref())
        .execute(self.storage.pool())
        .await?;
        Ok(())
    }

    /// List most recent sessions for a project (descending by `started_at`).
    pub async fn list_recent(
        &self,
        project_id: ProjectId,
        limit: u32,
    ) -> Result<Vec<Session>, StorageError> {
        let rows: Vec<(String, String, Option<String>, String, String, String, String, Option<String>)> =
            sqlx::query_as(
                "SELECT id, project_id, experiment_id, variant, config_id,
                        started_at, ended_at, adapter_session_ref
                 FROM sessions
                 WHERE project_id = ?
                 ORDER BY started_at DESC
                 LIMIT ?",
            )
            .bind(project_id.to_string())
            .bind(limit as i64)
            .fetch_all(self.storage.pool())
            .await?;
        rows.into_iter().map(row_to_session).collect()
    }

    /// All sessions belonging to a specific experiment.
    pub async fn list_for_experiment(
        &self,
        experiment_id: ExperimentId,
    ) -> Result<Vec<Session>, StorageError> {
        let rows: Vec<(String, String, Option<String>, String, String, String, String, Option<String>)> =
            sqlx::query_as(
                "SELECT id, project_id, experiment_id, variant, config_id,
                        started_at, ended_at, adapter_session_ref
                 FROM sessions
                 WHERE experiment_id = ?
                 ORDER BY started_at DESC",
            )
            .bind(experiment_id.to_string())
            .fetch_all(self.storage.pool())
            .await?;
        rows.into_iter().map(row_to_session).collect()
    }
}

#[allow(clippy::type_complexity)]
fn row_to_session(
    (id, project_id, experiment_id, variant, config_id, started_at, ended_at, ref_): (
        String,
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        Option<String>,
    ),
) -> Result<Session, StorageError> {
    let parse_ts = |s: &str| {
        DateTime::parse_from_rfc3339(s)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|e| StorageError::Sqlx(sqlx::Error::Decode(Box::new(e))))
    };
    Ok(Session {
        id: SessionId::from_uuid(Uuid::parse_str(&id)?),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&project_id)?),
        experiment_id: experiment_id
            .map(|s| Uuid::parse_str(&s).map(ExperimentId::from_uuid))
            .transpose()?,
        variant: SessionVariant::from_str(&variant)?,
        config_id: ConfigId::from_uuid(Uuid::parse_str(&config_id)?),
        started_at: parse_ts(&started_at)?,
        ended_at: parse_ts(&ended_at)?,
        adapter_session_ref: ref_,
    })
}
```

**Step 3: Register, run, lint, commit**

```bash
git commit -m "feat(storage): SessionRepo with recent-for-project and per-experiment lookups"
```

## Task 2.8 — `SignalRepo`

**Files:**
- Create: `crates/evolve-storage/src/signals.rs`
- Modify: `crates/evolve-storage/src/lib.rs`

**Step 1: Write failing tests (functional + privacy invariant)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
    use crate::projects::{Project, ProjectRepo};
    use crate::sessions::{Session, SessionRepo, SessionVariant};
    use chrono::Utc;
    use evolve_core::agent_config::AgentConfig;
    use evolve_core::ids::{AdapterId, ConfigId, ProjectId, SessionId};

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
        // Privacy invariant: payload_json must never carry source code.
        // This test validates the invariant by attempting to insert known bad content
        // and asserting insertion is rejected at the application layer.
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
        // DB-level CHECK will fire.
        assert!(matches!(err, StorageError::Sqlx(sqlx::Error::Database(_))));
    }
}
```

**Step 2: Add `PayloadRejected` variant to `StorageError`**

In `src/error.rs`, add:

```rust
    /// Privacy-invariant check tripped: payload looked code-like.
    #[error("payload rejected: {0}")]
    PayloadRejected(&'static str),
```

**Step 3: Implement `Signal`, `SignalKind`, `SignalRepo`**

```rust
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
                )))
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
        if let Some(payload) = s.payload_json.as_deref() {
            if looks_like_source_code(payload) {
                return Err(StorageError::PayloadRejected(
                    "payload contains code-like content",
                ));
            }
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
        let rows: Vec<(String, String, String, String, f64, String, Option<String>)> = sqlx::query_as(
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
        let rows: Vec<(String, String, String, String, f64, String, Option<String>)> = sqlx::query_as(
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
        "fn ", "def ", "class ", "function ", "=>", "import ", "#include",
        "public class", "console.log", "println!", "SELECT ", "INSERT INTO",
    ];
    BANNED.iter().any(|needle| payload.contains(needle))
}

#[allow(clippy::type_complexity)]
fn row_to_signal(
    (id, session_id, kind, source, value, recorded_at, payload): (
        String,
        String,
        String,
        String,
        f64,
        String,
        Option<String>,
    ),
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
```

**Step 4: Register module, run tests, lint, commit**

```bash
git commit -m "feat(storage): SignalRepo with privacy-guard on payload_json"
```

## Task 2.9 — Survives-restart integration test

**Files:**
- Create: `crates/evolve-storage/tests/restart.rs`

**Step 1: Write the test**

```rust
//! End-to-end: open → write → drop → reopen → read the same data.

use chrono::Utc;
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId};
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn data_survives_process_restart() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("evolve.db");

    let project_id = ProjectId::new();
    let config_id = ConfigId::new();

    {
        let storage = Storage::open(&db_path).await.unwrap();
        ProjectRepo::new(&storage)
            .insert(&Project {
                id: project_id,
                adapter_id: AdapterId::new("claude-code"),
                root_path: "/tmp/restart-test".into(),
                name: "restart".into(),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await
            .unwrap();

        AgentConfigRepo::new(&storage)
            .insert(&AgentConfigRow {
                id: config_id,
                project_id,
                adapter_id: AdapterId::new("claude-code"),
                role: ConfigRole::Champion,
                fingerprint: 42,
                payload: AgentConfig::default_for("claude-code"),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        // storage goes out of scope → pool closes
    }

    // Reopen and verify.
    let storage = Storage::open(&db_path).await.unwrap();
    let back_project = ProjectRepo::new(&storage)
        .get_by_id(project_id)
        .await
        .unwrap()
        .expect("project should survive restart");
    assert_eq!(back_project.id, project_id);

    let back_cfg = AgentConfigRepo::new(&storage)
        .get_by_id(config_id)
        .await
        .unwrap()
        .expect("config should survive restart");
    assert_eq!(back_cfg.id, config_id);
    assert_eq!(back_cfg.fingerprint, 42);
}

#[tokio::test]
async fn cascade_delete_project_removes_everything_downstream() {
    use evolve_storage::sessions::{Session, SessionRepo, SessionVariant};
    use evolve_storage::signals::{Signal, SignalKind, SignalRepo};
    use evolve_core::ids::{SessionId, SignalId};

    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("evolve.db");
    let storage = Storage::open(&db_path).await.unwrap();

    let project = Project {
        id: ProjectId::new(),
        adapter_id: AdapterId::new("claude-code"),
        root_path: "/tmp/cascade-test".into(),
        name: "cascade".into(),
        created_at: Utc::now(),
        champion_config_id: None,
    };
    ProjectRepo::new(&storage).insert(&project).await.unwrap();

    let cfg = AgentConfigRow {
        id: ConfigId::new(),
        project_id: project.id,
        adapter_id: AdapterId::new("claude-code"),
        role: ConfigRole::Champion,
        fingerprint: 1,
        payload: AgentConfig::default_for("claude-code"),
        created_at: Utc::now(),
    };
    AgentConfigRepo::new(&storage).insert(&cfg).await.unwrap();

    let session = Session {
        id: SessionId::new(),
        project_id: project.id,
        experiment_id: None,
        variant: SessionVariant::Champion,
        config_id: cfg.id,
        started_at: Utc::now(),
        ended_at: Utc::now(),
        adapter_session_ref: None,
    };
    SessionRepo::new(&storage).insert(&session).await.unwrap();

    let signal = Signal {
        id: SignalId::new(),
        session_id: session.id,
        kind: SignalKind::Implicit,
        source: "x".into(),
        value: 0.5,
        recorded_at: Utc::now(),
        payload_json: None,
    };
    SignalRepo::new(&storage).insert(&signal).await.unwrap();

    // Delete project and re-check.
    ProjectRepo::new(&storage).delete(project.id).await.unwrap();

    assert!(ProjectRepo::new(&storage).get_by_id(project.id).await.unwrap().is_none());
    assert!(AgentConfigRepo::new(&storage).get_by_id(cfg.id).await.unwrap().is_none());
    assert!(SignalRepo::new(&storage).list_for_session(session.id).await.unwrap().is_empty());
}
```

**Step 2: Expose repo modules as `pub` in `lib.rs`**

Ensure `src/lib.rs` looks like:

```rust
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod agent_configs;
pub mod error;
pub mod experiments;
pub mod pool;
pub mod projects;
pub mod sessions;
pub mod signals;

pub use error::StorageError;
pub use pool::Storage;
```

**Step 3: Run the test**

Run: `cargo test -p evolve-storage --test restart`
Expected: both tests pass.

**Step 4: Commit**

```bash
git commit -m "test(storage): end-to-end restart + cascade-delete integration tests"
```

## Task 2.10 — Phase 2 verification gates

**Step 1: Full workspace test + lint**

Run all in sequence:
```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: all clean.

**Step 2: Coverage**

Run: `cargo llvm-cov --package evolve-storage --summary-only`
Expected: line coverage ≥ 75%.

If below threshold: identify the untested code path, add a focused test, re-run. Do NOT weaken the threshold.

**Step 3: Verify privacy invariant held**

Run: `cargo test -p evolve-storage payload_json_never_contains_code_like_content`
Expected: PASS (the test itself asserts that inserting a code-like payload yields `StorageError::PayloadRejected`).

**Step 4: No commit (verification-only).**

**PHASE 2 COMPLETE.**

---

# PHASE 3 — Bayesian Promotion + Signal Aggregation

**Goal:** The math engine. Pure functions, no I/O. Lives entirely in `evolve-core/src/promotion.rs`.

## Phase 3 Design Decisions (binding for all tasks below)

- **Location:** new module `evolve-core::promotion`. No dependency on `evolve-storage` — math crate stays I/O-free so it can be reused in bindings and never touches a runtime.
- **`SignalInput` type is crate-local** — a minimal `{ kind, value }` struct distinct from `evolve_storage::Signal`. Callers in the CLI/adapter layer translate storage rows into math inputs.
- **Aggregation rule:** weighted arithmetic mean. Default weights: explicit = 5.0, implicit = 1.0. Values clipped to `[0.0, 1.0]` before weighting. Empty input returns 0.5 (neutral prior).
- **Binary win/loss conversion:** aggregated session score `≥ 0.5` → win, else loss. Simple and well-understood; the beta-binomial model assumes Bernoulli trials.
- **Posterior model:** Beta(1 + wins, 1 + losses) per arm, uniform Jeffreys-like prior. Monte Carlo with a user-provided `Rng` so tests are seed-reproducible.
- **Decision thresholds (defaults):** `min_sessions_per_arm = 20`, `promote_threshold = 0.95`, `mc_samples = 10_000`.
- **No criterion benchmark:** plan originally proposed `benches/promotion.rs` but criterion adds ~300 crates to the build. Use a plain `#[test]` that measures `Instant::elapsed()` and asserts `< 5 ms` on 100 sessions/arm at 10k MC samples. Zero new deps; still catches regressions.
- **New workspace dep:** `rand_distr = "0.4"` — for `Beta::new(...).sample(rng)`.

## Task 3.1 — `SignalInput`, `SignalKind`, `AggregationConfig`

**Files:**
- Modify: root `Cargo.toml` (add `rand_distr` to workspace.dependencies)
- Modify: `crates/evolve-core/Cargo.toml` (take `rand_distr.workspace = true`)
- Create: `crates/evolve-core/src/promotion.rs`
- Modify: `crates/evolve-core/src/lib.rs` (add `pub mod promotion;`)

**Step 1: Add `rand_distr` to workspace dependencies**

Append to root `Cargo.toml`, in `[workspace.dependencies]`:
```toml
rand_distr = "0.4"
```

In `crates/evolve-core/Cargo.toml`, add under `[dependencies]`:
```toml
rand_distr.workspace = true
```

**Step 2: Create `promotion.rs` with minimal types + weights**

```rust
//! Signal aggregation + Bayesian champion-vs-challenger promotion math.
//!
//! Pure functions, no I/O. Callers (CLI, adapters) translate
//! [`evolve_storage::Signal`](../../../evolve-storage/signals/struct.Signal.html)
//! rows into [`SignalInput`] before calling into this module.

/// Whether a signal was contributed explicitly by the user
/// (`evolve good`/`bad`/`thumbs`) or inferred implicitly by an adapter
/// from the session log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// User explicitly graded the session.
    Explicit,
    /// Inferred from adapter session log.
    Implicit,
}

/// One normalized fitness signal feeding into [`aggregate`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalInput {
    /// Source category — controls weighting.
    pub kind: SignalKind,
    /// Score in `[0.0, 1.0]`. Out-of-range values are clamped.
    pub value: f64,
}

/// Per-kind weights used by [`aggregate`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AggregationConfig {
    /// Weight applied to explicit user signals. Default 5.0.
    pub explicit_weight: f64,
    /// Weight applied to adapter-inferred implicit signals. Default 1.0.
    pub implicit_weight: f64,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            explicit_weight: 5.0,
            implicit_weight: 1.0,
        }
    }
}

impl SignalInput {
    /// Weight this signal carries under the given aggregation config.
    pub fn weight(&self, config: &AggregationConfig) -> f64 {
        match self.kind {
            SignalKind::Explicit => config.explicit_weight,
            SignalKind::Implicit => config.implicit_weight,
        }
    }
}
```

**Step 3: Register module in `lib.rs`**

Add `pub mod promotion;` alongside the existing module declarations.

**Step 4: Write the invariant tests (append to `promotion.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_are_five_to_one() {
        let cfg = AggregationConfig::default();
        assert_eq!(cfg.explicit_weight, 5.0);
        assert_eq!(cfg.implicit_weight, 1.0);
    }

    #[test]
    fn explicit_signal_weighs_five_times_implicit() {
        let cfg = AggregationConfig::default();
        let e = SignalInput { kind: SignalKind::Explicit, value: 1.0 };
        let i = SignalInput { kind: SignalKind::Implicit, value: 1.0 };
        assert_eq!(e.weight(&cfg) / i.weight(&cfg), 5.0);
    }
}
```

**Step 5: Run tests + gates**

```bash
cargo test -p evolve-core promotion
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

**Step 6: Commit**

```bash
git commit -m "feat(core): promotion module scaffolding with SignalInput and AggregationConfig"
```

## Task 3.2 — `aggregate()`

**Files:**
- Modify: `crates/evolve-core/src/promotion.rs`

**Step 1: Write failing tests first**

Append to the tests module:
```rust
    #[test]
    fn aggregate_empty_returns_neutral_half() {
        assert_eq!(aggregate(&[], &AggregationConfig::default()), 0.5);
    }

    #[test]
    fn aggregate_single_explicit_1_is_1() {
        let signals = [SignalInput { kind: SignalKind::Explicit, value: 1.0 }];
        assert_eq!(aggregate(&signals, &AggregationConfig::default()), 1.0);
    }

    #[test]
    fn aggregate_single_implicit_0_is_0() {
        let signals = [SignalInput { kind: SignalKind::Implicit, value: 0.0 }];
        assert_eq!(aggregate(&signals, &AggregationConfig::default()), 0.0);
    }

    #[test]
    fn aggregate_clips_out_of_range_values() {
        // value=2.0 should clip to 1.0 before weighting
        let signals = [SignalInput { kind: SignalKind::Implicit, value: 2.0 }];
        assert_eq!(aggregate(&signals, &AggregationConfig::default()), 1.0);
    }

    #[test]
    fn aggregate_weighted_mean_matches_hand_calculation() {
        // 1 explicit at 0.0 (weight 5) + 2 implicit at 1.0 (weight 1 each)
        // weighted mean = (5*0 + 1*1 + 1*1) / (5 + 1 + 1) = 2/7 ≈ 0.2857
        let signals = [
            SignalInput { kind: SignalKind::Explicit, value: 0.0 },
            SignalInput { kind: SignalKind::Implicit, value: 1.0 },
            SignalInput { kind: SignalKind::Implicit, value: 1.0 },
        ];
        let got = aggregate(&signals, &AggregationConfig::default());
        assert!((got - 2.0 / 7.0).abs() < 1e-9, "got {got}");
    }

    #[test]
    fn aggregate_single_explicit_dominates_many_implicit() {
        // Product-level intuition: one "bad" explicit outweighs reasonable implicit evidence.
        let signals = [
            SignalInput { kind: SignalKind::Explicit, value: 0.0 },
            SignalInput { kind: SignalKind::Implicit, value: 1.0 },
            SignalInput { kind: SignalKind::Implicit, value: 1.0 },
            SignalInput { kind: SignalKind::Implicit, value: 1.0 },
        ];
        let got = aggregate(&signals, &AggregationConfig::default());
        // (0*5 + 1*1 + 1*1 + 1*1) / (5+1+1+1) = 3/8 = 0.375 — still < 0.5 threshold
        assert!(got < 0.5, "explicit 0.0 should pull aggregate below 0.5, got {got}");
    }
```

**Step 2: Run tests — expected compile-fail (no `aggregate` yet)**

**Step 3: Implement**

Insert before the tests module:
```rust
/// Collapse a session's signals into a single fitness score in `[0.0, 1.0]`.
///
/// Uses the weighted arithmetic mean. Values are clamped to `[0.0, 1.0]`
/// before weighting. Empty input returns `0.5` (neutral prior).
pub fn aggregate(signals: &[SignalInput], config: &AggregationConfig) -> f64 {
    if signals.is_empty() {
        return 0.5;
    }
    let mut numerator = 0.0;
    let mut denominator = 0.0;
    for s in signals {
        let w = s.weight(config);
        numerator += w * s.value.clamp(0.0, 1.0);
        denominator += w;
    }
    (numerator / denominator).clamp(0.0, 1.0)
}
```

**Step 4: Run tests — all pass.**

**Step 5: Gates + commit**

```bash
git commit -m "feat(core): aggregate() with weighted mean, clamping, and neutral-prior on empty"
```

## Task 3.3 — `posterior_probability()` via Monte Carlo

**Files:**
- Modify: `crates/evolve-core/src/promotion.rs`

**Step 1: Write failing tests**

```rust
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn rng() -> ChaCha8Rng {
        ChaCha8Rng::seed_from_u64(42)
    }

    #[test]
    fn posterior_obvious_challenger_win_exceeds_threshold() {
        // Champion: 5 wins out of 20. Challenger: 18 wins out of 20. P(chall > champ) ≈ 1.
        let champion: Vec<f64> = (0..20).map(|i| if i < 5 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..20).map(|i| if i < 18 { 1.0 } else { 0.0 }).collect();
        let p = posterior_probability(&champion, &challenger, 10_000, &mut rng());
        assert!(p > 0.95, "expected P(chall > champ) > 0.95, got {p}");
    }

    #[test]
    fn posterior_obvious_champion_win_stays_below_threshold() {
        let champion: Vec<f64> = (0..20).map(|i| if i < 18 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..20).map(|i| if i < 5 { 1.0 } else { 0.0 }).collect();
        let p = posterior_probability(&champion, &challenger, 10_000, &mut rng());
        assert!(p < 0.05, "expected P(chall > champ) < 0.05, got {p}");
    }

    #[test]
    fn posterior_tied_evidence_stays_near_half() {
        let champion: Vec<f64> = (0..40).map(|i| if i < 20 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..40).map(|i| if i < 20 { 1.0 } else { 0.0 }).collect();
        let p = posterior_probability(&champion, &challenger, 10_000, &mut rng());
        assert!((p - 0.5).abs() < 0.05, "expected P near 0.5, got {p}");
    }

    #[test]
    fn posterior_is_deterministic_under_same_seed() {
        let champion: Vec<f64> = (0..10).map(|_| 0.6).collect();
        let challenger: Vec<f64> = (0..10).map(|_| 0.7).collect();
        let p1 = posterior_probability(&champion, &challenger, 5_000, &mut rng());
        let p2 = posterior_probability(&champion, &challenger, 5_000, &mut rng());
        assert_eq!(p1, p2);
    }
```

**Step 2: Implement**

```rust
use rand::Rng;
use rand_distr::{Beta, Distribution};

/// Count scores ≥ 0.5 as wins, rest as losses.
fn wins_losses(scores: &[f64]) -> (u32, u32) {
    let wins: u32 = scores.iter().filter(|&&s| s >= 0.5).count() as u32;
    let losses = scores.len() as u32 - wins;
    (wins, losses)
}

/// Monte Carlo estimate of `P(challenger > champion)` under beta-binomial
/// posteriors: `Beta(1 + wins, 1 + losses)` per arm (uniform Jeffreys-like prior).
///
/// Each session score is binarized at 0.5 before counting.
pub fn posterior_probability<R: Rng>(
    champion_scores: &[f64],
    challenger_scores: &[f64],
    samples: u32,
    rng: &mut R,
) -> f64 {
    let (cw, cl) = wins_losses(champion_scores);
    let (hw, hl) = wins_losses(challenger_scores);
    let champ = Beta::new(1.0 + cw as f64, 1.0 + cl as f64).expect("valid Beta params");
    let chall = Beta::new(1.0 + hw as f64, 1.0 + hl as f64).expect("valid Beta params");
    let mut hits: u32 = 0;
    for _ in 0..samples {
        let a: f64 = champ.sample(rng);
        let b: f64 = chall.sample(rng);
        if b > a {
            hits += 1;
        }
    }
    hits as f64 / samples as f64
}
```

**Step 3: Run tests — expect 4/4 pass.**

**Step 4: Gates + commit**

```bash
git commit -m "feat(core): posterior_probability via Monte Carlo over Beta posteriors"
```

## Task 3.4 — `promotion_decision()`

**Files:**
- Modify: `crates/evolve-core/src/promotion.rs`

**Step 1: Write failing tests**

```rust
    #[test]
    fn decision_needs_more_data_when_either_arm_is_thin() {
        let champion: Vec<f64> = vec![1.0; 5];
        let challenger: Vec<f64> = vec![0.0; 20];
        let cfg = PromotionConfig::default();
        let d = promotion_decision(&champion, &challenger, &cfg, &mut rng());
        assert!(matches!(d, Decision::NeedMoreData { .. }));
    }

    #[test]
    fn decision_promotes_obvious_winner() {
        let champion: Vec<f64> = (0..25).map(|i| if i < 5 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..25).map(|i| if i < 23 { 1.0 } else { 0.0 }).collect();
        let cfg = PromotionConfig::default();
        let d = promotion_decision(&champion, &challenger, &cfg, &mut rng());
        match d {
            Decision::Promote { posterior } => {
                assert!(posterior >= cfg.promote_threshold);
            }
            other => panic!("expected Promote, got {other:?}"),
        }
    }

    #[test]
    fn decision_holds_when_evidence_is_tied() {
        let champion: Vec<f64> = (0..30).map(|i| if i < 15 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..30).map(|i| if i < 15 { 1.0 } else { 0.0 }).collect();
        let cfg = PromotionConfig::default();
        let d = promotion_decision(&champion, &challenger, &cfg, &mut rng());
        assert!(matches!(d, Decision::Hold { .. }));
    }
```

**Step 2: Implement**

```rust
/// Configuration for [`promotion_decision`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PromotionConfig {
    /// Minimum sessions required in each arm before any decision is made.
    pub min_sessions_per_arm: usize,
    /// Posterior threshold above which the challenger is promoted.
    pub promote_threshold: f64,
    /// Monte Carlo sample count for [`posterior_probability`].
    pub mc_samples: u32,
}

impl Default for PromotionConfig {
    fn default() -> Self {
        Self {
            min_sessions_per_arm: 20,
            promote_threshold: 0.95,
            mc_samples: 10_000,
        }
    }
}

/// Outcome of a promotion evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    /// At least one arm has too few sessions to decide yet.
    NeedMoreData {
        /// Sessions in the thinner arm.
        sessions_each: usize,
        /// Minimum required per arm.
        required: usize,
    },
    /// Enough data, but posterior below threshold. Keep running.
    Hold {
        /// Current estimated `P(challenger > champion)`.
        posterior: f64,
    },
    /// Promote: posterior crossed threshold.
    Promote {
        /// Current estimated `P(challenger > champion)`.
        posterior: f64,
    },
}

/// Evaluate whether the challenger should be promoted, held, or needs more data.
pub fn promotion_decision<R: Rng>(
    champion_scores: &[f64],
    challenger_scores: &[f64],
    config: &PromotionConfig,
    rng: &mut R,
) -> Decision {
    let champ_n = champion_scores.len();
    let chall_n = challenger_scores.len();
    if champ_n < config.min_sessions_per_arm || chall_n < config.min_sessions_per_arm {
        return Decision::NeedMoreData {
            sessions_each: champ_n.min(chall_n),
            required: config.min_sessions_per_arm,
        };
    }
    let posterior =
        posterior_probability(champion_scores, challenger_scores, config.mc_samples, rng);
    if posterior >= config.promote_threshold {
        Decision::Promote { posterior }
    } else {
        Decision::Hold { posterior }
    }
}
```

**Step 3: Run tests + gates + commit**

```bash
git commit -m "feat(core): promotion_decision with NeedMoreData/Hold/Promote outcomes"
```

## Task 3.5 — Property tests

**Files:**
- Modify: `crates/evolve-core/src/promotion.rs`

**Step 1: Add a `proptests` sub-module at the bottom of the file**

```rust
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn arb_scores(max_n: usize) -> impl Strategy<Value = Vec<f64>> {
        prop::collection::vec(prop_oneof![Just(0.0_f64), Just(1.0_f64)], 0..max_n)
    }

    proptest! {
        /// `aggregate` always returns a value in `[0.0, 1.0]` for any inputs.
        #[test]
        fn aggregate_is_in_unit_interval(
            signals in prop::collection::vec(
                (prop::bool::ANY, -10.0_f64..10.0_f64)
                    .prop_map(|(is_explicit, v)| SignalInput {
                        kind: if is_explicit { SignalKind::Explicit } else { SignalKind::Implicit },
                        value: v,
                    }),
                0..50,
            ),
        ) {
            let out = aggregate(&signals, &AggregationConfig::default());
            prop_assert!((0.0..=1.0).contains(&out), "got {out}");
        }

        /// `promotion_decision` never returns `Promote` with a posterior below threshold.
        #[test]
        fn decision_never_promotes_below_threshold(
            champion in arb_scores(60),
            challenger in arb_scores(60),
        ) {
            let cfg = PromotionConfig::default();
            let mut r = ChaCha8Rng::seed_from_u64(1);
            let d = promotion_decision(&champion, &challenger, &cfg, &mut r);
            if let Decision::Promote { posterior } = d {
                prop_assert!(posterior >= cfg.promote_threshold);
            }
        }

        /// Posterior probability is always in `[0.0, 1.0]`.
        #[test]
        fn posterior_is_in_unit_interval(
            champion in arb_scores(50),
            challenger in arb_scores(50),
        ) {
            let mut r = ChaCha8Rng::seed_from_u64(7);
            let p = posterior_probability(&champion, &challenger, 1_000, &mut r);
            prop_assert!((0.0..=1.0).contains(&p), "got {p}");
        }
    }
}
```

**Step 2: Run, gate, commit**

```bash
git commit -m "test(core): proptest invariants for aggregate, posterior, and decision"
```

## Task 3.6 — Runtime sanity test

**Files:**
- Modify: `crates/evolve-core/src/promotion.rs`

**Step 1: Add a timing-assert test**

```rust
    #[test]
    fn decision_finishes_in_reasonable_time_for_realistic_input() {
        use rand::SeedableRng;
        use rand_chacha::ChaCha8Rng;

        let champion: Vec<f64> = (0..100).map(|i| if i < 60 { 1.0 } else { 0.0 }).collect();
        let challenger: Vec<f64> = (0..100).map(|i| if i < 70 { 1.0 } else { 0.0 }).collect();
        let cfg = PromotionConfig::default();
        let mut r = ChaCha8Rng::seed_from_u64(42);

        let start = std::time::Instant::now();
        let _ = promotion_decision(&champion, &challenger, &cfg, &mut r);
        let elapsed = start.elapsed();

        // Generous cap: real budget is ~1ms, but debug builds on slow CI can swell
        // this by 5-10x. Failing here is a real red flag, not a flake.
        assert!(
            elapsed.as_millis() < 50,
            "promotion_decision took {elapsed:?}; expected < 50ms",
        );
    }
```

**Step 2: Phase 3 verification gate**

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo llvm-cov --package evolve-core --summary-only  # expect ≥80%
```

**PHASE 3 COMPLETE.**

---

# PHASE 4 — `evolve-llm` Minimal Client

**Goal:** ONE LLM client for ONE purpose: occasionally generating challenger configs.

## Phase 4 Design Decisions (binding for all tasks below)

- **HTTP client:** `reqwest` (rustls-tls, no OpenSSL). Used for *both* Anthropic and Ollama — we do **not** take `async-openai` (~50 extra deps for a single `POST`).
- **Ollama endpoint:** `http://localhost:11434/api/chat` (native) — *not* the OpenAI-compat `/v1/chat/completions`. The native endpoint is simpler and documented. No auth.
- **Anthropic endpoint:** `POST https://api.anthropic.com/v1/messages` with header `anthropic-version: 2023-06-01`.
- **Model targets:** Haiku = `"claude-haiku-4-5-20251001"` (current latest Haiku per assistant instructions). Ollama model is user-configurable via `OLLAMA_MODEL` env var, default `"qwen2.5-coder:7b"`.
- **Retry policy:** 1 retry on connection errors, 5xx responses, or 429. No retry on 4xx-except-429. Fixed 500ms backoff (tiny volume — exponential backoff is overkill).
- **Error type:** crate-local `LlmError` with variants: `NoApiKey`, `Http(reqwest::Error)`, `UnexpectedStatus { status, body }`, `ParseFailure(serde_json::Error)`, `NoLlmAvailable`.
- **CompletionResult:** `{ text: String, usage: TokenUsage { input: u32, output: u32 } }`. Usage populated from provider's reported token counts; `0` if unknown.
- **Cassette tests:** `wiremock = "0.6"` — a MockServer stand-in for the provider. Default test suite runs entirely against mocks, zero network. `--ignored` smoke tests hit real providers; not run in default CI.
- **Cost tracker:** in-memory `CostTracker` with atomic accumulators for total input/output tokens per-model. Price tables hard-coded for Haiku (`$0.25/M in, $1.25/M out`). Ollama = $0. Expose `spent_micro_cents() -> u64` and `log_session()` that emits a `tracing::info!` line. No enforcement.

## Task 4.1 — Crate skeleton + `LlmError`

**Files:**
- Modify: root `Cargo.toml` (add `reqwest`, `wiremock` to workspace.dependencies; add `"crates/evolve-llm"` to members)
- Create: `crates/evolve-llm/Cargo.toml`
- Create: `crates/evolve-llm/src/lib.rs`
- Create: `crates/evolve-llm/src/error.rs`

**Step 1: Workspace deps — append to root `Cargo.toml`**

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
wiremock = "0.6"
```

Add `"crates/evolve-llm"` to `[workspace] members`.

**Step 2: Create `crates/evolve-llm/Cargo.toml`**

```toml
[package]
name = "evolve-llm"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Minimal LLM client (Anthropic Haiku + Ollama) for occasional challenger generation"

[dependencies]
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
async-trait.workspace = true
tokio = { workspace = true, features = ["macros", "rt"] }

[dev-dependencies]
tokio = { workspace = true, features = ["macros", "rt-multi-thread"] }
wiremock.workspace = true
```

**Step 3: Create `crates/evolve-llm/src/error.rs`**

```rust
//! Unified error type for the LLM client crate.

use thiserror::Error;

/// Errors produced by LLM clients.
#[derive(Debug, Error)]
pub enum LlmError {
    /// Environment variable `ANTHROPIC_API_KEY` was not set when an Anthropic
    /// client was requested.
    #[error("ANTHROPIC_API_KEY not set")]
    NoApiKey,
    /// Transport or TLS failure.
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    /// Server returned a non-2xx status after retries were exhausted.
    #[error("unexpected status {status}: {body}")]
    UnexpectedStatus {
        /// HTTP status code.
        status: u16,
        /// Body snippet (truncated to 512 chars).
        body: String,
    },
    /// Response body did not match the expected schema.
    #[error("parse: {0}")]
    ParseFailure(#[from] serde_json::Error),
    /// Neither Ollama nor Anthropic was reachable / configured.
    #[error("no llm available")]
    NoLlmAvailable,
}
```

**Step 4: Create `crates/evolve-llm/src/lib.rs`**

```rust
//! evolve-llm: minimal LLM client for occasional challenger generation.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;

pub use error::LlmError;
```

**Step 5: Verify + commit**

```bash
cargo check -p evolve-llm
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/evolve-llm
git commit -m "feat(llm): scaffold evolve-llm crate with LlmError"
```

## Task 4.2 — `LlmClient` trait + `CompletionResult`

**Files:**
- Create: `crates/evolve-llm/src/client.rs`
- Modify: `crates/evolve-llm/src/lib.rs` (add `pub mod client;` + re-exports)

**Step 1: Define the trait and result types**

`src/client.rs`:
```rust
//! The `LlmClient` trait and its shared types.

use crate::error::LlmError;
use async_trait::async_trait;

/// Token usage reported by the provider. Both fields are 0 if unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    /// Input tokens billed.
    pub input: u32,
    /// Output tokens billed.
    pub output: u32,
}

/// One completion response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResult {
    /// Assistant text.
    pub text: String,
    /// Token usage.
    pub usage: TokenUsage,
}

/// Shared interface for LLM clients.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Run a single non-streaming completion.
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<CompletionResult, LlmError>;

    /// Stable identifier used by the cost tracker price table.
    fn model_id(&self) -> &str;
}
```

**Step 2: Register + test**

In `lib.rs`:
```rust
pub mod client;
pub use client::{CompletionResult, LlmClient, TokenUsage};
```

Add a trivial test in `client.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_default_is_zero() {
        let u = TokenUsage::default();
        assert_eq!(u.input, 0);
        assert_eq!(u.output, 0);
    }
}
```

**Step 3: Commit**

```bash
git commit -m "feat(llm): LlmClient trait with CompletionResult + TokenUsage"
```

## Task 4.3 — `AnthropicHaikuClient`

**Files:**
- Create: `crates/evolve-llm/src/anthropic.rs`
- Modify: `crates/evolve-llm/src/lib.rs`

**Step 1: Write the wiremock cassette test FIRST**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SAMPLE_RESPONSE: &str = r#"{
        "id": "msg_01",
        "type": "message",
        "role": "assistant",
        "model": "claude-haiku-4-5-20251001",
        "content": [{"type":"text","text":"Hello from mock Haiku"}],
        "usage": {"input_tokens": 12, "output_tokens": 5}
    }"#;

    #[tokio::test]
    async fn happy_path_parses_text_and_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", "2023-06-01"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RESPONSE))
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let got = client.complete("hi", 16).await.unwrap();
        assert_eq!(got.text, "Hello from mock Haiku");
        assert_eq!(got.usage.input, 12);
        assert_eq!(got.usage.output, 5);
    }

    #[tokio::test]
    async fn retries_once_on_5xx_then_succeeds() {
        let server = MockServer::start().await;
        // First call: 503. Second call: 200.
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SAMPLE_RESPONSE))
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let got = client.complete("hi", 16).await.unwrap();
        assert_eq!(got.text, "Hello from mock Haiku");
    }

    #[tokio::test]
    async fn gives_up_after_second_5xx() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let err = client.complete("hi", 16).await.unwrap_err();
        assert!(matches!(err, LlmError::UnexpectedStatus { status: 503, .. }));
    }

    #[tokio::test]
    async fn does_not_retry_on_4xx_except_429() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_string(r#"{"error":"bad"}"#))
            .expect(1) // exactly one hit — no retry
            .mount(&server)
            .await;

        let client = AnthropicHaikuClient::with_endpoint("test-key", server.uri());
        let err = client.complete("hi", 16).await.unwrap_err();
        assert!(matches!(err, LlmError::UnexpectedStatus { status: 400, .. }));
    }
}
```

**Step 2: Implement `AnthropicHaikuClient`**

```rust
//! Anthropic Messages API client, Haiku-only.

use crate::client::{CompletionResult, LlmClient, TokenUsage};
use crate::error::LlmError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-haiku-4-5-20251001";
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// Minimal client for Anthropic's Messages API, wired for Haiku.
#[derive(Debug, Clone)]
pub struct AnthropicHaikuClient {
    api_key: String,
    endpoint: String,
    model: String,
    http: reqwest::Client,
}

impl AnthropicHaikuClient {
    /// Build from the `ANTHROPIC_API_KEY` env var, using the production endpoint.
    pub fn from_env() -> Result<Self, LlmError> {
        let key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| LlmError::NoApiKey)?;
        Ok(Self::with_endpoint(key, DEFAULT_ENDPOINT))
    }

    /// Construct with a caller-supplied endpoint (used by cassette tests).
    pub fn with_endpoint(api_key: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: endpoint.into(),
            model: DEFAULT_MODEL.to_string(),
            http: reqwest::Client::new(),
        }
    }
}

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<UsageReport>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct UsageReport {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[async_trait]
impl LlmClient for AnthropicHaikuClient {
    async fn complete(&self, prompt: &str, max_tokens: u32) -> Result<CompletionResult, LlmError> {
        let url = format!("{}/v1/messages", self.endpoint);
        let body = MessagesRequest {
            model: &self.model,
            max_tokens,
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
        };

        for attempt in 0..=1 {
            let req = self
                .http
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&body);
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let raw = resp.text().await?;
                        let parsed: MessagesResponse = serde_json::from_str(&raw)?;
                        let text = parsed
                            .content
                            .into_iter()
                            .filter(|b| b.kind == "text")
                            .filter_map(|b| b.text)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let usage = parsed
                            .usage
                            .map(|u| TokenUsage {
                                input: u.input_tokens,
                                output: u.output_tokens,
                            })
                            .unwrap_or_default();
                        return Ok(CompletionResult { text, usage });
                    }
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if retryable && attempt == 0 {
                        tokio::time::sleep(RETRY_DELAY).await;
                        continue;
                    }
                    let body = resp.text().await.unwrap_or_default();
                    let snippet = if body.len() > 512 { &body[..512] } else { &body };
                    return Err(LlmError::UnexpectedStatus {
                        status: status.as_u16(),
                        body: snippet.to_string(),
                    });
                }
                Err(e) if attempt == 0 && e.is_connect() => {
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(e) => return Err(LlmError::Http(e)),
            }
        }
        unreachable!("retry loop exits via return")
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}
```

**Step 3: Register + test + commit**

```bash
git commit -m "feat(llm): AnthropicHaikuClient with retry-once on 5xx/429"
```

## Task 4.4 — `OllamaClient`

**Files:**
- Create: `crates/evolve-llm/src/ollama.rs`

Similar shape to Anthropic. Endpoint `POST /api/chat`. Request body:
```json
{
  "model": "qwen2.5-coder:7b",
  "messages": [{"role":"user","content":"..."}],
  "stream": false,
  "options": {"num_predict": 128}
}
```
Response:
```json
{
  "model": "qwen2.5-coder:7b",
  "message": {"role":"assistant","content":"..."},
  "prompt_eval_count": 12,
  "eval_count": 5
}
```

**Step 1:** Write cassette tests mirroring 4.3 (happy path, retry, 4xx no-retry).
**Step 2:** Implement with same retry pattern as Anthropic.
**Step 3:** Commit.

## Task 4.5 — `pick_default_client()`

**Files:**
- Create: `crates/evolve-llm/src/factory.rs`

**Step 1:** Implement:
```rust
/// Select an LLM client: try Ollama first (zero cost, no auth), fall back to
/// Anthropic Haiku if `ANTHROPIC_API_KEY` is set, else return `NoLlmAvailable`.
pub async fn pick_default_client() -> Result<Box<dyn LlmClient>, LlmError> {
    // Cheap reachability probe: GET /api/version on Ollama default port
    let ollama_url = std::env::var("OLLAMA_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:11434".to_string());
    let reachable = reqwest::Client::new()
        .get(format!("{ollama_url}/api/version"))
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    if reachable {
        return Ok(Box::new(OllamaClient::with_endpoint(ollama_url)));
    }
    if let Ok(client) = AnthropicHaikuClient::from_env() {
        return Ok(Box::new(client));
    }
    Err(LlmError::NoLlmAvailable)
}
```

**Step 2:** Test with wiremock — both success branches + the `NoLlmAvailable` fall-through.
**Step 3:** Commit.

## Task 4.6 — `--ignored` smoke tests against real providers

**Files:**
- Create: `crates/evolve-llm/tests/smoke.rs`

**Step 1:** Write `#[ignore]`-gated tests:
```rust
#[tokio::test]
#[ignore = "requires real Ollama running locally"]
async fn smoke_ollama_round_trip() { /* ... */ }

#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY"]
async fn smoke_anthropic_round_trip() { /* ... */ }
```

These are developer-invoked, not CI-invoked. Documented in the crate README.
**Step 2:** Commit.

## Task 4.7 — `CostTracker`

**Files:**
- Create: `crates/evolve-llm/src/cost.rs`

**Step 1:** Implement:
```rust
//! In-memory token cost tracker. Purely observational -- no enforcement.

use std::sync::atomic::{AtomicU64, Ordering};

/// Price table entry (micro-cents per token).
#[derive(Debug, Clone, Copy)]
pub struct Price {
    /// Micro-cents per input token.
    pub input_per_token: u64,
    /// Micro-cents per output token.
    pub output_per_token: u64,
}

impl Price {
    /// Haiku 4.5: $0.25/M input, $1.25/M output => 0.25 / 1_000_000 * 100_000 micro-cents
    /// per token = 25 micro-cents/M-tokens input, 125 output. Stored scaled up:
    pub const HAIKU: Self = Self {
        input_per_token: 25,
        output_per_token: 125,
    };
    /// Ollama is free.
    pub const OLLAMA: Self = Self {
        input_per_token: 0,
        output_per_token: 0,
    };
}

/// Accumulates token usage across calls. Thread-safe.
#[derive(Debug, Default)]
pub struct CostTracker {
    input_tokens: AtomicU64,
    output_tokens: AtomicU64,
    micro_cents: AtomicU64,
}

impl CostTracker {
    /// Fresh tracker at zero.
    pub fn new() -> Self { Self::default() }

    /// Record one call's usage. `price` is per-token (see [`Price`]).
    pub fn record(&self, usage: crate::TokenUsage, price: Price) {
        self.input_tokens.fetch_add(usage.input as u64, Ordering::Relaxed);
        self.output_tokens.fetch_add(usage.output as u64, Ordering::Relaxed);
        let cost = (usage.input as u64) * price.input_per_token
            + (usage.output as u64) * price.output_per_token;
        self.micro_cents.fetch_add(cost, Ordering::Relaxed);
    }

    /// Total spent so far, in micro-cents (1 cent = 100_000 micro-cents? no -
    /// 1 cent = 10_000 micro-cents given our scaling. Consumer should divide.)
    pub fn spent_micro_cents(&self) -> u64 {
        self.micro_cents.load(Ordering::Relaxed)
    }

    /// Emit a `tracing::info!` line with accumulated cost.
    pub fn log_session(&self) {
        let mc = self.spent_micro_cents();
        let input = self.input_tokens.load(Ordering::Relaxed);
        let output = self.output_tokens.load(Ordering::Relaxed);
        tracing::info!(
            target: "evolve::cost",
            input_tokens = input,
            output_tokens = output,
            micro_cents = mc,
            "evolve llm usage"
        );
    }
}
```

**Step 2:** Unit tests: record twice, totals add correctly; Haiku price math matches hand-calc; Ollama records zero cost.
**Step 3:** Phase 4 verification gates.
**Step 4:** Commit.

**PHASE 4 COMPLETE.**

---

# PHASE 5 — Mutation Operators

**Goal:** Five mutators that take an `AgentConfig` and produce a varied "challenger" config. Each generation, ONE is applied (chosen probabilistically).

**Tasks:**

- 5.1 — `Mutator` trait: `async fn mutate(&self, parent: &AgentConfig, llm: &dyn LlmClient, rng: &mut dyn RngCore) -> Result<AgentConfig>`.
- 5.2 — `LlmRewriteMutator` — calls `llm.complete()` with prompt template asking for a small variation of the system_prompt_prefix.
- 5.3 — `BehavioralRulesMutator` — adds, removes, or rephrases ONE rule from a curated pool of 30 candidate rules.
- 5.4 — `ResponseStyleMutator` — cycles through `{Terse, Normal, Verbose}`.
- 5.5 — `ModelPrefMutator` — swaps to a neighboring model from a per-adapter pool.
- 5.6 — `ToolPermissionsMutator` — toggles ONE permission from the adapter's allow-list.
- 5.7 — `MutatorPicker` — weighted random selection (50/15/15/10/10).
- 5.8 — Tests: each mutator returns a valid AgentConfig that differs from input in exactly one axis.
- 5.9 — Test: with seeded RNG, sequence of mutations is deterministic.

**PHASE 5 COMPLETE.**

---

# PHASE 6 — Adapter Trait + Claude Code Adapter

**Goal:** The Adapter trait + the most important adapter (Claude Code). This is THE load-bearing milestone — if Claude Code adapter doesn't work end-to-end, nothing else matters.

**Tasks:**

- 6.1 — Create `crates/evolve-adapters/`.
- 6.2 — Define `Adapter` trait (see design doc 4.4).
- 6.3 — `AdapterRegistry` — runtime lookup `&str → Box<dyn Adapter>`.
- 6.4 — `ClaudeCodeAdapter::detect()` — checks for `.claude/`, `CLAUDE.md`, etc. in repo or `~/.claude/`.
- 6.5 — `ClaudeCodeAdapter::install()` — modifies `~/.claude/settings.json` to add a `Stop` hook calling `evolve record-claude-code <transcript-path>`. Idempotent. Detects user's existing hook config and merges safely.
- 6.6 — `ClaudeCodeAdapter::apply_config()` — writes managed section into `~/.claude/CLAUDE.md` (or per-project `CLAUDE.md` for project-specific configs) bracketed by `<!-- evolve:start -->` / `<!-- evolve:end -->` markers. Never touches content outside markers.
- 6.7 — `ClaudeCodeAdapter::parse_session()` — reads transcript JSONL, extracts:
  - Session duration (very short = abandoned)
  - User /clear command (negative)
  - Bash exit codes for test/lint commands (positive if 0)
  - User feedback patterns ("redo", "no, that's wrong", "thanks!", etc. — regex-matched)
  - Subagent invocations and their outcomes
  Returns `Vec<Signal>`.
- 6.8 — Test fixtures: 5 hand-crafted Claude Code session transcripts in `tests/fixtures/claude-code/`. Test `parse_session` extracts expected signals from each.
- 6.9 — Idempotency test: install → install again → settings.json unchanged the second time.
- 6.10 — Restoration test: `forget` removes the managed section and restores original settings.json.

**PHASE 6 COMPLETE.**

---

# PHASE 7 — Cursor Adapter

**Tasks:**

- 7.1 — `CursorAdapter::detect()` — checks for `.vscode/`, `.cursorrules`, or `cursor` in PATH.
- 7.2 — `CursorAdapter::install()` — writes initial `.cursorrules` (managed-section-bracketed) AND prints instructions for the user to point Cursor's "Custom OpenAI base URL" at `http://localhost:7777` (the Evolve proxy) for live signal capture.
- 7.3 — `CursorAdapter::apply_config()` — updates `.cursorrules` managed section.
- 7.4 — `CursorAdapter::parse_session()` — for v1, primary signal source is the proxy (Phase 9), not Cursor logs. Adapter consumes proxy-emitted records.
- 7.5 — Test fixtures + parse tests for proxy-emitted records.

**PHASE 7 COMPLETE.**

---

# PHASE 8 — Aider Adapter

**Tasks:**

- 8.1 — `AiderAdapter::detect()` — checks for `.aider.conf.yml` or `.aider.tags.cache.v3` or `aider` in PATH.
- 8.2 — `AiderAdapter::install()` — writes managed section in `aider.conf.yml`; installs git `post-commit` hook calling `evolve record-aider HEAD`.
- 8.3 — `AiderAdapter::apply_config()` — updates managed section.
- 8.4 — `AiderAdapter::parse_session()` — for a given commit SHA: runs configured lint+test commands, checks exit codes, looks at whether the commit was reverted within 24h. Emits Signals.
- 8.5 — Tests with fixture git repo state.

**PHASE 8 COMPLETE.**

---

# PHASE 9 — `evolve-proxy`

**Goal:** OpenAI-compat HTTP server that injects active config's `system_prompt_prefix` into requests, forwards to upstream, and records suggestion accept/reject as signals.

**Tasks:**

- 9.1 — Create `crates/evolve-proxy/`. axum-based.
- 9.2 — `POST /v1/chat/completions` endpoint. Reads active config for the calling project (project identified by request header or by directory of running process — TBD per adapter).
- 9.3 — Inject `system_prompt_prefix` into the request's system message.
- 9.4 — Forward to upstream (configurable: real OpenAI, real Anthropic, real Ollama). Stream response back.
- 9.5 — Track signal candidates: which assistant message, which session, which variant. Persist via `SessionRepo` and `SignalRepo`.
- 9.6 — Auth: shared secret env var, OR localhost-only binding (default).
- 9.7 — `evolve proxy --for cursor` CLI subcommand (Phase 10) starts it.
- 9.8 — Cassette tests; integration test: real openai-python client → proxy → mock upstream → signal recorded.

**PHASE 9 COMPLETE.**

---

# PHASE 10 — `evolve-cli`

**Goal:** Single user-facing binary. Every command in the design doc 4.7.

**Tasks:**

- 10.1 — Create `crates/evolve-cli/`. clap-based.
- 10.2 — `evolve init <adapter>` — detect, install, write initial config.
- 10.3 — `evolve record-claude-code <transcript-path>` — parse, store, check promotion.
- 10.4 — `evolve record-aider <commit-sha>` — same.
- 10.5 — `evolve record-cursor-event <event-json>` — same (used by proxy).
- 10.6 — `evolve good` / `evolve bad` — explicit override on most recent session.
- 10.7 — `evolve thumbs <session-id> <±>` — explicit on a specific session.
- 10.8 — `evolve roll` — force challenger generation now.
- 10.9 — `evolve status` — one-line per-project summary.
- 10.10 — `evolve dashboard [--port N]` — start dashboard.
- 10.11 — `evolve proxy --for <adapter>` — start proxy.
- 10.12 — `evolve list` — list known projects.
- 10.13 — `evolve forget <project-id> | --all` — remove project + restore original configs.
- 10.14 — Integration test: `evolve init claude-code` in a tmpdir; assert hook installed; record fake session; assert signal stored.

**PHASE 10 COMPLETE.**

---

# PHASE 11 — `evolve-dashboard`

**Goal:** Local-only web UI that makes evolution visible.

**Tasks:**

- 11.1 — Create `crates/evolve-dashboard/`. axum + rust-embed.
- 11.2 — Decide SPA framework: Vite+React (recommended for contributor pool) OR leptos (Rust-only). Document decision.
- 11.3 — Web build under `crates/evolve-dashboard/web/`. Components: ProjectList, ProjectDetail, ExperimentDetail, SignalTimeline, PromotionLog, Settings.
- 11.4 — REST API endpoints: `GET /projects`, `GET /projects/:id`, `GET /projects/:id/experiments`, `GET /projects/:id/sessions?limit=N`, `GET /projects/:id/promotion-log`, `POST /signals/:id/override`, `POST /experiments/:id/abort`.
- 11.5 — SSE endpoint `GET /projects/:id/live` pushing new sessions/signals as they arrive.
- 11.6 — `npm run build` produces `web/dist/`; `rust-embed` bundles it.
- 11.7 — `evolve dashboard` opens browser to localhost.
- 11.8 — Manual UI test against real SQLite data.

**PHASE 11 COMPLETE.**

---

# PHASE 12 — Python Bindings (PyO3)

**Goal:** `pip install evolveai`; scriptable evolution control from Python.

**Tasks:**

- 12.1 — `bindings/python/` with maturin. Wrap `Storage`, `AdapterRegistry`, `Mutator`, `promotion_decision`.
- 12.2 — Async via `pyo3-async-runtimes`.
- 12.3 — Pytest suite. Parity test: 5 simulated sessions → expected promotion → identical to Rust result.
- 12.4 — Wheel build CI for Linux/macOS/Windows × CPython 3.10/3.11/3.12.

**PHASE 12 COMPLETE.**

---

# PHASE 13 — TypeScript Bindings (napi-rs)

**Tasks:**

- 13.1 — `bindings/typescript/` with napi-rs. Wrap same surface as Python.
- 13.2 — Async via napi-rs's tokio bridge.
- 13.3 — `node --test` suite. Parity test #2.
- 13.4 — Prebuilt binary CI.

**PHASE 13 COMPLETE.**

---

# PHASE 14 — Examples + Documentation

**Tasks:**

- 14.1 — mdBook scaffold under `docs/book/`. Chapters: Introduction, Quickstart (per adapter), How Evolution Works, Architecture, Cost & Privacy, FAQ, Contributing.
- 14.2 — `examples/claude-code-rust-project/` — a tiny Rust project with `evolve init claude-code` already configured; README walks the user through "use it for a week, watch it evolve."
- 14.3 — Same for Cursor (`examples/cursor-nextjs-project/`).
- 14.4 — Same for Aider (`examples/aider-python-project/`).
- 14.5 — GitHub Pages workflow deploys mdBook on every push to main.

**PHASE 14 COMPLETE.**

---

# PHASE 15 — Release Pipeline

**Tasks:**

- 15.1 — `cargo-dist` for prebuilt CLI binaries.
- 15.2 — `maturin-action` for PyPI publishing.
- 15.3 — `napi publish` for npm publishing.
- 15.4 — Multi-arch Docker image (`ghcr.io/<user>/evolve:<version>`).
- 15.5 — `CHANGELOG.md` (Keep a Changelog format).
- 15.6 — v1.0.0 release: tag, push, verify all artifacts published.

**PHASE 15 COMPLETE — v1.0 GA.**

---

## Cross-Cutting Concerns

### Privacy invariants (enforced by tests)

- No source code in `signals` table — tests assert `signals.payload` columns never contain code-like content
- No prompts logged unless `--include-prompts` opt-in flag set on `evolve init`
- No outbound HTTP except to user-configured LLM provider (mutation only)

### Idempotency (enforced by tests)

- Every `evolve init` is idempotent (re-run = no change)
- Every adapter's `install()` is idempotent
- Every adapter's `apply_config()` is idempotent in the sense that reapplying the same config = no change to the file

### Observability

- `tracing` everywhere with structured fields. JSON logs for service mode (proxy/dashboard), pretty for CLI.
- `EVOLVE_LOG_LEVEL` env var controls verbosity.
- No OTEL by default (privacy posture). Opt-in via env var if user wants to wire it up themselves.

---

## Acceptance Criteria for v1.0 GA

(From design doc Section 11.)

1. `cargo test --workspace` passes on Linux/macOS/Windows × stable
2. `pip install evolveai` then `evolve init claude-code` in a Rust project: hooks installed, baseline `CLAUDE.md` snippet written
3. Same for Cursor and Aider
4. After 100 simulated sessions where challenger always succeeds and champion always fails: challenger promoted
5. Same scenario but with random tied-50/50 outcomes: champion holds, no false promotion
6. `evolve dashboard` serves a working SPA on localhost; shows current project's champion + experiment + signals
7. `evolve good` / `evolve bad` records explicit signals that override conflicting implicit signals
8. `evolve forget <project-id>` removes all SQLite rows AND restores the user's pre-evolve config files
9. Coverage thresholds met (`evolve-core` ≥80%, `evolve-storage` ≥75%)
10. Real-world soak test on 3 of my own active projects (one per adapter), 2 weeks, document what evolved

---

*End of implementation plan.*
