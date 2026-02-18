use serde::Serialize;
use tauri::State;

use crate::error::AppError;
use crate::mcp::proxy::ProxyState;

#[derive(Debug, Clone, Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub port: u16,
}

#[tauri::command]
pub async fn get_proxy_status(proxy_state: State<'_, ProxyState>) -> Result<ProxyStatus, AppError> {
    Ok(ProxyStatus {
        running: proxy_state.is_running().await,
        port: proxy_state.port().await,
    })
}
