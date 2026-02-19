// Marketplace types (from skills.sh)
export interface SkillsSearchResult {
  skills: MarketplaceSkillSummary[];
  count: number;
}

export interface MarketplaceSkillSummary {
  id: string;
  name: string;
  source: string;
  skillId: string;
  installs: number;
  installed: boolean;
}

export interface MarketplaceSkillDetail {
  id: string;
  name: string;
  source: string;
  skillId: string;
  installs: number;
  description: string;
  content: string;
}

// Installed skill (persisted state — content excluded for list payloads)
export interface InstalledSkill {
  id: string;
  name: string;
  skillId: string;
  source: string;
  description: string;
  enabled: boolean;
  installs?: number;
}

// Skill content response (for detail view)
export interface SkillContentResponse {
  id: string;
  name: string;
  content: string;
}

// Skill tool info (for Settings > Skills)
export interface SkillToolInfo {
  id: string;
  name: string;
  installed: boolean;
  enabled: boolean;
  skillsPath: string;
}
