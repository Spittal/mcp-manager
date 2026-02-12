mod commands;
mod error;
mod mcp;
mod state;

use state::AppState;
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_http::init())
        .manage(Mutex::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![
            commands::servers::list_servers,
            commands::servers::add_server,
            commands::servers::remove_server,
            commands::connections::connect_server,
            commands::connections::disconnect_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
