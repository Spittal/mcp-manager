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

    info!("Memory server disabled");
    Ok(())
}
