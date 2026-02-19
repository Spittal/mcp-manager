use serde::{Deserialize, Serialize};
use std::path::Path;

// ---------------------------------------------------------------------------
// Types returned to the frontend — derived from `claude plugin list --json`
// ---------------------------------------------------------------------------

/// An installed plugin from `claude plugin list --json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPlugin {
    pub id: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub install_path: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
    #[serde(default)]
    pub last_updated: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    /// MCP servers bundled with this plugin (kept for deserialization).
    #[serde(default)]
    pub mcp_servers: Option<serde_json::Value>,
}

impl InstalledPlugin {
    /// Scan the install directory to discover what the plugin includes.
    /// Returns structured components grouped by category.
    pub fn discover_components(&self) -> Vec<PluginComponent> {
        let mut components = Vec::new();

        if let Some(ref path) = self.install_path {
            let root = Path::new(path);
            if root.exists() {
                let checks: &[(&str, &str)] = &[
                    ("skills", "Skills"),
                    ("agents", "Agents"),
                    ("commands", "Commands"),
                    ("hooks", "Hooks"),
                ];

                for &(dir, category) in checks {
                    let items = Self::list_dir_items(&root.join(dir));
                    if !items.is_empty() {
                        components.push(PluginComponent {
                            category: category.to_string(),
                            items,
                        });
                    }
                }
            }
        }

        // MCP servers from the CLI data
        if let Some(serde_json::Value::Object(servers)) = &self.mcp_servers {
            if !servers.is_empty() {
                let items: Vec<String> = servers.keys().cloned().collect();
                components.push(PluginComponent {
                    category: "MCP Servers".to_string(),
                    items,
                });
            }
        }

        components
    }

    /// List meaningful entries in a directory, stripping extensions for display.
    fn list_dir_items(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };

        let mut items: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy().to_string();
                // Skip hidden files and config files
                if name_str.starts_with('.') || name_str == "hooks.json" {
                    return None;
                }
                // Strip common extensions for cleaner display
                let display = name_str
                    .strip_suffix(".md")
                    .or_else(|| name_str.strip_suffix(".sh"))
                    .or_else(|| name_str.strip_suffix(".cmd"))
                    .or_else(|| name_str.strip_suffix(".json"))
                    .unwrap_or(&name_str)
                    .to_string();
                Some(display)
            })
            .collect();

        items.sort();
        items
    }
}

/// An available plugin from `claude plugin list --available --json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailablePluginRaw {
    pub plugin_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub marketplace_name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub source: Option<serde_json::Value>,
    #[serde(default)]
    pub install_count: Option<u64>,
}

/// The full JSON output of `claude plugin list --available --json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginListOutput {
    #[serde(default)]
    pub installed: Vec<InstalledPlugin>,
    #[serde(default)]
    pub available: Vec<AvailablePluginRaw>,
}

impl PluginListOutput {
    /// Merge available and installed into a unified list.
    /// Installed plugins that don't appear in the available list (e.g. remote
    /// plugins like Slack) are included via `PluginInfo::from_installed`.
    pub fn into_plugin_list(self) -> Vec<PluginInfo> {
        let mut plugins: Vec<PluginInfo> = self
            .available
            .iter()
            .map(|raw| PluginInfo::from_available(raw, &self.installed))
            .collect();

        // IDs already covered by the available list
        let available_ids: std::collections::HashSet<&str> =
            self.available.iter().map(|a| a.plugin_id.as_str()).collect();

        // Append installed-only plugins
        for inst in &self.installed {
            if !available_ids.contains(inst.id.as_str()) {
                plugins.push(PluginInfo::from_installed(inst));
            }
        }

        plugins
    }
}

// ---------------------------------------------------------------------------
// Frontend-facing types (normalized from CLI output)
// ---------------------------------------------------------------------------

/// A group of items within a plugin (e.g. "Skills" with item names).
#[derive(Debug, Clone, Serialize)]
pub struct PluginComponent {
    pub category: String,
    pub items: Vec<String>,
}

/// Plugin info sent to the frontend for display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub marketplace: String,
    pub version: Option<String>,
    pub install_count: Option<u64>,
    pub is_remote: bool,
    pub installed: bool,
    pub enabled: bool,
    pub scope: Option<String>,
    /// What this plugin includes, grouped by category with item names.
    pub components: Vec<PluginComponent>,
}

impl PluginInfo {
    /// Build from an available plugin entry, merging installed state.
    pub fn from_available(raw: &AvailablePluginRaw, installed: &[InstalledPlugin]) -> Self {
        let inst = installed.iter().find(|i| i.id == raw.plugin_id);
        let is_remote = matches!(&raw.source, Some(serde_json::Value::Object(_)));

        let components = inst
            .map(|i| i.discover_components())
            .unwrap_or_default();

        PluginInfo {
            id: raw.plugin_id.clone(),
            name: raw.name.clone(),
            description: raw.description.clone().unwrap_or_default(),
            marketplace: raw
                .marketplace_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            version: raw.version.clone(),
            install_count: raw.install_count,
            is_remote,
            installed: inst.is_some(),
            enabled: inst.and_then(|i| i.enabled).unwrap_or(false),
            scope: inst.and_then(|i| i.scope.clone()),
            components,
        }
    }

    /// Build from an installed plugin that has no entry in the available list.
    pub fn from_installed(inst: &InstalledPlugin) -> Self {
        let parts: Vec<&str> = inst.id.splitn(2, '@').collect();
        let name = parts.first().copied().unwrap_or(&inst.id).to_string();
        let marketplace = parts.get(1).copied().unwrap_or("unknown").to_string();

        PluginInfo {
            id: inst.id.clone(),
            name,
            description: String::new(),
            marketplace,
            version: inst.version.clone(),
            install_count: None,
            is_remote: false,
            installed: true,
            enabled: inst.enabled.unwrap_or(true),
            scope: inst.scope.clone(),
            components: inst.discover_components(),
        }
    }
}
