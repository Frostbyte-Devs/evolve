//! evolve CLI — user-facing binary.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::{Parser, Subcommand};
use evolve_adapters::{
    AdapterRegistry, AiderAdapter, ClaudeCodeAdapter, CursorAdapter, SessionLog,
};
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId, SessionId, SignalId};
use evolve_storage::Storage;
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::sessions::{Session, SessionRepo, SessionVariant};
use evolve_storage::signals::{Signal, SignalKind, SignalRepo};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(
    name = "evolve",
    about = "Passive A/B evolution for AI coding assistants"
)]
struct Cli {
    /// Override the Evolve home directory (default `~/.evolve`).
    #[arg(long, env = "EVOLVE_HOME")]
    home: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize Evolve for a project by adapter id (claude-code, cursor, aider).
    Init {
        /// Adapter id.
        adapter: String,
        /// Project root; defaults to the current working directory.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Record a Claude Code session transcript.
    RecordClaudeCode {
        /// Path to the JSONL transcript file.
        transcript: PathBuf,
    },
    /// Record an Aider git commit session.
    RecordAider {
        /// Commit SHA.
        sha: String,
    },
    /// Record a Cursor proxy event (JSON).
    RecordCursorEvent {
        /// JSON event payload.
        event: String,
    },
    /// Mark the most recent session as a success.
    Good,
    /// Mark the most recent session as a failure.
    Bad,
    /// One-line status per project.
    Status,
    /// List known projects.
    List,
    /// Remove a project (delete configs + signals + restore files).
    Forget {
        /// Project id, or `--all`.
        project_id: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("EVOLVE_LOG_LEVEL")
                .unwrap_or_else(|_| "evolve=info".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let home = resolve_home(cli.home)?;
    tokio::fs::create_dir_all(&home).await?;
    let db_path = home.join("evolve.db");
    let storage = Storage::open(&db_path).await?;

    let registry = default_registry();

    match cli.command {
        Command::Init { adapter, root } => cmd_init(&storage, &registry, &adapter, root).await,
        Command::RecordClaudeCode { transcript } => {
            cmd_record(
                &storage,
                &registry,
                "claude-code",
                SessionLog::Transcript(transcript),
            )
            .await
        }
        Command::RecordAider { sha } => {
            cmd_record(&storage, &registry, "aider", SessionLog::GitCommit(sha)).await
        }
        Command::RecordCursorEvent { event } => {
            let json: serde_json::Value =
                serde_json::from_str(&event).context("event must be valid JSON")?;
            cmd_record(&storage, &registry, "cursor", SessionLog::ProxyEvent(json)).await
        }
        Command::Good => cmd_explicit(&storage, 1.0, "user_explicit_good").await,
        Command::Bad => cmd_explicit(&storage, 0.0, "user_explicit_bad").await,
        Command::Status => cmd_status(&storage).await,
        Command::List => cmd_list(&storage).await,
        Command::Forget { project_id, all } => {
            cmd_forget(&storage, &registry, project_id, all).await
        }
    }
}

fn resolve_home(flag: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = flag {
        return Ok(p);
    }
    let home = dirs::home_dir().context("no home directory; set EVOLVE_HOME or --home")?;
    Ok(home.join(".evolve"))
}

fn default_registry() -> AdapterRegistry {
    let mut r = AdapterRegistry::new();
    r.register(Arc::new(ClaudeCodeAdapter::new()));
    r.register(Arc::new(CursorAdapter::new()));
    r.register(Arc::new(AiderAdapter::new()));
    r
}

async fn cmd_init(
    storage: &Storage,
    registry: &AdapterRegistry,
    adapter_id: &str,
    root: Option<PathBuf>,
) -> Result<()> {
    let root = match root {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let root = root.canonicalize().unwrap_or(root);
    let adapter = registry
        .get(adapter_id)
        .with_context(|| format!("unknown adapter: {adapter_id}"))?;

    let config = AgentConfig::default_for(adapter_id);

    // Apply to disk (adapter writes managed section + installs hook).
    adapter.apply_config(&root, &config).await?;
    adapter.install(&root, &config).await?;

    // Register in storage.
    let project_repo = ProjectRepo::new(storage);
    let root_str = root.to_string_lossy().to_string();
    let existing = project_repo.get_by_root_path(&root_str).await?;
    let project_id = if let Some(p) = existing {
        p.id
    } else {
        let pid = ProjectId::new();
        project_repo
            .insert(&Project {
                id: pid,
                adapter_id: AdapterId::new(adapter_id),
                root_path: root_str.clone(),
                name: root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "project".into()),
                created_at: Utc::now(),
                champion_config_id: None,
            })
            .await?;
        pid
    };

    let cfg_id = ConfigId::new();
    let cfg_repo = AgentConfigRepo::new(storage);
    cfg_repo
        .insert(&AgentConfigRow {
            id: cfg_id,
            project_id,
            adapter_id: AdapterId::new(adapter_id),
            role: ConfigRole::Champion,
            fingerprint: config.fingerprint(),
            payload: config,
            created_at: Utc::now(),
        })
        .await?;
    project_repo.set_champion(project_id, cfg_id).await?;

    println!("Initialized project {} at {}", project_id, root.display());
    Ok(())
}

async fn cmd_record(
    storage: &Storage,
    registry: &AdapterRegistry,
    adapter_id: &str,
    log: SessionLog,
) -> Result<()> {
    let adapter = registry
        .get(adapter_id)
        .with_context(|| format!("unknown adapter: {adapter_id}"))?;

    // Find a project for this adapter id. v1: use the most recent match.
    let project_repo = ProjectRepo::new(storage);
    let projects = project_repo.list().await?;
    let project = projects
        .into_iter()
        .find(|p| p.adapter_id.as_str() == adapter_id)
        .context("no projects registered for this adapter; run `evolve init` first")?;
    let config_id = project
        .champion_config_id
        .context("project has no champion config")?;

    let parsed = adapter.parse_session(log).await?;

    // Insert Session.
    let session_id = SessionId::new();
    SessionRepo::new(storage)
        .insert(&Session {
            id: session_id,
            project_id: project.id,
            experiment_id: None,
            variant: SessionVariant::Champion,
            config_id,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            adapter_session_ref: None,
        })
        .await?;

    let signal_repo = SignalRepo::new(storage);
    for ps in parsed {
        signal_repo
            .insert(&Signal {
                id: SignalId::new(),
                session_id,
                kind: match ps.kind {
                    evolve_adapters::SignalKind::Explicit => SignalKind::Explicit,
                    evolve_adapters::SignalKind::Implicit => SignalKind::Implicit,
                },
                source: ps.source,
                value: ps.value,
                recorded_at: Utc::now(),
                payload_json: ps.payload_json,
            })
            .await?;
    }

    println!("Recorded session {session_id}");
    Ok(())
}

async fn cmd_explicit(storage: &Storage, value: f64, source: &str) -> Result<()> {
    // Find most recent session across all projects.
    let project_repo = ProjectRepo::new(storage);
    let projects = project_repo.list().await?;
    let session_repo = SessionRepo::new(storage);
    for project in projects {
        let sessions = session_repo.list_recent(project.id, 1).await?;
        if let Some(latest) = sessions.into_iter().next() {
            SignalRepo::new(storage)
                .insert(&Signal {
                    id: SignalId::new(),
                    session_id: latest.id,
                    kind: SignalKind::Explicit,
                    source: source.to_string(),
                    value,
                    recorded_at: Utc::now(),
                    payload_json: None,
                })
                .await?;
            println!("Marked session {} as {source} ({value})", latest.id);
            return Ok(());
        }
    }
    bail!("no sessions found; record one before using `evolve good`/`bad`");
}

async fn cmd_status(storage: &Storage) -> Result<()> {
    let projects = ProjectRepo::new(storage).list().await?;
    if projects.is_empty() {
        println!("No projects registered.");
        return Ok(());
    }
    let session_repo = SessionRepo::new(storage);
    for project in projects {
        let sessions = session_repo.list_recent(project.id, 1).await?;
        let latest = sessions
            .first()
            .map(|s| s.started_at.to_rfc3339())
            .unwrap_or_else(|| "none".into());
        println!(
            "{} [{}] {} (last session: {})",
            project.id, project.adapter_id, project.name, latest,
        );
    }
    Ok(())
}

async fn cmd_list(storage: &Storage) -> Result<()> {
    for project in ProjectRepo::new(storage).list().await? {
        println!(
            "{}  {}  {}  {}",
            project.id, project.adapter_id, project.root_path, project.name,
        );
    }
    Ok(())
}

async fn cmd_forget(
    storage: &Storage,
    registry: &AdapterRegistry,
    project_id: Option<String>,
    all: bool,
) -> Result<()> {
    let project_repo = ProjectRepo::new(storage);
    let targets: Vec<Project> = if all {
        project_repo.list().await?
    } else {
        let pid_str = project_id.context("either --all or <project_id> is required")?;
        let uuid = uuid::Uuid::parse_str(&pid_str).context("invalid project id")?;
        match project_repo.get_by_id(ProjectId::from_uuid(uuid)).await? {
            Some(p) => vec![p],
            None => bail!("no such project"),
        }
    };

    for project in targets {
        if let Some(adapter) = registry.get(project.adapter_id.as_str()) {
            adapter
                .forget(std::path::Path::new(&project.root_path))
                .await
                .ok(); // best-effort — root may be gone
        }
        project_repo.delete(project.id).await?;
        println!("Forgot project {}", project.id);
    }
    Ok(())
}
