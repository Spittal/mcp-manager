use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::error::AppError;
use crate::persistence::save_servers;
use crate::state::{ServerConfig, ServerConfigInput, ServerStatus, SharedState};

/// Core server-creation logic, reusable by both the `add_server` command and registry install.
pub fn add_server_inner(
    app: &AppHandle,
    state: &SharedState,
    input: ServerConfigInput,
    registry_name: Option<String>,
) -> Result<ServerConfig, AppError> {
    let server = ServerConfig {
        id: Uuid::new_v4().to_string(),
        name: input.name,
        enabled: input.enabled,
        transport: input.transport,
        command: input.command,
        args: input.args,
        env: input.env,
        url: input.url,
        headers: input.headers,
        tags: input.tags,
        status: Some(ServerStatus::Disconnected),
        last_connected: None,
        managed: None,
        registry_name,
    };

    {
        let mut state = state.lock().unwrap();
        state.servers.push(server.clone());
        save_servers(app, &state.servers);
    }
    crate::tray::rebuild_tray_menu(app);
    Ok(server)
}

#[tauri::command]
pub async fn list_servers(state: State<'_, SharedState>) -> Result<Vec<ServerConfig>, AppError> {
    let state = state.lock().unwrap();
    Ok(state.servers.clone())
}

#[tauri::command]
pub async fn add_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    input: ServerConfigInput,
) -> Result<ServerConfig, AppError> {
    add_server_inner(&app, &state, input, None)
}

#[tauri::command]
pub async fn remove_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    {
        let mut state = state.lock().unwrap();
        let is_managed = state
            .servers
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.managed)
            .unwrap_or(false);
        if is_managed {
            return Err(AppError::Validation(
                "Cannot delete a managed server".into(),
            ));
        }
        let len_before = state.servers.len();
        state.servers.retain(|s| s.id != id);
        if state.servers.len() == len_before {
            return Err(AppError::ServerNotFound(id));
        }
        state.connections.remove(&id);
        save_servers(&app, &state.servers);
    }
    crate::tray::rebuild_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn update_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
    input: ServerConfigInput,
) -> Result<ServerConfig, AppError> {
    let updated = {
        let mut s = state.lock().unwrap();
        let server = s
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;

        server.name = input.name;
        server.transport = input.transport;
        server.command = input.command;
        server.args = input.args;
        server.env = input.env;
        server.url = input.url;
        server.headers = input.headers;
        server.enabled = input.enabled;
        server.tags = input.tags;
        // Preserve registry_name — don't overwrite from input

        let updated = server.clone();
        save_servers(&app, &s.servers);
        updated
    };
    crate::tray::rebuild_tray_menu(&app);
    Ok(updated)
}
