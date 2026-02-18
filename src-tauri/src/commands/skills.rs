use std::path::PathBuf;

use serde::Serialize;
use tracing::warn;

use crate::error::AppError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub plugin_name: String,
    pub plugin_author: String,
    pub version: Option<String>,
    pub tools: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub info: SkillInfo,
    pub content: String,
}

/// YAML frontmatter fields we care about.
#[derive(Debug, serde::Deserialize, Default)]
struct SkillFrontmatter {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    tools: Option<String>,
}

/// Minimal plugin.json structure.
#[derive(Debug, serde::Deserialize)]
struct PluginJson {
    name: Option<String>,
    author: Option<PluginAuthor>,
}

#[derive(Debug, serde::Deserialize)]
struct PluginAuthor {
    name: Option<String>,
}

fn home_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Home directory not found",
        ))
    })
}

/// Parse YAML frontmatter from a SKILL.md file.
/// Returns (frontmatter, body) where body is everything after the closing `---`.
fn parse_frontmatter(content: &str) -> (SkillFrontmatter, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (SkillFrontmatter::default(), content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    if let Some(end_idx) = after_first.find("\n---") {
        let yaml_str = &after_first[..end_idx];
        let body_start = end_idx + 4; // skip "\n---"
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

/// Scan all skills from ~/.claude/plugins/marketplaces/*/plugins/*/skills/*/SKILL.md
fn scan_skills(home: &PathBuf) -> Vec<(SkillInfo, PathBuf)> {
    let marketplaces_dir = home.join(".claude/plugins/marketplaces");
    let mut results = Vec::new();

    let marketplace_entries = match std::fs::read_dir(&marketplaces_dir) {
        Ok(entries) => entries,
        Err(_) => return results,
    };

    for marketplace_entry in marketplace_entries.flatten() {
        let plugins_dir = marketplace_entry.path().join("plugins");
        let plugin_entries = match std::fs::read_dir(&plugins_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for plugin_entry in plugin_entries.flatten() {
            let plugin_path = plugin_entry.path();

            // Read plugin.json
            let plugin_json_path = plugin_path.join(".claude-plugin/plugin.json");
            let (plugin_name, plugin_author) = match std::fs::read_to_string(&plugin_json_path) {
                Ok(content) => match serde_json::from_str::<PluginJson>(&content) {
                    Ok(pj) => (
                        pj.name.unwrap_or_default(),
                        pj.author.and_then(|a| a.name).unwrap_or_default(),
                    ),
                    Err(_) => continue,
                },
                Err(_) => continue,
            };

            // Scan skills/ directory
            let skills_dir = plugin_path.join("skills");
            let skill_entries = match std::fs::read_dir(&skills_dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for skill_entry in skill_entries.flatten() {
                let skill_md_path = skill_entry.path().join("SKILL.md");
                if !skill_md_path.exists() {
                    continue;
                }

                let content = match std::fs::read_to_string(&skill_md_path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let (fm, _body) = parse_frontmatter(&content);
                let skill_dir_name = skill_entry.file_name().to_string_lossy().to_string();

                let id = format!("{}/{}", plugin_name, skill_dir_name);

                results.push((
                    SkillInfo {
                        id,
                        name: fm.name.unwrap_or_else(|| skill_dir_name.clone()),
                        description: fm.description.unwrap_or_default(),
                        plugin_name: plugin_name.clone(),
                        plugin_author: plugin_author.clone(),
                        version: fm.version,
                        tools: fm.tools,
                    },
                    skill_md_path,
                ));
            }
        }
    }

    results.sort_by(|a, b| a.0.name.to_lowercase().cmp(&b.0.name.to_lowercase()));
    results
}

#[tauri::command]
pub async fn list_skills() -> Result<Vec<SkillInfo>, AppError> {
    let home = home_dir()?;
    let skills = scan_skills(&home);
    Ok(skills.into_iter().map(|(info, _)| info).collect())
}

#[tauri::command]
pub async fn get_skill_content(id: String) -> Result<SkillDetail, AppError> {
    let home = home_dir()?;
    let skills = scan_skills(&home);

    let (info, path) = skills
        .into_iter()
        .find(|(info, _)| info.id == id)
        .ok_or_else(|| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Skill not found: {id}"),
            ))
        })?;

    let content = std::fs::read_to_string(&path)?;
    let (_fm, body) = parse_frontmatter(&content);

    Ok(SkillDetail {
        info,
        content: body,
    })
}
