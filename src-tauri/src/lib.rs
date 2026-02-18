mod commands;
mod error;
mod mcp;
mod memory_client;
mod persistence;
mod state;
pub mod stats;
mod tray;

use commands::status::SharedSystem;
use mcp::client::McpConnections;
use state::registry::MarketplaceCache;
use state::{AppState, OAuthStore};
use stats::StatsStore;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tokio::sync::RwLock;
use tracing::info;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Load persisted server configs, enabled integrations, and stats
            let servers = persistence::load_servers(app.handle());
            let enabled_integrations = persistence::load_enabled_integrations(app.handle());
            let stats = persistence::load_stats(app.handle());
            let embedding_config = persistence::load_embedding_config(app.handle());
            info!(
                "Loaded {} servers, {} enabled integrations, {} server stats from persistent store",
                servers.len(),
                enabled_integrations.len(),
                stats.len()
            );

            let mut app_state = AppState::new();
            app_state.servers = servers;
            app_state.enabled_integrations = enabled_integrations;
            app_state.embedding_config = embedding_config;
            app.manage(Mutex::new(app_state));
            app.manage(tokio::sync::Mutex::new(McpConnections::new()));
            app.manage(tokio::sync::Mutex::new(OAuthStore::new()));
            app.manage(Mutex::new(sysinfo::System::new()) as SharedSystem);

            let stats_store: StatsStore = Arc::new(RwLock::new(stats));
            app.manage(stats_store);
            app.manage(MarketplaceCache::new());

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

            // Auto-reconnect servers that were connected in the previous session
            let reconnect_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                commands::connections::reconnect_on_startup(reconnect_handle).await;
            });

            tray::setup_tray(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::remove_server,
            commands::servers::update_server,
            commands::connections::connect_server,
            commands::connections::disconnect_server,
            commands::connections::drain_log_buffer,
            commands::tools::list_tools,
            commands::tools::list_all_tools,
            commands::tools::call_tool,
            commands::proxy::get_proxy_status,
            commands::integrations::detect_integrations,
            commands::integrations::enable_integration,
            commands::integrations::disable_integration,
            commands::oauth::start_oauth_flow,
            commands::oauth::clear_oauth_tokens,
            commands::skills::list_skills,
            commands::skills::get_skill_content,
            commands::memory::get_memory_status,
            commands::memory::enable_memory,
            commands::memory::disable_memory,
            commands::memory::get_embedding_config,
            commands::memory::save_embedding_config_cmd,
            commands::memory::delete_ollama_model,
            commands::stats::get_server_stats,
            commands::stats::reset_server_stats,
            commands::status::get_system_status,
            commands::memories::search_memories,
            commands::memories::get_memory,
            commands::memories::check_memory_health,
            commands::registry::search_registry,
            commands::registry::get_registry_server,
            commands::registry::install_registry_server,
            commands::registry::check_runtime_deps,
            commands::data_management::export_memories,
            commands::data_management::import_memories,
            commands::data_management::format_memory_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
