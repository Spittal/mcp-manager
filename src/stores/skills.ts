import { defineStore } from 'pinia';
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  InstalledSkill,
  SkillContentResponse,
  SkillsSearchResult,
  MarketplaceSkillSummary,
  MarketplaceSkillDetail,
} from '@/types/skill';

export const useSkillsStore = defineStore('skills', () => {
  // --- Installed skills ---
  const installedSkills = ref<InstalledSkill[]>([]);
  const selectedSkillId = ref<string | null>(null);
  const skillContent = ref<SkillContentResponse | null>(null);

  async function loadInstalled() {
    try {
      installedSkills.value = await invoke<InstalledSkill[]>('list_installed_skills');
    } catch (e) {
      console.error('Failed to load installed skills:', e);
    }
  }

  async function selectSkill(id: string) {
    selectedSkillId.value = id;
    try {
      skillContent.value = await invoke<SkillContentResponse>('get_skill_content', { id });
    } catch (e) {
      console.error('Failed to load skill content:', e);
      skillContent.value = null;
    }
  }

  function clearSelection() {
    selectedSkillId.value = null;
    skillContent.value = null;
  }

  async function toggleSkill(id: string, enabled: boolean) {
    try {
      await invoke('toggle_skill', { id, enabled });
      await loadInstalled();
    } catch (e) {
      console.error('Failed to toggle skill:', e);
    }
  }

  async function uninstallSkill(id: string) {
    try {
      await invoke('uninstall_skill', { id });
      if (selectedSkillId.value === id) {
        clearSelection();
      }
      await loadInstalled();
    } catch (e) {
      console.error('Failed to uninstall skill:', e);
    }
  }

  async function installSkill(summary: MarketplaceSkillSummary): Promise<InstalledSkill> {
    const result = await invoke<InstalledSkill>('install_skill', {
      id: summary.id,
      name: summary.name,
      source: summary.source,
      skillId: summary.skillId,
      installs: summary.installs,
    });
    await loadInstalled();
    return result;
  }

  // --- Marketplace ---
  const marketplaceSkills = ref<MarketplaceSkillSummary[]>([]);
  const marketplaceLoading = ref(false);
  const marketplaceError = ref<string | null>(null);
  const marketplaceCount = ref(0);

  async function searchMarketplace(query: string) {
    marketplaceLoading.value = true;
    marketplaceError.value = null;
    try {
      const result = await invoke<SkillsSearchResult>('search_skills_marketplace', {
        search: query,
        limit: 30,
      });
      marketplaceSkills.value = result.skills;
      marketplaceCount.value = result.count;
    } catch (e) {
      marketplaceError.value = String(e);
      console.error('Failed to search skills marketplace:', e);
    } finally {
      marketplaceLoading.value = false;
    }
  }

  async function fetchMarketplaceDetail(
    source: string,
    skillId: string,
    name: string,
    installs: number,
  ): Promise<MarketplaceSkillDetail> {
    return invoke<MarketplaceSkillDetail>('get_skills_marketplace_detail', {
      source,
      skillId,
      name,
      installs,
    });
  }

  return {
    // Installed
    installedSkills,
    selectedSkillId,
    skillContent,
    loadInstalled,
    selectSkill,
    clearSelection,
    toggleSkill,
    uninstallSkill,
    installSkill,
    // Marketplace
    marketplaceSkills,
    marketplaceLoading,
    marketplaceError,
    marketplaceCount,
    searchMarketplace,
    fetchMarketplaceDetail,
  };
});
