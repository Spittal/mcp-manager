use tauri::State;
use uuid::Uuid;

use crate::error::AppError;
use crate::state::{ServerConfig, ServerConfigInput, ServerStatus, SharedState};

#[tauri::command]
pub async fn list_servers(state: State<'_, SharedState>) -> Result<Vec<ServerConfig>, AppError> {
    let state = state.lock().unwrap();
    Ok(state.servers.clone())
}

#[tauri::command]
pub async fn add_server(
    state: State<'_, SharedState>,
    input: ServerConfigInput,
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
        tags: input.tags,
        status: Some(ServerStatus::Disconnected),
        last_connected: None,
    };

    let mut state = state.lock().unwrap();
    state.servers.push(server.clone());
    Ok(server)
}

#[tauri::command]
pub async fn remove_server(state: State<'_, SharedState>, id: String) -> Result<(), AppError> {
    let mut state = state.lock().unwrap();
    let len_before = state.servers.len();
    state.servers.retain(|s| s.id != id);
    if state.servers.len() == len_before {
        return Err(AppError::ServerNotFound(id));
    }
    state.connections.remove(&id);
    Ok(())
}
