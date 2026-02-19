use std::path::PathBuf;

use tracing::{info, warn};

use crate::error::AppError;
use crate::state::skill::InstalledSkill;

// ---------------------------------------------------------------------------
// Tool definitions — which AI tools support skills and where they go
// ---------------------------------------------------------------------------

pub struct SkillToolDef {
    pub id: &'static str,
    pub name: &'static str,
    pub skills_dir: PathBuf,
}

fn home_dir() -> Result<PathBuf, AppError> {
    dirs::home_dir().ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Home directory not found",
        ))
    })
}

pub fn get_skill_tool_definitions() -> Result<Vec<SkillToolDef>, AppError> {
    let home = home_dir()?;
    Ok(vec![
        SkillToolDef {
            id: "claude-code",
            name: "Claude Code",
            skills_dir: home.join(".claude/skills"),
        },
        SkillToolDef {
            id: "cursor",
            name: "Cursor",
            skills_dir: home.join(".cursor/skills"),
        },
        SkillToolDef {
            id: "windsurf",
            name: "Windsurf",
            skills_dir: home.join(".codeium/windsurf/skills"),
        },
        SkillToolDef {
            id: "opencode",
            name: "OpenCode",
            skills_dir: home.join(".config/opencode/skills"),
        },
        SkillToolDef {
            id: "codex",
            name: "Codex",
            skills_dir: home.join(".codex/skills"),
        },
    ])
}

/// Returns whether a given integration ID supports skills.
pub fn supports_skills(integration_id: &str) -> bool {
    matches!(
        integration_id,
        "claude-code" | "cursor" | "windsurf" | "opencode" | "codex"
    )
}

// ---------------------------------------------------------------------------
// Write / remove SKILL.md files
// ---------------------------------------------------------------------------

/// Write a skill's SKILL.md to all enabled tool directories.
pub fn write_skill(
    skill_id: &str,
    content: &str,
    enabled_skill_integrations: &[String],
) -> Result<(), AppError> {
    let tools = get_skill_tool_definitions()?;

    for tool in &tools {
        if !enabled_skill_integrations.contains(&tool.id.to_string()) {
            continue;
        }

        let skill_dir = tool.skills_dir.join(skill_id);
        std::fs::create_dir_all(&skill_dir)?;

        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, content)?;
        info!("Wrote SKILL.md to {} for {}", skill_path.display(), tool.name);
    }

    Ok(())
}

/// Remove a skill's directory from all enabled tool directories.
pub fn remove_skill(
    skill_id: &str,
    enabled_skill_integrations: &[String],
) -> Result<(), AppError> {
    let tools = get_skill_tool_definitions()?;

    for tool in &tools {
        if !enabled_skill_integrations.contains(&tool.id.to_string()) {
            continue;
        }

        let skill_dir = tool.skills_dir.join(skill_id);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)?;
            info!("Removed skill dir {} for {}", skill_dir.display(), tool.name);
        }
    }

    Ok(())
}

/// Sync all installed skills for a specific tool.
/// Writes enabled skills and removes disabled ones.
pub fn sync_skills_for_tool(
    tool_id: &str,
    installed_skills: &[InstalledSkill],
) -> Result<(), AppError> {
    let tools = get_skill_tool_definitions()?;
    let tool = tools.iter().find(|t| t.id == tool_id).ok_or_else(|| {
        AppError::Validation(format!("Unknown skill tool: {tool_id}"))
    })?;

    for skill in installed_skills {
        let skill_dir = tool.skills_dir.join(&skill.skill_id);

        if skill.enabled {
            std::fs::create_dir_all(&skill_dir)?;
            let skill_path = skill_dir.join("SKILL.md");
            std::fs::write(&skill_path, &skill.content)?;
            info!("Synced skill {} to {}", skill.skill_id, tool.name);
        } else if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)?;
            info!("Removed disabled skill {} from {}", skill.skill_id, tool.name);
        }
    }

    Ok(())
}

/// Remove all managed skill files from a specific tool.
pub fn remove_all_skills_for_tool(
    tool_id: &str,
    installed_skills: &[InstalledSkill],
) -> Result<(), AppError> {
    let tools = get_skill_tool_definitions()?;
    let tool = tools.iter().find(|t| t.id == tool_id).ok_or_else(|| {
        AppError::Validation(format!("Unknown skill tool: {tool_id}"))
    })?;

    for skill in installed_skills {
        let skill_dir = tool.skills_dir.join(&skill.skill_id);
        if skill_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&skill_dir) {
                warn!("Failed to remove {} from {}: {e}", skill.skill_id, tool.name);
            }
        }
    }

    Ok(())
}
