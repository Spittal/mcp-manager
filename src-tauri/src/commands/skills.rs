use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;
use tauri::{AppHandle, State};
use tracing::{info, warn};

use crate::commands::skills_config;
use crate::error::AppError;
use crate::persistence;
use crate::state::skill::InstalledSkill;
use crate::state::skills_registry::{
    MarketplaceSkillDetail, SkillsMarketplaceCache, SkillsSearchResult,
};
use crate::state::SharedState;

// ---------------------------------------------------------------------------
// YAML frontmatter parser (reused from old implementation)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, Default)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
}

fn parse_frontmatter(content: &str) -> (SkillFrontmatter, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (SkillFrontmatter::default(), content.to_string());
    }

    let after_first = &trimmed[3..];
    if let Some(end_idx) = after_first.find("\n---") {
        let yaml_str = &after_first[..end_idx];
        let body_start = end_idx + 4;
        let body = after_first[body_start..]
            .trim_start_matches('\n')
            .to_string();

        match serde_yaml::from_str::<SkillFrontmatter>(yaml_str) {
            Ok(fm) => (fm, body),
            Err(e) => {
                warn!("Failed to parse SKILL.md frontmatter: {e}");
                (SkillFrontmatter::default(), content.to_string())
            }
        }
    } else {
        (SkillFrontmatter::default(), content.to_string())
    }
}

// ---------------------------------------------------------------------------
// Marketplace commands
// ---------------------------------------------------------------------------

/// Collect all skill_ids found on disk across all tool directories.
fn collect_local_skill_ids() -> Vec<String> {
    let tools = match skills_config::get_skill_tool_definitions() {
        Ok(t) => t,
        Err(_) => return vec![],
    };

    let mut ids = Vec::new();
    for tool in &tools {
        if !tool.skills_dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(&tool.skills_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("SKILL.md").exists() {
                if let Some(name) = path.file_name() {
                    ids.push(name.to_string_lossy().to_string());
                }
            }
        }
    }
    ids.sort();
    ids.dedup();
    ids
}

#[tauri::command]
pub async fn search_skills_marketplace(
    cache: State<'_, SkillsMarketplaceCache>,
    state: State<'_, SharedState>,
    search: String,
    limit: Option<u32>,
) -> Result<SkillsSearchResult, AppError> {
    let installed_ids: Vec<String> = {
        let s = state.lock().unwrap();
        s.installed_skills.iter().map(|sk| sk.id.clone()).collect()
    };

    let local_skill_ids = collect_local_skill_ids();

    let result = cache
        .search(&search, limit.unwrap_or(30), &installed_ids, &local_skill_ids)
        .await;
    Ok(result)
}

#[tauri::command]
pub async fn get_skills_marketplace_detail(
    cache: State<'_, SkillsMarketplaceCache>,
    source: String,
    skill_id: String,
    name: String,
    installs: u64,
) -> Result<MarketplaceSkillDetail, AppError> {
    let content = cache
        .fetch_skill_content(&source, &skill_id)
        .await
        .ok_or_else(|| {
            AppError::Protocol(format!(
                "Could not fetch SKILL.md for {source}/{skill_id}"
            ))
        })?;

    let (fm, _body) = parse_frontmatter(&content);

    Ok(MarketplaceSkillDetail {
        id: format!("{source}/{skill_id}"),
        name: fm.name.unwrap_or(name),
        source: source.clone(),
        skill_id,
        installs,
        description: fm.description.unwrap_or_default(),
        content,
    })
}

// ---------------------------------------------------------------------------
// Management commands
// ---------------------------------------------------------------------------

/// Frontend-facing installed skill (without full content to keep payloads small).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSkillInfo {
    pub id: String,
    pub name: String,
    pub skill_id: String,
    pub source: String,
    pub description: String,
    pub enabled: bool,
    pub installs: Option<u64>,
    pub managed: bool,
}

impl From<&InstalledSkill> for InstalledSkillInfo {
    fn from(s: &InstalledSkill) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            skill_id: s.skill_id.clone(),
            source: s.source.clone(),
            description: s.description.clone(),
            enabled: s.enabled,
            installs: s.installs,
            managed: s.managed,
        }
    }
}

#[tauri::command]
pub async fn list_installed_skills(
    state: State<'_, SharedState>,
) -> Result<Vec<InstalledSkillInfo>, AppError> {
    let s = state.lock().unwrap();
    Ok(s.installed_skills.iter().map(InstalledSkillInfo::from).collect())
}

#[tauri::command]
pub async fn install_skill(
    app: AppHandle,
    state: State<'_, SharedState>,
    cache: State<'_, SkillsMarketplaceCache>,
    id: String,
    name: String,
    source: String,
    skill_id: String,
    installs: Option<u64>,
) -> Result<InstalledSkillInfo, AppError> {
    // Check if already installed
    {
        let s = state.lock().unwrap();
        if s.installed_skills.iter().any(|sk| sk.id == id) {
            return Err(AppError::Validation(format!("Skill already installed: {id}")));
        }
    }

    // Fetch SKILL.md content
    let content = cache
        .fetch_skill_content(&source, &skill_id)
        .await
        .ok_or_else(|| {
            AppError::Protocol(format!(
                "Could not fetch SKILL.md for {source}/{skill_id}"
            ))
        })?;

    let (fm, _body) = parse_frontmatter(&content);

    let skill = InstalledSkill {
        id: id.clone(),
        name: fm.name.unwrap_or(name),
        skill_id: skill_id.clone(),
        source,
        description: fm.description.unwrap_or_default(),
        content: content.clone(),
        enabled: true,
        installs,
        managed: false,
    };

    let enabled_integrations: Vec<String>;
    {
        let mut s = state.lock().unwrap();
        s.installed_skills.push(skill.clone());
        enabled_integrations = s.enabled_skill_integrations.clone();
        persistence::save_installed_skills(&app, &s.installed_skills);
    }

    // Write SKILL.md to all enabled tool directories
    if let Err(e) = skills_config::write_skill(&skill_id, &content, &enabled_integrations) {
        warn!("Failed to write skill files: {e}");
    }

    info!("Installed skill: {id}");
    Ok(InstalledSkillInfo::from(&skill))
}

#[tauri::command]
pub async fn uninstall_skill(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<(), AppError> {
    // Check if managed — managed skills cannot be uninstalled directly
    {
        let s = state.lock().unwrap();
        let skill = s.installed_skills.iter().find(|sk| sk.id == id)
            .ok_or_else(|| AppError::Validation(format!("Skill not found: {id}")))?;
        if skill.managed {
            return Err(AppError::Validation("Cannot uninstall a managed skill. Disable the parent feature instead.".into()));
        }
    }

    let (skill_id, enabled_integrations) = {
        let mut s = state.lock().unwrap();
        let idx = s
            .installed_skills
            .iter()
            .position(|sk| sk.id == id)
            .ok_or_else(|| AppError::Validation(format!("Skill not found: {id}")))?;

        let skill = s.installed_skills.remove(idx);
        let integrations = s.enabled_skill_integrations.clone();
        persistence::save_installed_skills(&app, &s.installed_skills);
        (skill.skill_id, integrations)
    };

    // Remove SKILL.md from all enabled tool directories
    if let Err(e) = skills_config::remove_skill(&skill_id, &enabled_integrations) {
        warn!("Failed to remove skill files: {e}");
    }

    info!("Uninstalled skill: {id}");
    Ok(())
}

#[tauri::command]
pub async fn toggle_skill(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
    enabled: bool,
) -> Result<InstalledSkillInfo, AppError> {
    let (skill_id, content, enabled_integrations) = {
        let mut s = state.lock().unwrap();
        let skill = s
            .installed_skills
            .iter_mut()
            .find(|sk| sk.id == id)
            .ok_or_else(|| AppError::Validation(format!("Skill not found: {id}")))?;

        skill.enabled = enabled;
        let skill_id = skill.skill_id.clone();
        let content = skill.content.clone();
        let integrations = s.enabled_skill_integrations.clone();
        persistence::save_installed_skills(&app, &s.installed_skills);
        (skill_id, content, integrations)
    };

    if enabled {
        if let Err(e) = skills_config::write_skill(&skill_id, &content, &enabled_integrations) {
            warn!("Failed to write skill files on enable: {e}");
        }
    } else {
        if let Err(e) = skills_config::remove_skill(&skill_id, &enabled_integrations) {
            warn!("Failed to remove skill files on disable: {e}");
        }
    }

    let s = state.lock().unwrap();
    let skill = s.installed_skills.iter().find(|sk| sk.id == id).unwrap();
    Ok(InstalledSkillInfo::from(skill))
}

#[tauri::command]
pub async fn get_skill_content(
    state: State<'_, SharedState>,
    id: String,
) -> Result<SkillContentResponse, AppError> {
    let s = state.lock().unwrap();
    let skill = s
        .installed_skills
        .iter()
        .find(|sk| sk.id == id)
        .ok_or_else(|| AppError::Validation(format!("Skill not found: {id}")))?;

    let (_fm, body) = parse_frontmatter(&skill.content);

    Ok(SkillContentResponse {
        id: skill.id.clone(),
        name: skill.name.clone(),
        content: body,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillContentResponse {
    pub id: String,
    pub name: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Local skill discovery commands
// ---------------------------------------------------------------------------

/// A skill found on disk in a tool's skills directory that is NOT in the
/// installed_skills state (i.e. not installed via the marketplace).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalSkill {
    /// Composite id: "tool_id:skill_id"
    pub id: String,
    /// Display name from frontmatter or directory name
    pub name: String,
    /// Description from frontmatter
    pub description: String,
    /// Tool identifier, e.g. "claude-code"
    pub tool_id: String,
    /// Tool display name, e.g. "Claude Code"
    pub tool_name: String,
    /// Directory or file name (without extension)
    pub skill_id: String,
    /// Absolute path to the SKILL.md file
    pub file_path: String,
}

/// Scan all tool skill directories and return skills found on disk that are
/// NOT tracked in the installed_skills state.
#[tauri::command]
pub async fn list_local_skills(
    state: State<'_, SharedState>,
) -> Result<Vec<LocalSkill>, AppError> {
    let installed_skill_ids: HashSet<String> = {
        let s = state.lock().unwrap();
        s.installed_skills.iter().map(|sk| sk.skill_id.clone()).collect()
    };

    let tools = skills_config::get_skill_tool_definitions()?;
    let mut results: Vec<LocalSkill> = Vec::new();

    for tool in &tools {
        if !tool.skills_dir.exists() {
            continue;
        }

        let entries = match std::fs::read_dir(&tool.skills_dir) {
            Ok(entries) => entries,
            Err(e) => {
                warn!("Failed to read skills dir {}: {e}", tool.skills_dir.display());
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            // Case 1: subdirectory with SKILL.md
            if path.is_dir() {
                let skill_id = match path.file_name().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                if installed_skill_ids.contains(&skill_id) {
                    continue;
                }

                let skill_md = path.join("SKILL.md");
                if !skill_md.exists() {
                    continue;
                }

                if let Some(local_skill) =
                    read_local_skill(&skill_md, &skill_id, tool.id, tool.name)
                {
                    results.push(local_skill);
                }
            }
            // Case 2: standalone .md file
            else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                let skill_id = match path.file_stem().and_then(|n| n.to_str()) {
                    Some(name) => name.to_string(),
                    None => continue,
                };

                if installed_skill_ids.contains(&skill_id) {
                    continue;
                }

                if let Some(local_skill) =
                    read_local_skill(&path, &skill_id, tool.id, tool.name)
                {
                    results.push(local_skill);
                }
            }
        }
    }

    results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(results)
}

/// Read a single SKILL.md file and build a `LocalSkill`, returning `None` on
/// any IO or parse failure (we just skip broken entries).
fn read_local_skill(
    file_path: &Path,
    skill_id: &str,
    tool_id: &str,
    tool_name: &str,
) -> Option<LocalSkill> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| warn!("Failed to read {}: {e}", file_path.display()))
        .ok()?;

    let (fm, _body) = parse_frontmatter(&content);

    let name = fm
        .name
        .unwrap_or_else(|| skill_id.to_string());

    Some(LocalSkill {
        id: format!("{tool_id}:{skill_id}"),
        name,
        description: fm.description.unwrap_or_default(),
        tool_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        skill_id: skill_id.to_string(),
        file_path: file_path.to_string_lossy().into_owned(),
    })
}

/// Read a local skill file, strip frontmatter, and return its body content.
#[tauri::command]
pub async fn get_local_skill_content(
    file_path: String,
) -> Result<SkillContentResponse, AppError> {
    let path = Path::new(&file_path);
    let content = std::fs::read_to_string(path)?;
    let (fm, body) = parse_frontmatter(&content);

    let name = fm.name.unwrap_or_else(|| {
        // Derive name from parent directory name
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string()
    });

    Ok(SkillContentResponse {
        id: file_path,
        name,
        content: body,
    })
}

// ---------------------------------------------------------------------------
// Skill integration commands (Settings > Skills)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillToolInfo {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub enabled: bool,
    pub skills_path: String,
}

/// Detect which tools support skills, whether they're installed, and whether
/// skill management is enabled for each.
#[tauri::command]
pub async fn detect_skill_integrations(
    state: State<'_, SharedState>,
) -> Result<Vec<SkillToolInfo>, AppError> {
    let tools = skills_config::get_skill_tool_definitions()?;
    let enabled_ids: Vec<String> = {
        let s = state.lock().unwrap();
        s.enabled_skill_integrations.clone()
    };

    let results = tools
        .into_iter()
        .map(|tool| {
            // A tool is "installed" if its parent directory exists
            // e.g. ~/.claude/ exists means Claude Code is likely installed
            let parent = tool.skills_dir.parent();
            let installed = parent.map(|p| p.exists()).unwrap_or(false);

            SkillToolInfo {
                id: tool.id.to_string(),
                name: tool.name.to_string(),
                installed,
                enabled: enabled_ids.contains(&tool.id.to_string()),
                skills_path: tool.skills_dir.display().to_string(),
            }
        })
        .collect();

    Ok(results)
}

/// Enable skill file management for a tool — writes all enabled skills to that tool's directory.
#[tauri::command]
pub async fn enable_skill_integration(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<SkillToolInfo, AppError> {
    if !skills_config::supports_skills(&id) {
        return Err(AppError::Validation(format!(
            "Tool {id} does not support skills"
        )));
    }

    let (installed_skills, tools) = {
        let mut s = state.lock().unwrap();
        if !s.enabled_skill_integrations.contains(&id) {
            s.enabled_skill_integrations.push(id.clone());
            persistence::save_enabled_skill_integrations(&app, &s.enabled_skill_integrations);
        }
        (s.installed_skills.clone(), skills_config::get_skill_tool_definitions()?)
    };

    // Sync all enabled skills to this tool
    if let Err(e) = skills_config::sync_skills_for_tool(&id, &installed_skills) {
        warn!("Failed to sync skills for {id}: {e}");
    }

    let tool = tools.iter().find(|t| t.id == id).unwrap();
    let parent = tool.skills_dir.parent();
    let installed = parent.map(|p| p.exists()).unwrap_or(false);

    info!("Enabled skill integration for {}", tool.name);

    Ok(SkillToolInfo {
        id: tool.id.to_string(),
        name: tool.name.to_string(),
        installed,
        enabled: true,
        skills_path: tool.skills_dir.display().to_string(),
    })
}

/// Disable skill file management for a tool — removes all managed skill files.
#[tauri::command]
pub async fn disable_skill_integration(
    app: AppHandle,
    state: State<'_, SharedState>,
    id: String,
) -> Result<SkillToolInfo, AppError> {
    let (installed_skills, tools) = {
        let mut s = state.lock().unwrap();
        s.enabled_skill_integrations.retain(|i| i != &id);
        persistence::save_enabled_skill_integrations(&app, &s.enabled_skill_integrations);
        (s.installed_skills.clone(), skills_config::get_skill_tool_definitions()?)
    };

    // Remove all managed skill files from this tool
    if let Err(e) = skills_config::remove_all_skills_for_tool(&id, &installed_skills) {
        warn!("Failed to remove skills for {id}: {e}");
    }

    let tool = tools.iter().find(|t| t.id == id).ok_or_else(|| {
        AppError::Validation(format!("Unknown skill tool: {id}"))
    })?;
    let parent = tool.skills_dir.parent();
    let installed = parent.map(|p| p.exists()).unwrap_or(false);

    info!("Disabled skill integration for {}", tool.name);

    Ok(SkillToolInfo {
        id: tool.id.to_string(),
        name: tool.name.to_string(),
        installed,
        enabled: false,
        skills_path: tool.skills_dir.display().to_string(),
    })
}
