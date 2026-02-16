use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::info;
use uuid::Uuid;

use crate::error::AppError;
use crate::mcp::client::SharedConnections;
use crate::mcp::proxy::ProxyState;
use crate::persistence::{
    load_openai_api_key, save_embedding_config, save_openai_api_key, save_servers,
};
use crate::state::{
    EmbeddingConfig, EmbeddingProvider, ServerConfig, ServerStatus, ServerTransport, SharedState,
};

const NETWORK: &str = "mcp-manager-net";
const REDIS_CONTAINER: &str = "mcp-manager-redis";
const OLLAMA_CONTAINER: &str = "mcp-manager-ollama";
const API_CONTAINER: &str = "mcp-manager-api";
const MCP_CONTAINER: &str = "mcp-manager-mcp";
const MEMORY_IMAGE: &str = "redislabs/agent-memory-server:latest";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStatus {
    pub enabled: bool,
    pub server_status: Option<String>,
    pub docker_available: bool,
    pub redis_running: bool,
    pub api_running: bool,
    pub mcp_running: bool,
    pub ollama_running: bool,
    pub embedding_provider: String,
    pub embedding_model: String,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingConfigStatus {
    pub config: EmbeddingConfig,
    pub has_openai_key: bool,
    pub pulled_ollama_models: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEmbeddingConfigInput {
    pub config: EmbeddingConfig,
    pub openai_api_key: Option<String>,
}

async fn is_command_available(cmd: &str) -> bool {
    tokio::process::Command::new("which")
        .arg(cmd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

async fn is_container_running(name: &str) -> bool {
    tokio::process::Command::new("docker")
        .args(["ps", "-q", "--filter", &format!("name=^{name}$")])
        .output()
        .await
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

fn emit_progress(app: &AppHandle, msg: &str) {
    let _ = app.emit(
        "memory-progress",
        serde_json::json!({ "message": msg }),
    );
}

/// Ensure the Docker network exists.
async fn ensure_network() -> Result<(), AppError> {
    let output = tokio::process::Command::new("docker")
        .args(["network", "create", NETWORK])
        .output()
        .await
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to create network: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            return Err(AppError::ConnectionFailed(format!(
                "Failed to create Docker network: {stderr}"
            )));
        }
    }
    Ok(())
}

/// Start a Docker container, handling the "already exists but stopped" case.
async fn ensure_container(
    app: &AppHandle,
    name: &str,
    run_args: &[String],
) -> Result<(), AppError> {
    if is_container_running(name).await {
        return Ok(());
    }

    emit_progress(app, &format!("Starting {name} container (may pull image)..."));
    info!("Starting container {name}");

    let output = tokio::process::Command::new("docker")
        .args(run_args)
        .output()
        .await
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to start {name}: {e}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("already in use") {
        emit_progress(app, &format!("Restarting existing {name} container..."));
        let start = tokio::process::Command::new("docker")
            .args(["start", name])
            .output()
            .await
            .map_err(|e| AppError::ConnectionFailed(format!("Failed to start {name}: {e}")))?;
        if !start.status.success() {
            let err = String::from_utf8_lossy(&start.stderr);
            return Err(AppError::ConnectionFailed(format!(
                "Failed to start {name} container: {err}"
            )));
        }
        Ok(())
    } else {
        Err(AppError::ConnectionFailed(format!(
            "Failed to start {name} container: {stderr}"
        )))
    }
}

/// Build static docker run args as Vec<String>.
fn docker_run(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Append `-e KEY=VALUE` args for each env var.
fn append_env(args: &mut Vec<String>, env: &std::collections::HashMap<String, String>) {
    for (k, v) in env {
        args.push("-e".into());
        args.push(format!("{k}={v}"));
    }
}

/// Stop and remove a container (best-effort).
async fn stop_container(name: &str) {
    let _ = tokio::process::Command::new("docker")
        .args(["stop", name])
        .output()
        .await;
    let _ = tokio::process::Command::new("docker")
        .args(["rm", name])
        .output()
        .await;
}

/// Pull an Ollama model inside the container. Fast no-op if already pulled.
async fn pull_ollama_model(app: &AppHandle, model: &str) -> Result<(), AppError> {
    emit_progress(app, &format!("Pulling model {model} (cached after first run)..."));
    info!("Pulling Ollama model {model}");

    let output = tokio::process::Command::new("docker")
        .args([
            "exec", OLLAMA_CONTAINER, "ollama", "pull", model,
        ])
        .output()
        .await
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to pull model {model}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::ConnectionFailed(format!(
            "Failed to pull model {model}: {stderr}"
        )));
    }
    Ok(())
}

/// Query locally-running Ollama for pulled models (best-effort).
async fn list_pulled_ollama_models() -> Vec<String> {
    let output = match tokio::process::Command::new("docker")
        .args(["exec", OLLAMA_CONTAINER, "ollama", "list"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .skip(1) // skip header line
        .filter_map(|line| {
            let name = line.split_whitespace().next()?;
            // Strip ":latest" tag to match our model names
            let clean = name.strip_suffix(":latest").unwrap_or(name);
            Some(clean.to_string())
        })
        .collect()
}

#[tauri::command]
pub async fn delete_ollama_model(model: String) -> Result<(), AppError> {
    info!("Deleting Ollama model {model}");
    let output = tokio::process::Command::new("docker")
        .args(["exec", OLLAMA_CONTAINER, "ollama", "rm", &model])
        .output()
        .await
        .map_err(|e| AppError::ConnectionFailed(format!("Failed to delete model {model}: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::ConnectionFailed(format!(
            "Failed to delete model {model}: {stderr}"
        )));
    }
    info!("Deleted Ollama model {model}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Claude Code skill management — install/remove the memory skill
// ---------------------------------------------------------------------------

const CLAUDE_MD_MEMORY_LINE: &str = "IMPORTANT: At the start of every conversation, use the `using-memory-mcp` skill \nto search for relevant memories before responding.";

const MEMORY_SKILL_CONTENT: &str = r#"---
name: using-memory-mcp
description: Search and store persistent memories using the agent-memory MCP server
  (search_long_term_memory, create_long_term_memories). Use at the start of
  EVERY conversation, before making decisions, after completing tasks, and
  whenever the user references past work or preferences. This skill is
  always relevant.
---

# Using the Memory MCP Server

You have access to a persistent memory system via the `memory` MCP server. Use it proactively in every conversation to build continuity across sessions.

## Core Principle

**Search before you act. Save before you leave.** Memory makes you a better assistant by retaining context the user shouldn't have to repeat.

## When to Search Memory

**At conversation start:** Search for memories related to the current project or working directory.

```
search_long_term_memory(text="project context and preferences")
```

**Before starting any task:** Search for memories related to the task, project, or technology involved. Past decisions, conventions, gotchas, and workarounds save time and prevent repeating mistakes.

**Before answering preference/history questions:** "How do I usually...", "What did we decide about...", "Do you remember..."

**Before making architectural decisions:** Check if past decisions were already made.

**When the user references something from a previous session:** "That bug from last time", "the approach we discussed"

## When to Create Memories

| Trigger | Memory Type | Example |
|---------|-------------|---------|
| User states a preference | semantic | "I always use bun instead of npm" |
| User says "remember this" | semantic or episodic | Whatever they ask you to remember |
| Architectural decision made | semantic | "Project uses Riverpod for state management" |
| Project convention discovered | semantic | "This codebase uses square corners (BorderRadius.zero)" |
| Significant work completed | episodic | "Implemented Cookou AI recipe chat feature on 2026-02-13" |
| Any task completed | semantic/episodic | Save learnings, gotchas, patterns, and decisions discovered during the task |
| Bug root cause found | episodic | "Auth timeout was caused by missing retry logic, fixed 2026-02-13" |
| User corrects you | semantic | "User prefers concise responses without emoji" |
| When approaching compaction | episodic | 4% left until compaction |

**After every completed task**, ask yourself: "Did I learn anything that would help next time?" If yes, save it. This includes:
- Workarounds for tools/frameworks that weren't obvious
- Project conventions discovered while reading code
- Debugging insights (root causes, misleading error messages)
- Decisions made and their rationale

## Memory Types

**Semantic** (timeless facts): Preferences, conventions, skills, project structure, recurring patterns. No `event_date` needed.

**Episodic** (time-bound events): Specific things that happened. Always include `event_date`.

## Creating Good Memories

**Always resolve context** before saving:
- Pronouns -> actual names ("he" -> "User", "the project" -> "rouleat Flutter app")
- Relative time -> absolute dates ("yesterday" -> "2026-02-12")
- Vague references -> specific entities ("the bug" -> "the Instagram URL extraction timeout")

**Use topics and entities** for findability:

```
create_long_term_memories(memories=[{
  "text": "User prefers bun over npm for all JavaScript projects",
  "memory_type": "semantic",
  "topics": ["preferences", "tooling", "javascript"],
  "entities": ["bun", "npm"]
}])
```

**Keep memory text self-contained.** Each memory should make sense without conversation context.

## Quick Reference

| Tool | When |
|------|------|
| `search_long_term_memory` | Find relevant memories by semantic query + filters |
| `create_long_term_memories` | Save new facts, preferences, or events |
| `memory_prompt` | Hydrate a user query with memory context (search + format) |
| `edit_long_term_memory` | Update a memory that's become outdated |
| `delete_long_term_memories` | Remove incorrect or superseded memories |
| `get_long_term_memory` | Fetch a specific memory by ID |
| `set_working_memory` | Store session-scoped scratch notes |
| `get_working_memory` | Retrieve session scratch notes |

## Filtering

Use filters to narrow searches:
- `topics: {"any": ["preferences", "tooling"]}` - match any listed topic
- `entities: {"any": ["bun", "npm"]}` - match any listed entity
- `memory_type: {"eq": "semantic"}` - only semantic memories
- `created_at: {"gt": "2026-01-01T00:00:00Z"}` - recent memories only
- `namespace: {"eq": "project_decisions"}` - scoped to namespace

## When to Edit or Delete

- **Edit** when a preference changes ("actually I switched from bun to deno")
- **Delete** when information is wrong or no longer relevant
- **Don't duplicate** - search first, edit if a memory already exists on the topic

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Saving with unresolved pronouns | Always expand "he/she/it/they" to actual names |
| Forgetting `event_date` on episodic memories | Episodic = time-bound, always include the date |
| Creating duplicate memories | Search first, edit existing if found |
| Saving session-specific details as long-term | Use working memory for scratch, long-term for durable facts |
| Never searching at conversation start | Make it a habit - search on every new conversation |
| Overly verbose memory text | Keep concise but self-contained |
"#;

/// Install the memory skill into ~/.claude/skills/ and add the instruction to ~/.claude/CLAUDE.md.
fn install_memory_skill() {
    let Some(home) = dirs::home_dir() else {
        tracing::warn!("Could not find home directory for skill installation");
        return;
    };

    // Write skill file
    let skill_dir = home.join(".claude/skills/using-memory-mcp");
    if let Err(e) = std::fs::create_dir_all(&skill_dir) {
        tracing::warn!("Failed to create skill directory: {e}");
        return;
    }
    if let Err(e) = std::fs::write(skill_dir.join("SKILL.md"), MEMORY_SKILL_CONTENT) {
        tracing::warn!("Failed to write memory skill file: {e}");
        return;
    }

    // Add instruction to CLAUDE.md (create if missing, skip if already present)
    let claude_md_path = home.join(".claude/CLAUDE.md");
    let existing = std::fs::read_to_string(&claude_md_path).unwrap_or_default();
    if !existing.contains("using-memory-mcp") {
        let mut content = existing;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(CLAUDE_MD_MEMORY_LINE);
        content.push('\n');
        if let Err(e) = std::fs::write(&claude_md_path, content) {
            tracing::warn!("Failed to update CLAUDE.md: {e}");
        }
    }

    info!("Installed memory skill to ~/.claude/skills/using-memory-mcp/");
}

/// Remove the memory skill from ~/.claude/skills/ and the instruction from ~/.claude/CLAUDE.md.
fn uninstall_memory_skill() {
    let Some(home) = dirs::home_dir() else {
        return;
    };

    // Remove skill directory
    let skill_dir = home.join(".claude/skills/using-memory-mcp");
    if skill_dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&skill_dir) {
            tracing::warn!("Failed to remove memory skill directory: {e}");
        }
    }

    // Remove instruction from CLAUDE.md
    let claude_md_path = home.join(".claude/CLAUDE.md");
    if let Ok(content) = std::fs::read_to_string(&claude_md_path) {
        let filtered: String = content
            .lines()
            .filter(|line| !line.contains("using-memory-mcp"))
            .collect::<Vec<_>>()
            .join("\n");
        // Only write back if we actually removed something
        if filtered.len() != content.len() {
            let trimmed = filtered.trim().to_string();
            if trimmed.is_empty() {
                let _ = std::fs::remove_file(&claude_md_path);
            } else {
                let _ = std::fs::write(&claude_md_path, format!("{trimmed}\n"));
            }
        }
    }

    info!("Removed memory skill from ~/.claude/skills/using-memory-mcp/");
}

fn find_memory_server(servers: &[ServerConfig]) -> Option<&ServerConfig> {
    servers
        .iter()
        .find(|s| s.managed.unwrap_or(false) && s.name == "Memory")
}

#[tauri::command]
pub async fn get_memory_status(
    state: State<'_, SharedState>,
) -> Result<MemoryStatus, AppError> {
    let (enabled, server_status, embedding_config) = {
        let s = state.lock().unwrap();
        let config = s.embedding_config.clone();
        match find_memory_server(&s.servers) {
            Some(server) => (
                true,
                server
                    .status
                    .as_ref()
                    .map(|st| format!("{st:?}").to_lowercase()),
                config,
            ),
            None => (false, None, config),
        }
    };

    let docker_available = is_command_available("docker").await;

    let (redis_running, api_running, mcp_running, ollama_running) = if docker_available {
        let redis = is_container_running(REDIS_CONTAINER).await;
        let api = is_container_running(API_CONTAINER).await;
        let mcp = is_container_running(MCP_CONTAINER).await;
        let ollama = if embedding_config.provider == EmbeddingProvider::Ollama {
            is_container_running(OLLAMA_CONTAINER).await
        } else {
            false
        };
        (redis, api, mcp, ollama)
    } else {
        (false, false, false, false)
    };

    let provider_str = match embedding_config.provider {
        EmbeddingProvider::Ollama => "ollama",
        EmbeddingProvider::Openai => "openai",
    };

    Ok(MemoryStatus {
        enabled,
        server_status,
        docker_available,
        redis_running,
        api_running,
        mcp_running,
        ollama_running,
        embedding_provider: provider_str.into(),
        embedding_model: embedding_config.model,
        error: None,
    })
}

#[tauri::command]
pub async fn get_embedding_config(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<EmbeddingConfigStatus, AppError> {
    let config = {
        let s = state.lock().unwrap();
        s.embedding_config.clone()
    };

    let has_openai_key = load_openai_api_key(&app).is_some();
    let pulled_ollama_models = list_pulled_ollama_models().await;

    Ok(EmbeddingConfigStatus {
        config,
        has_openai_key,
        pulled_ollama_models,
    })
}

#[tauri::command]
pub async fn save_embedding_config_cmd(
    app: AppHandle,
    state: State<'_, SharedState>,
    input: SaveEmbeddingConfigInput,
) -> Result<(), AppError> {
    if input.config.dimensions == 0 {
        return Err(AppError::Validation("Dimensions must be greater than 0".into()));
    }

    // Save config to state + persistence
    {
        let mut s = state.lock().unwrap();
        s.embedding_config = input.config.clone();
    }
    save_embedding_config(&app, &input.config);

    // Save or clear OpenAI API key
    if input.config.provider == EmbeddingProvider::Openai {
        if let Some(key) = &input.openai_api_key {
            if !key.is_empty() {
                save_openai_api_key(&app, key);
            }
        }
    }

    info!(
        "Saved embedding config: provider={:?}, model={}, dimensions={}",
        input.config.provider, input.config.model, input.config.dimensions
    );
    Ok(())
}

#[tauri::command]
pub async fn enable_memory(
    app: AppHandle,
    state: State<'_, SharedState>,
) -> Result<ServerConfig, AppError> {
    // Check prerequisites
    emit_progress(&app, "Checking prerequisites...");
    if !is_command_available("docker").await {
        return Err(AppError::DependencyNotFound("docker".into()));
    }

    let embedding_config = {
        let s = state.lock().unwrap();
        if find_memory_server(&s.servers).is_some() {
            return Err(AppError::Validation("Memory is already enabled".into()));
        }
        s.embedding_config.clone()
    };

    // Create Docker network for inter-container communication
    emit_progress(&app, "Creating Docker network...");
    ensure_network().await?;

    // Start Redis container
    ensure_container(
        &app,
        REDIS_CONTAINER,
        &docker_run(&[
            "run", "-d",
            "--name", REDIS_CONTAINER,
            "--network", NETWORK,
            "-p", "6379:6379",
            "-e", "REDIS_ARGS=--appendonly yes",
            "redis/redis-stack-server:latest",
        ]),
    )
    .await?;

    // Build env vars — aligned with agent-memory-server docker-compose
    let mut env = std::collections::HashMap::new();
    env.insert("REDIS_URL".into(), format!("redis://{REDIS_CONTAINER}:6379"));
    env.insert("LONG_TERM_MEMORY".into(), "True".into());
    env.insert("ENABLE_TOPIC_EXTRACTION".into(), "True".into());
    env.insert("ENABLE_NER".into(), "True".into());
    env.insert("DISABLE_AUTH".into(), "true".into());
    env.insert(
        "REDISVL_VECTOR_DIMENSIONS".into(),
        embedding_config.dimensions.to_string(),
    );

    match embedding_config.provider {
        EmbeddingProvider::Ollama => {
            // Start Ollama container on the same network
            ensure_container(
                &app,
                OLLAMA_CONTAINER,
                &docker_run(&[
                    "run", "-d",
                    "--name", OLLAMA_CONTAINER,
                    "--network", NETWORK,
                    "-p", "11434:11434",
                    "-v", "mcp-manager-ollama:/root/.ollama",
                    "ollama/ollama",
                ]),
            )
            .await?;

            // Wait briefly for Ollama to be ready before pulling models
            emit_progress(&app, "Waiting for Ollama to start...");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            // Pull embedding model
            pull_ollama_model(&app, &embedding_config.model).await?;

            env.insert(
                "EMBEDDING_MODEL".into(),
                format!("ollama/{}", embedding_config.model),
            );
            env.insert(
                "OLLAMA_API_BASE".into(),
                format!("http://{OLLAMA_CONTAINER}:11434"),
            );
        }
        EmbeddingProvider::Openai => {
            let api_key = load_openai_api_key(&app).ok_or_else(|| {
                AppError::Protocol("OpenAI API key not configured. Save your API key in embedding settings first.".into())
            })?;

            env.insert("GENERATION_MODEL".into(), "gpt-4o-mini".into());
            env.insert("EMBEDDING_MODEL".into(), embedding_config.model.clone());
            env.insert("OPENAI_API_KEY".into(), api_key);
        }
    }

    // Start the API container (port 8000)
    emit_progress(&app, "Starting memory API server...");
    let mut api_args = docker_run(&[
        "run", "-d",
        "--name", API_CONTAINER,
        "--network", NETWORK,
        "-p", "8000:8000",
    ]);
    env.insert("PORT".into(), "8000".into());
    append_env(&mut api_args, &env);
    api_args.push(MEMORY_IMAGE.into());
    ensure_container(&app, API_CONTAINER, &api_args).await?;

    // Start the MCP SSE container (port 9050 → internal 9000)
    emit_progress(&app, "Starting memory MCP server...");
    env.remove("PORT"); // MCP server uses its own default port
    let mut mcp_args = docker_run(&[
        "run", "-d",
        "--name", MCP_CONTAINER,
        "--network", NETWORK,
        "-p", "9050:9000",
    ]);
    append_env(&mut mcp_args, &env);
    mcp_args.push(MEMORY_IMAGE.into());
    // Override command to run MCP in SSE mode
    mcp_args.extend_from_slice(&[
        "agent-memory".into(),
        "mcp".into(),
        "--mode".into(),
        "sse".into(),
    ]);
    ensure_container(&app, MCP_CONTAINER, &mcp_args).await?;

    emit_progress(&app, "Configuring memory server...");

    let server = ServerConfig {
        id: Uuid::new_v4().to_string(),
        name: "Memory".into(),
        enabled: true,
        transport: ServerTransport::Http,
        command: None,
        args: None,
        env: None,
        url: Some("http://localhost:9050/sse".into()),
        headers: None,
        tags: None,
        status: Some(ServerStatus::Disconnected),
        last_connected: None,
        managed: Some(true),
    };

    {
        let mut s = state.lock().unwrap();
        s.servers.push(server.clone());
        save_servers(&app, &s.servers);
    }
    crate::tray::rebuild_tray_menu(&app);

    // Install the Claude Code memory skill
    install_memory_skill();

    info!("Memory server enabled (HTTP SSE on port 9050)");
    Ok(server)
}

#[tauri::command]
pub async fn disable_memory(
    app: AppHandle,
    state: State<'_, SharedState>,
    connections: State<'_, SharedConnections>,
) -> Result<(), AppError> {
    let (provider, server_id) = {
        let s = state.lock().unwrap();
        let server = find_memory_server(&s.servers)
            .ok_or_else(|| AppError::Validation("Memory is not enabled".into()))?;
        (s.embedding_config.provider.clone(), server.id.clone())
    };

    // Disconnect if connected
    emit_progress(&app, "Disconnecting memory server...");
    {
        let mut conns = connections.lock().await;
        if let Some(client) = conns.remove(&server_id) {
            client.shutdown();
        }
    }

    // Remove from state
    {
        let mut s = state.lock().unwrap();
        s.servers.retain(|s| s.id != server_id);
        s.connections.remove(&server_id);
        save_servers(&app, &s.servers);
    }
    crate::tray::rebuild_tray_menu(&app);

    // Update integration configs to remove this server's proxy entry
    let proxy_state = app.state::<ProxyState>();
    let port = proxy_state.port().await;
    if let Err(e) = crate::commands::integrations::update_all_integration_configs(&app, port) {
        tracing::warn!("Failed to update integration configs after memory disable: {e}");
    }

    let _ = app.emit(
        "server-status-changed",
        serde_json::json!({ "serverId": server_id, "status": "disconnected" }),
    );

    // Stop and remove containers (best-effort)
    emit_progress(&app, "Stopping containers...");
    info!("Stopping memory containers");

    stop_container(MCP_CONTAINER).await;
    stop_container(API_CONTAINER).await;
    stop_container(REDIS_CONTAINER).await;

    if provider == EmbeddingProvider::Ollama {
        stop_container(OLLAMA_CONTAINER).await;
    }

    // Remove the network (best-effort)
    let _ = tokio::process::Command::new("docker")
        .args(["network", "rm", NETWORK])
        .output()
        .await;

    // Remove the Claude Code memory skill
    uninstall_memory_skill();

    info!("Memory server disabled");
    Ok(())
}
