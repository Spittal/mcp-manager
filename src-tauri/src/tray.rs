use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};
use tracing::error;

use crate::state::{ServerStatus, SharedState};

/// Set up the system tray icon and initial menu. Called once from `lib.rs` setup.
pub fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_tray_menu(&app.handle())?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().unwrap().clone())
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(handle_menu_event)
        .build(app)?;

    Ok(())
}

/// Rebuild the tray menu from current server state. Called after any state change.
pub fn rebuild_tray_menu(app: &AppHandle) {
    match build_tray_menu(app) {
        Ok(menu) => {
            if let Some(tray) = app.tray_by_id("main") {
                if let Err(e) = tray.set_menu(Some(menu)) {
                    error!("Failed to update tray menu: {e}");
                }
            }
        }
        Err(e) => {
            error!("Failed to build tray menu: {e}");
        }
    }
}

fn build_tray_menu(
    app: &AppHandle,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let mut builder = MenuBuilder::new(app);

    // Add server items from AppState
    let state = app.state::<SharedState>();
    let s = state.lock().unwrap();

    if s.servers.is_empty() {
        let item = MenuItemBuilder::new("No servers configured")
            .id("no-servers")
            .enabled(false)
            .build(app)?;
        builder = builder.item(&item);
    } else {
        for server in &s.servers {
            let indicator = match server.status.as_ref() {
                Some(ServerStatus::Connected) => "●",
                Some(ServerStatus::Connecting) => "◌",
                Some(ServerStatus::Disconnected) | Some(ServerStatus::Error) | None => "○",
            };

            let label = format!("{indicator}  {}", server.name);
            let item = MenuItemBuilder::new(label)
                .id(format!("server:{}", server.id))
                .build(app)?;
            builder = builder.item(&item);
        }
    }

    builder = builder.separator();

    let show = MenuItemBuilder::new("Show MCP Manager")
        .id("show")
        .build(app)?;
    builder = builder.item(&show);

    let quit = MenuItemBuilder::new("Quit").id("quit").build(app)?;
    builder = builder.item(&quit);

    Ok(builder.build()?)
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().0.as_str();

    match id {
        "quit" => {
            app.exit(0);
        }
        "show" => {
            focus_main_window(app);
        }
        _ if id.starts_with("server:") => {
            let server_id = &id["server:".len()..];
            focus_main_window(app);
            let _ = app.emit(
                "navigate-to-server",
                serde_json::json!({ "serverId": server_id }),
            );
        }
        _ => {}
    }
}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
