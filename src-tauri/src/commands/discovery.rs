use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::integrations::update_all_integration_configs;
use crate::error::AppError;
use crate::mcp::proxy::ProxyState;
use crate::persistence::save_tool_discovery;
use crate::state::SharedState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryStatus {
    pub enabled: bool,
}

#[tauri::command]
pub async fn get_discovery_mode(state: State<'_, SharedState>) -> Result<DiscoveryStatus, AppError> {
    let s = state.lock().unwrap();
    Ok(DiscoveryStatus {
        enabled: s.tool_discovery_enabled,
    })
}

#[tauri::command]
pub async fn set_discovery_mode(
    app: AppHandle,
    state: State<'_, SharedState>,
    proxy_state: State<'_, ProxyState>,
    enabled: bool,
) -> Result<DiscoveryStatus, AppError> {
    {
        let mut s = state.lock().unwrap();
        s.tool_discovery_enabled = enabled;
    }

    save_tool_discovery(&app, enabled);

    let port = proxy_state.port().await;
    if let Err(e) = update_all_integration_configs(&app, port) {
        tracing::warn!("Failed to update integration configs after discovery toggle: {e}");
    }

    Ok(DiscoveryStatus { enabled })
}
