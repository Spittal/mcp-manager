import { defineStore } from 'pinia';
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { PluginInfo } from '@/types/plugin';

export const usePluginsStore = defineStore('plugins', () => {
  // --- Installed plugins ---
  const installedPlugins = ref<PluginInfo[]>([]);
  const selectedPluginId = ref<string | null>(null);

  async function loadInstalled() {
    try {
      installedPlugins.value = await invoke<PluginInfo[]>('list_installed_plugins');
    } catch (e) {
      console.error('Failed to load installed plugins:', e);
    }
  }

  function selectPlugin(id: string) {
    selectedPluginId.value = id;
  }

  function clearSelection() {
    selectedPluginId.value = null;
  }

  function splitId(id: string): { pluginName: string; marketplace: string } {
    const atIdx = id.indexOf('@');
    return {
      pluginName: atIdx >= 0 ? id.substring(0, atIdx) : id,
      marketplace: atIdx >= 0 ? id.substring(atIdx + 1) : '',
    };
  }

  async function installPlugin(plugin: PluginInfo): Promise<string> {
    const { pluginName, marketplace } = splitId(plugin.id);
    const key = await invoke<string>('install_plugin', { pluginName, marketplace });
    await loadInstalled();
    await refreshMarketplace();
    return key;
  }

  async function uninstallPlugin(id: string) {
    const { pluginName, marketplace } = splitId(id);
    await invoke('uninstall_plugin', { pluginName, marketplace });
    if (selectedPluginId.value === id) {
      clearSelection();
    }
    await loadInstalled();
    await refreshMarketplace();
  }

  async function togglePlugin(id: string, enabled: boolean) {
    const { pluginName, marketplace } = splitId(id);
    await invoke('toggle_plugin', { pluginName, marketplace, enabled });
    await loadInstalled();
  }

  // --- Marketplace browsing ---
  const marketplacePlugins = ref<PluginInfo[]>([]);
  const marketplaceLoading = ref(false);
  const marketplaceError = ref<string | null>(null);
  const updating = ref(false);
  // Track last search query so we can refresh without losing context
  const lastSearchQuery = ref('');

  async function searchMarketplace(query: string) {
    lastSearchQuery.value = query;
    marketplaceLoading.value = true;
    marketplaceError.value = null;
    try {
      const search = query.trim() || undefined;
      marketplacePlugins.value = await invoke<PluginInfo[]>('list_available_plugins', { search });
    } catch (e) {
      marketplaceError.value = String(e);
      console.error('Failed to search plugins marketplace:', e);
    } finally {
      marketplaceLoading.value = false;
    }
  }

  /** Re-fetch marketplace with last query (used after install/uninstall to update badges). */
  async function refreshMarketplace() {
    if (marketplacePlugins.value.length > 0 || lastSearchQuery.value) {
      try {
        const search = lastSearchQuery.value.trim() || undefined;
        marketplacePlugins.value = await invoke<PluginInfo[]>('list_available_plugins', { search });
      } catch {
        // Non-critical — marketplace refresh can fail silently
      }
    }
  }

  async function updateMarketplace(name: string) {
    updating.value = true;
    try {
      await invoke<string>('update_marketplace', { name });
    } finally {
      updating.value = false;
    }
  }

  return {
    // Installed
    installedPlugins,
    selectedPluginId,
    loadInstalled,
    selectPlugin,
    clearSelection,
    installPlugin,
    uninstallPlugin,
    togglePlugin,
    // Marketplace
    marketplacePlugins,
    marketplaceLoading,
    marketplaceError,
    updating,
    searchMarketplace,
    updateMarketplace,
  };
});
