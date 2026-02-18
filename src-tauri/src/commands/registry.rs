use std::collections::HashMap;

use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::state::registry::{
    MarketplaceCache, MarketplaceServerDetail, RegistrySearchResult, RuntimeDeps,
};
use crate::state::{ServerConfig, ServerConfigInput, ServerTransport, SharedState};

#[tauri::command]
pub async fn search_registry(
    state: State<'_, SharedState>,
    cache: State<'_, MarketplaceCache>,
    search: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<RegistrySearchResult, AppError> {
    if !cache.ensure_loaded().await {
        return Err(AppError::Protocol(
            "Failed to load marketplace data. Check your network connection.".into(),
        ));
    }

    let installed_ids: Vec<String> = {
        let state = state.lock().unwrap();
        state
            .servers
            .iter()
            .filter_map(|s| s.registry_name.clone())
            .collect()
    };

    let query = search.unwrap_or_default();
    let result = cache
        .search(
            &query,
            offset.unwrap_or(0),
            limit.unwrap_or(40),
            &installed_ids,
        )
        .await;

    Ok(result)
}

#[tauri::command]
pub async fn get_registry_server(
    cache: State<'_, MarketplaceCache>,
    id: String,
) -> Result<MarketplaceServerDetail, AppError> {
    if !cache.ensure_loaded().await {
        return Err(AppError::Protocol(
            "Failed to load marketplace data. Check your network connection.".into(),
        ));
    }

    cache
        .get_detail(&id)
        .await
        .ok_or_else(|| AppError::Validation(format!("Server not found: {id}")))
}

#[tauri::command]
pub async fn install_registry_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    cache: State<'_, MarketplaceCache>,
    id: String,
    env_vars: Option<HashMap<String, String>>,
) -> Result<ServerConfig, AppError> {
    if !cache.ensure_loaded().await {
        return Err(AppError::Protocol(
            "Failed to load marketplace data. Check your network connection.".into(),
        ));
    }

    let (display_name, config) = cache
        .get_install_config(&id)
        .await
        .ok_or_else(|| AppError::Validation(format!("No install config for server: {id}")))?;

    // Start with non-placeholder defaults, then overlay user-provided values.
    let mut env = config.default_env();

    if let Some(user_env) = env_vars {
        env.extend(user_env);
    }

    let input = ServerConfigInput {
        name: display_name,
        enabled: true,
        transport: ServerTransport::Stdio,
        command: Some(config.command),
        args: Some(config.args),
        env: if env.is_empty() { None } else { Some(env) },
        url: None,
        headers: None,
        tags: None,
    };

    crate::commands::servers::add_server_inner(&app, &state, input, Some(id))
}

#[tauri::command]
pub async fn check_runtime_deps() -> Result<RuntimeDeps, AppError> {
    let (npx, uvx, docker) = tokio::join!(
        check_command("npx", &["--version"]),
        check_command("uvx", &["--version"]),
        check_command("docker", &["--version"]),
    );

    Ok(RuntimeDeps { npx, uvx, docker })
}

async fn check_command(cmd: &str, args: &[&str]) -> bool {
    tokio::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}
