use tauri::{AppHandle, State};

use crate::error::AppError;
use crate::persistence::save_stats;
use crate::stats::{ServerStats, StatsStore};

#[tauri::command]
pub async fn get_server_stats(
    stats_store: State<'_, StatsStore>,
    server_id: String,
) -> Result<ServerStats, AppError> {
    let store = stats_store.read().await;
    Ok(store.get(&server_id).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn reset_server_stats(
    app: AppHandle,
    stats_store: State<'_, StatsStore>,
    server_id: String,
) -> Result<(), AppError> {
    let mut store = stats_store.write().await;
    store.remove(&server_id);
    save_stats(&app, &store);
    Ok(())
}
