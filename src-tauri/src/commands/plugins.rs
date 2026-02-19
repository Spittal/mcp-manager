use tracing::info;

use crate::error::AppError;
use crate::state::plugin::{PluginInfo, PluginListOutput};

// ---------------------------------------------------------------------------
// CLI helper
// ---------------------------------------------------------------------------

/// Run a `claude plugin <subcommand>` and return stdout.
async fn run_claude_plugin(args: &[&str]) -> Result<String, AppError> {
    let output = tokio::process::Command::new("claude")
        .arg("plugin")
        .args(args)
        .env_remove("CLAUDECODE")
        .output()
        .await
        .map_err(|e| {
            tracing::warn!("Failed to run claude CLI: {e}");
            AppError::DependencyNotFound(
                "Claude Code CLI not found. Make sure `claude` is installed and in your PATH."
                    .to_string(),
            )
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let msg = if stderr.is_empty() {
            stdout.clone()
        } else {
            stderr
        };
        return Err(AppError::Protocol(msg));
    }

    Ok(stdout)
}

/// Fetch the full available+installed list from `claude plugin list --available --json`,
/// merging both into a unified `Vec<PluginInfo>`.
async fn fetch_all_plugins() -> Result<Vec<PluginInfo>, AppError> {
    let json = run_claude_plugin(&["list", "--available", "--json"]).await?;
    let output: PluginListOutput = serde_json::from_str(&json).map_err(|e| {
        AppError::Protocol(format!("Failed to parse plugin list output: {e}"))
    })?;
    Ok(output.into_plugin_list())
}

// ---------------------------------------------------------------------------
// Browse commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_available_plugins(
    search: Option<String>,
) -> Result<Vec<PluginInfo>, AppError> {
    let mut all = fetch_all_plugins().await?;

    // Client-side search filter
    if let Some(ref query) = search {
        let q = query.to_lowercase();
        if !q.is_empty() {
            all.retain(|p| {
                p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
                    || p.marketplace.to_lowercase().contains(&q)
            });
        }
    }

    // Sort alphabetically
    all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(all)
}

#[tauri::command]
pub async fn list_installed_plugins() -> Result<Vec<PluginInfo>, AppError> {
    let all = fetch_all_plugins().await?;
    Ok(all.into_iter().filter(|p| p.installed).collect())
}

// ---------------------------------------------------------------------------
// Management commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn install_plugin(
    plugin_name: String,
    marketplace: String,
) -> Result<String, AppError> {
    let key = format!("{plugin_name}@{marketplace}");
    info!("Installing plugin via CLI: {key}");
    run_claude_plugin(&["install", &key]).await?;
    info!("Installed plugin: {key}");
    Ok(key)
}

#[tauri::command]
pub async fn uninstall_plugin(
    plugin_name: String,
    marketplace: String,
) -> Result<(), AppError> {
    let key = format!("{plugin_name}@{marketplace}");
    info!("Uninstalling plugin via CLI: {key}");
    run_claude_plugin(&["uninstall", &key]).await?;
    info!("Uninstalled plugin: {key}");
    Ok(())
}

#[tauri::command]
pub async fn toggle_plugin(
    plugin_name: String,
    marketplace: String,
    enabled: bool,
) -> Result<(), AppError> {
    let key = format!("{plugin_name}@{marketplace}");
    let subcmd = if enabled { "enable" } else { "disable" };
    info!("Toggling plugin via CLI: {subcmd} {key}");
    run_claude_plugin(&[subcmd, &key]).await?;
    info!("Toggled plugin {key} -> {enabled}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Marketplace update
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn update_marketplace(name: String) -> Result<String, AppError> {
    info!("Updating marketplace: {name}");
    let result = run_claude_plugin(&["marketplace", "update", &name]).await?;
    info!("Marketplace {name} updated successfully");
    Ok(result)
}
