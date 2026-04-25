//! evolve CLI — user-facing binary.

use evolve_cli::engine;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::{Parser, Subcommand};
use evolve_adapters::{
    AdapterRegistry, AiderAdapter, ClaudeCodeAdapter, CursorAdapter, SessionLog,
};
use evolve_core::agent_config::AgentConfig;
use evolve_core::ids::{AdapterId, ConfigId, ProjectId, SessionId, SignalId};
use evolve_core::promotion::Decision;
use evolve_storage::Storage;
use evolve_storage::agent_configs::{AgentConfigRepo, AgentConfigRow, ConfigRole};
use evolve_storage::projects::{Project, ProjectRepo};
use evolve_storage::sessions::{Session, SessionRepo};
use evolve_storage::signals::{Signal, SignalKind, SignalRepo};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
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
    /// Force-generate a new challenger now (skip the schedule).
    Roll,
    /// Diagnose what's stopping evolution from happening.
    Doctor,
    /// Start the local dashboard (http://127.0.0.1:<port>).
    Dashboard {
        /// TCP port. Default 8787.
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Start the OpenAI-compat proxy (http://127.0.0.1:<port>). Requires
    /// EVOLVE_UPSTREAM_URL and EVOLVE_UPSTREAM_TOKEN in the environment.
    Proxy {
        /// Adapter id this proxy serves (informational, for logging).
        #[arg(long, default_value = "cursor")]
        adapter: String,
        /// TCP port. Default 7777.
        #[arg(long, default_value_t = 7777)]
        port: u16,
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
            let project_root = std::env::current_dir().ok();
            cmd_record(
                &storage,
                &registry,
                "aider",
                SessionLog::GitCommit { sha, project_root },
            )
            .await
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
        Command::Roll => cmd_roll(&storage, &registry).await,
        Command::Doctor => cmd_doctor(&storage, &registry, home).await,
        Command::Dashboard { port } => cmd_dashboard(storage, port).await,
        Command::Proxy { adapter, port } => cmd_proxy(&storage, &adapter, port).await,
    }
}

async fn cmd_dashboard(storage: Storage, port: u16) -> Result<()> {
    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let state = evolve_dashboard::AppState {
        storage: std::sync::Arc::new(storage),
    };
    println!("Dashboard listening on http://{addr}");
    evolve_dashboard::serve(addr, state).await?;
    Ok(())
}

async fn cmd_proxy(storage: &Storage, adapter: &str, port: u16) -> Result<()> {
    // Pick the active champion's system_prompt_prefix for the first project
    // matching this adapter. Bare-minimum wiring — a future version can let
    // the proxy swap prefixes live as new champions are promoted.
    let project_repo = ProjectRepo::new(storage);
    let projects = project_repo.list().await?;
    let project = projects
        .into_iter()
        .find(|p| p.adapter_id.as_str() == adapter)
        .context("no projects registered for this adapter; run `evolve init` first")?;
    let cfg_id = project
        .champion_config_id
        .context("project has no champion config")?;
    let cfg_row = AgentConfigRepo::new(storage)
        .get_by_id(cfg_id)
        .await?
        .context("champion config row missing")?;

    let upstream = std::env::var("EVOLVE_UPSTREAM_URL")
        .context("EVOLVE_UPSTREAM_URL must be set (e.g. https://api.openai.com)")?;
    let upstream_token = std::env::var("EVOLVE_UPSTREAM_TOKEN").ok();

    let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse()?;
    let state = evolve_proxy::AppState {
        config: evolve_proxy::ProxyConfig {
            upstream,
            upstream_token,
            prefix: cfg_row.payload.system_prompt_prefix,
        },
        signals: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
        http: reqwest::Client::new(),
    };
    println!("Proxy listening on http://{addr}  (adapter={adapter})");
    evolve_proxy::serve(addr, state).await?;
    Ok(())
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

    // Resolve which variant + config + experiment this session belongs to.
    let (variant, config_id, experiment_id) =
        engine::resolve_active_deployment(storage, &project).await?;

    let parsed = adapter.parse_session(log).await?;

    // Insert Session tagged with the active deployment.
    let session_id = SessionId::new();
    SessionRepo::new(storage)
        .insert(&Session {
            id: session_id,
            project_id: project.id,
            experiment_id,
            variant,
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

    // ---- the actual evolution loop ----

    // 1) If an experiment is running, evaluate the promotion posterior.
    if let Some((exp, decision)) = engine::evaluate_promotion(storage, project.id).await? {
        match decision {
            Decision::Promote { posterior } => {
                engine::promote_challenger(storage, registry, &project, &exp, posterior).await?;
                println!(
                    "Promoted challenger {} (posterior {:.3})",
                    exp.challenger_config_id, posterior
                );
            }
            Decision::Hold { posterior } => {
                tracing::debug!(target: "evolve::engine", "experiment holding at posterior {posterior:.3}");
            }
            Decision::NeedMoreData {
                sessions_each,
                required,
            } => {
                tracing::debug!(
                    target: "evolve::engine",
                    "experiment needs more data ({sessions_each}/{required} per arm)"
                );
            }
        }
    } else if engine::should_evolve(storage, project.id, 20).await? {
        // 2) No running experiment + threshold reached → spin up a challenger.
        // Use whichever picker matches LLM availability. With no LLM the
        // picker uses only the four rule-based mutators.
        let mut rng = ChaCha8Rng::from_entropy();
        let llm_box = evolve_llm::pick_default_client().await.ok();
        let has_llm = llm_box.is_some();
        let picker = engine::picker_for_environment(has_llm);
        let noop = evolve_llm::NoOpLlmClient;
        let llm: &dyn evolve_llm::LlmClient = match llm_box.as_ref() {
            Some(b) => b.as_ref(),
            None => &noop,
        };
        match engine::generate_challenger_with_picker(
            storage, registry, llm, &picker, &project, &mut rng,
        )
        .await
        {
            Ok((cid, eid)) => {
                println!("Generated challenger {cid} in experiment {eid}");
                if !has_llm {
                    println!(
                        "(No LLM available — used rule-based mutator only. Set ANTHROPIC_API_KEY or run Ollama locally to enable LlmRewrite mutator.)"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(target: "evolve::engine", "challenger generation failed: {e}");
            }
        }
    }

    Ok(())
}

/// Diagnose every step required for evolution to actually happen on this machine.
async fn cmd_doctor(storage: &Storage, registry: &AdapterRegistry, home: PathBuf) -> Result<()> {
    fn ok(label: &str, detail: &str) {
        println!("[OK]   {label:<32} {detail}");
    }
    fn warn(label: &str, detail: &str) {
        println!("[WARN] {label:<32} {detail}");
    }
    fn missing(label: &str, detail: &str) {
        println!("[MISS] {label:<32} {detail}");
    }
    fn info(label: &str, detail: &str) {
        println!("[INFO] {label:<32} {detail}");
    }

    println!("Evolve doctor");
    println!("--------------------------------------------------------");

    ok("evolve home", &home.display().to_string());

    let projects = ProjectRepo::new(storage).list().await?;
    if projects.is_empty() {
        missing(
            "projects registered",
            "run `evolve init <adapter>` in a project directory",
        );
        println!("--------------------------------------------------------");
        return Ok(());
    }
    ok("projects registered", &format!("{}", projects.len()));

    let cwd = std::env::current_dir().ok();
    let canonical_cwd = cwd
        .as_deref()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().to_string());

    let current_project = canonical_cwd.as_deref().and_then(|c| {
        projects
            .iter()
            .find(|p| p.root_path == c || p.root_path.to_lowercase() == c.to_lowercase())
    });

    let project = match current_project {
        Some(p) => p.clone(),
        None => {
            // Fall back to first project so we can still report.
            warn(
                "current dir match",
                "this dir is not a registered project — using most recent",
            );
            projects[0].clone()
        }
    };
    ok(
        "active project",
        &format!("{} ({})", project.name, project.adapter_id),
    );

    // Adapter-specific checks.
    let root = std::path::Path::new(&project.root_path);
    if let Some(adapter) = registry.get(project.adapter_id.as_str()) {
        match adapter.detect(root) {
            evolve_adapters::AdapterDetection::Detected => {
                ok("adapter config files", "present");
            }
            evolve_adapters::AdapterDetection::NotDetected => {
                warn(
                    "adapter config files",
                    "missing — run `evolve init` again to recreate",
                );
            }
        }
    }

    // Hook installed?
    let cc_settings = root.join(".claude").join("settings.json");
    if project.adapter_id.as_str() == "claude-code" {
        if cc_settings.is_file() {
            let raw = tokio::fs::read_to_string(&cc_settings)
                .await
                .unwrap_or_default();
            if raw.contains("evolve record-claude-code") {
                ok("Stop hook installed", ".claude/settings.json");
            } else {
                missing(
                    "Stop hook installed",
                    "Stop hook not found in settings.json",
                );
            }
        } else {
            missing("Stop hook installed", ".claude/settings.json missing");
        }
    }

    // Champion config + sessions.
    if let Some(champ_id) = project.champion_config_id {
        ok("champion config", &champ_id.to_string());
    } else {
        missing("champion config", "project has no champion (re-init?)");
    }

    let session_count = SessionRepo::new(storage)
        .list_recent(project.id, 9999)
        .await?
        .len();
    let threshold = 20;
    if session_count >= threshold {
        ok(
            "sessions recorded",
            &format!("{session_count} (>= {threshold} threshold)"),
        );
    } else {
        info(
            "sessions recorded",
            &format!(
                "{session_count} ({} more before challenger generation)",
                threshold - session_count
            ),
        );
    }

    // Experiment status.
    if let Some(exp) = evolve_storage::experiments::ExperimentRepo::new(storage)
        .get_running_for_project(project.id)
        .await?
    {
        ok(
            "experiment running",
            &format!("started {}", exp.started_at.to_rfc3339()),
        );
    } else {
        info("experiment running", "none yet");
    }

    // LLM availability.
    match evolve_llm::pick_default_client().await {
        Ok(llm) => ok("LLM available", llm.model_id()),
        Err(_) => warn(
            "LLM available",
            "no Anthropic key + no Ollama on :11434 — only rule-based mutators will run",
        ),
    }

    println!("--------------------------------------------------------");
    Ok(())
}

async fn cmd_roll(storage: &Storage, registry: &AdapterRegistry) -> Result<()> {
    let projects = ProjectRepo::new(storage).list().await?;
    let project = projects
        .into_iter()
        .next()
        .context("no projects registered; run `evolve init` first")?;
    let mut rng = ChaCha8Rng::from_entropy();
    let llm_box = evolve_llm::pick_default_client().await.ok();
    let has_llm = llm_box.is_some();
    let picker = engine::picker_for_environment(has_llm);
    let noop = evolve_llm::NoOpLlmClient;
    let llm: &dyn evolve_llm::LlmClient = match llm_box.as_ref() {
        Some(b) => b.as_ref(),
        None => &noop,
    };
    let (cid, eid) = engine::generate_challenger_with_picker(
        storage, registry, llm, &picker, &project, &mut rng,
    )
    .await?;
    println!("Generated challenger {cid} in experiment {eid}");
    if !has_llm {
        println!(
            "(No LLM detected — used rule-based mutator. Set ANTHROPIC_API_KEY or run Ollama locally for LlmRewrite mutations.)"
        );
    }
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
