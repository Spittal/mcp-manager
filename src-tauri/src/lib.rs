mod commands;
mod error;
mod mcp;
mod persistence;
mod state;

use mcp::client::McpConnections;
use state::{AppState, OAuthStore};
use std::sync::Mutex;
use tauri::Manager;
use tracing::info;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Load persisted server configs
            let servers = persistence::load_servers(app.handle());
            info!("Loaded {} servers from persistent store", servers.len());

            let mut app_state = AppState::new();
            app_state.servers = servers;
            app.manage(Mutex::new(app_state));
            app.manage(tokio::sync::Mutex::new(McpConnections::new()));
            app.manage(tokio::sync::Mutex::new(OAuthStore::new()));

            // Start the MCP proxy server
            let proxy_state = mcp::proxy::ProxyState::new();
            let proxy_state_clone = proxy_state.clone();
            app.manage(proxy_state.clone());

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = mcp::proxy::start_proxy(handle, proxy_state_clone).await {
                    tracing::error!("Failed to start MCP proxy server: {e}");
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::remove_server,
            commands::servers::update_server,
            commands::connections::connect_server,
            commands::connections::disconnect_server,
            commands::tools::list_tools,
            commands::tools::list_all_tools,
            commands::tools::call_tool,
            commands::proxy::get_proxy_status,
            commands::oauth::start_oauth_flow,
            commands::oauth::clear_oauth_tokens,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
