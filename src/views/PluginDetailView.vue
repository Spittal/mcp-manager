<script setup lang="ts">
import { computed, ref } from 'vue';
import { usePluginsStore } from '@/stores/plugins';
import { storeToRefs } from 'pinia';

const store = usePluginsStore();
const { selectedPluginId, installedPlugins } = storeToRefs(store);

const selectedPlugin = computed(() =>
  installedPlugins.value.find(p => p.id === selectedPluginId.value)
);

const confirmUninstall = ref(false);
const error = ref<string | null>(null);
const toggling = ref(false);
const uninstalling = ref(false);

async function onToggle() {
  if (!selectedPlugin.value || toggling.value) return;
  error.value = null;
  toggling.value = true;
  try {
    await store.togglePlugin(selectedPlugin.value.id, !selectedPlugin.value.enabled);
  } catch (e) {
    error.value = `Toggle failed: ${e}`;
  } finally {
    toggling.value = false;
  }
}

async function onUninstall() {
  if (!selectedPlugin.value || uninstalling.value) return;
  error.value = null;
  uninstalling.value = true;
  try {
    await store.uninstallPlugin(selectedPlugin.value.id);
    confirmUninstall.value = false;
  } catch (e) {
    error.value = `Uninstall failed: ${e}`;
    uninstalling.value = false;
  }
}

const categoryIcons: Record<string, string> = {
  Skills: 'M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z',
  Agents: 'M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z',
  Commands: 'M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z',
  Hooks: 'M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1',
  'MCP Servers': 'M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01',
};
</script>

<template>
  <div v-if="selectedPlugin" class="flex h-full flex-col">
    <!-- Header -->
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">{{ selectedPlugin.name }}</h1>
      <span class="font-mono text-xs text-text-muted">{{ selectedPlugin.marketplace }}</span>
      <span
        v-if="selectedPlugin.version"
        class="text-[10px] text-text-muted"
      >
        v{{ selectedPlugin.version }}
      </span>
      <span
        v-if="selectedPlugin.isRemote"
        class="rounded bg-surface-2 px-1.5 py-0.5 text-[10px] text-text-muted"
      >
        Remote
      </span>
      <div class="ml-auto flex items-center gap-2">
        <!-- Toggle -->
        <button
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none"
          :class="[
            selectedPlugin.enabled ? 'bg-status-connected' : 'bg-surface-3',
            toggling ? 'opacity-50' : '',
          ]"
          :disabled="toggling"
          @click="onToggle"
        >
          <span
            class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out"
            :class="selectedPlugin.enabled ? 'translate-x-4' : 'translate-x-0'"
          />
        </button>
        <!-- Uninstall -->
        <template v-if="confirmUninstall">
          <button
            class="rounded bg-status-error px-3 py-1 text-[11px] font-medium text-white transition-colors hover:bg-status-error/80"
            :disabled="uninstalling"
            @click="onUninstall"
          >
            {{ uninstalling ? 'Removing...' : 'Confirm' }}
          </button>
          <button
            class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2"
            :disabled="uninstalling"
            @click="confirmUninstall = false"
          >
            Cancel
          </button>
        </template>
        <button
          v-else
          class="rounded bg-surface-3 px-3 py-1 text-[11px] text-text-secondary transition-colors hover:bg-surface-2"
          @click="confirmUninstall = true"
        >
          Uninstall
        </button>
      </div>
    </header>

    <!-- Content -->
    <div class="min-h-0 flex-1 overflow-y-auto p-4">
      <!-- Error -->
      <div v-if="error" class="mb-4 rounded-lg bg-status-error/10 p-3 text-xs text-status-error">
        {{ error }}
      </div>

      <!-- Auth info -->
      <div class="mb-4 rounded-lg bg-surface-2 p-3 text-xs text-text-secondary">
        If this plugin requires authentication, you'll be prompted to authorize on your first use in Claude Code.
      </div>

      <!-- Description -->
      <section v-if="selectedPlugin.description" class="mb-4">
        <p class="text-xs text-text-secondary">{{ selectedPlugin.description }}</p>
      </section>

      <!-- Components (what's included) -->
      <section v-if="selectedPlugin.components?.length" class="mb-4">
        <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Includes</h2>
        <div class="space-y-2">
          <div
            v-for="comp in selectedPlugin.components"
            :key="comp.category"
            class="rounded border border-border bg-surface-1 p-3"
          >
            <div class="mb-1.5 flex items-center gap-1.5">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-3.5 w-3.5 text-text-muted"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <path stroke-linecap="round" stroke-linejoin="round" :d="categoryIcons[comp.category] || categoryIcons['Commands']" />
              </svg>
              <span class="text-xs font-medium text-text-secondary">{{ comp.category }}</span>
              <span class="text-[10px] text-text-muted">({{ comp.items.length }})</span>
            </div>
            <div class="flex flex-wrap gap-1">
              <span
                v-for="item in comp.items"
                :key="item"
                class="rounded bg-surface-2 px-1.5 py-0.5 font-mono text-[11px] text-text-secondary"
              >
                {{ item }}
              </span>
            </div>
          </div>
        </div>
      </section>

      <!-- Info -->
      <section class="mb-4 space-y-1">
        <div v-if="selectedPlugin.scope" class="text-xs text-text-muted">
          Scope: <span class="text-text-secondary">{{ selectedPlugin.scope }}</span>
        </div>
        <div v-if="selectedPlugin.installCount" class="text-xs text-text-muted">
          {{ selectedPlugin.installCount.toLocaleString() }} installs
        </div>
      </section>
    </div>
  </div>

  <div v-else-if="!selectedPluginId" class="flex h-full items-center justify-center text-text-muted">
    <div class="text-center">
      <p class="mb-1 text-sm">No plugin selected</p>
      <p class="text-xs">Select a plugin from the sidebar or browse to add one.</p>
    </div>
  </div>
</template>
