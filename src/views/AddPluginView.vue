<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useRouter } from 'vue-router';
import { usePluginsStore } from '@/stores/plugins';
import type { PluginInfo } from '@/types/plugin';
import PluginMarketplaceCard from '@/components/PluginMarketplaceCard.vue';

const router = useRouter();
const store = usePluginsStore();

const searchInput = ref('');
let debounceTimer: ReturnType<typeof setTimeout> | null = null;
const installingPluginId = ref<string | null>(null);
const installError = ref<string | null>(null);
const updateError = ref<string | null>(null);

// Derive unique marketplace names from available plugins
const marketplaceNames = computed(() => {
  const names = new Set(store.marketplacePlugins.map(p => p.marketplace));
  return Array.from(names).filter(n => n && n !== 'unknown');
});

onMounted(() => {
  store.searchMarketplace('');
});

function onSearchInput() {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    store.searchMarketplace(searchInput.value);
  }, 300);
}

async function handleInstall(plugin: PluginInfo) {
  if (plugin.installed) return;
  installingPluginId.value = plugin.id;
  installError.value = null;
  try {
    const key = await store.installPlugin(plugin);
    store.selectPlugin(key);
    router.push('/plugins');
  } catch (e) {
    installError.value = `Failed to install ${plugin.name}: ${e}`;
    console.error('Install failed:', e);
  } finally {
    installingPluginId.value = null;
  }
}

async function handleUpdateAll() {
  updateError.value = null;
  for (const name of marketplaceNames.value) {
    try {
      await store.updateMarketplace(name);
    } catch (e) {
      updateError.value = String(e);
      console.error(`Failed to update ${name}:`, e);
    }
  }
  // Refresh plugin list after updating
  store.searchMarketplace(searchInput.value);
}
</script>

<template>
  <div class="flex h-full flex-col overflow-hidden">
    <div class="min-h-0 flex-1 overflow-y-auto px-6 py-4">
      <!-- Header -->
      <div class="mb-4 flex items-center justify-between">
        <div>
          <h1 class="text-base font-semibold text-text-primary">Add Plugin</h1>
          <span class="text-[11px] text-text-muted">Claude Code only</span>
        </div>
        <button
          v-if="marketplaceNames.length > 0"
          class="flex items-center gap-1.5 rounded-md bg-surface-2 px-3 py-1.5 text-xs text-text-secondary transition-colors hover:bg-surface-3"
          :disabled="store.updating"
          @click="handleUpdateAll"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-3.5 w-3.5"
            :class="store.updating ? 'animate-spin' : ''"
            viewBox="0 0 20 20"
            fill="currentColor"
          >
            <path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd" />
          </svg>
          {{ store.updating ? 'Updating...' : 'Update Marketplaces' }}
        </button>
      </div>

      <!-- Info banner -->
      <div class="mb-4 rounded-lg bg-surface-2 p-3 text-xs text-text-secondary">
        <strong>Plugins</strong> extend Claude Code with additional capabilities. Install and manage them here — they run exclusively within Claude Code.
      </div>

      <!-- Errors -->
      <div v-if="installError" class="mb-4 rounded-lg bg-status-error/10 p-3 text-xs text-status-error">
        {{ installError }}
      </div>
      <div v-if="updateError" class="mb-4 rounded-lg bg-status-error/10 p-3 text-xs text-status-error">
        {{ updateError }}
      </div>

      <!-- Search -->
      <div class="relative mb-4">
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-text-muted"
          viewBox="0 0 20 20"
          fill="currentColor"
        >
          <path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd" />
        </svg>
        <input
          v-model="searchInput"
          type="text"
          placeholder="Search plugins..."
          class="w-full rounded-lg border border-border bg-surface-1 py-2 pl-9 pr-3 text-xs text-text-primary placeholder-text-muted outline-none transition-colors focus:border-accent"
          @input="onSearchInput"
        />
      </div>

      <!-- Results -->
      <div>
        <!-- Loading -->
        <div v-if="store.marketplaceLoading && store.marketplacePlugins.length === 0" class="flex items-center justify-center py-12">
          <span class="text-xs text-text-muted">Loading plugins...</span>
        </div>

        <!-- Error -->
        <div v-else-if="store.marketplaceError && store.marketplacePlugins.length === 0" class="flex flex-col items-center justify-center gap-3 py-12">
          <p class="text-xs text-status-error">{{ store.marketplaceError }}</p>
          <button
            class="rounded-md bg-surface-2 px-3 py-1.5 text-xs text-text-secondary transition-colors hover:bg-surface-3"
            @click="store.searchMarketplace(searchInput)"
          >
            Retry
          </button>
        </div>

        <!-- Empty -->
        <div v-else-if="!store.marketplaceLoading && store.marketplacePlugins.length === 0" class="flex flex-col items-center justify-center py-12">
          <span class="mb-2 text-xs text-text-muted">No plugins found</span>
          <span class="text-[11px] text-text-muted">Make sure Claude Code is installed and has marketplaces configured.</span>
        </div>

        <!-- Grid -->
        <div v-else class="grid gap-2 pb-4" style="grid-template-columns: repeat(auto-fill, minmax(380px, 1fr))">
          <PluginMarketplaceCard
            v-for="plugin in store.marketplacePlugins"
            :key="plugin.id"
            :plugin="plugin"
            :installing="installingPluginId === plugin.id"
            @install="handleInstall(plugin)"
          />
        </div>

        <!-- Loading more -->
        <div v-if="store.marketplaceLoading && store.marketplacePlugins.length > 0" class="flex justify-center pb-4 pt-2">
          <span class="text-xs text-text-muted">Loading...</span>
        </div>
      </div>
    </div>
  </div>
</template>
