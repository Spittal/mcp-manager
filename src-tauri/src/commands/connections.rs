use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::state::{ConnectionState, ServerStatus, SharedState};

#[tauri::command]
pub async fn connect_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    // Update status to connecting
    {
        let mut s = state.lock().unwrap();
        let server = s
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;
        server.status = Some(ServerStatus::Connecting);
    }

    let _ = app.emit("server-status-changed", serde_json::json!({
        "id": id,
        "status": "connecting"
    }));

    // TODO: Actually spawn MCP client connection here
    // For now, simulate a successful connection
    {
        let mut s = state.lock().unwrap();
        if let Some(server) = s.servers.iter_mut().find(|s| s.id == id) {
            server.status = Some(ServerStatus::Connected);
            server.last_connected = Some(chrono_now());
        }
        s.connections.insert(
            id.clone(),
            ConnectionState {
                tools: vec![],
                child_pid: None,
            },
        );
    }

    let _ = app.emit("server-status-changed", serde_json::json!({
        "id": id,
        "status": "connected"
    }));

    Ok(())
}

#[tauri::command]
pub async fn disconnect_server(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    {
        let mut s = state.lock().unwrap();
        let server = s
            .servers
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| AppError::ServerNotFound(id.clone()))?;
        server.status = Some(ServerStatus::Disconnected);
        s.connections.remove(&id);
    }

    let _ = app.emit("server-status-changed", serde_json::json!({
        "id": id,
        "status": "disconnected"
    }));

    Ok(())
}

fn chrono_now() -> String {
    // Simple ISO timestamp without chrono dependency
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}", now)
}
