<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { SkillToolInfo } from '@/types/skill';

const integrations = ref<SkillToolInfo[] | null>(null);
const error = ref<string | null>(null);
const togglingId = ref<string | null>(null);

const installedTools = computed(() =>
  integrations.value?.filter(t => t.installed) ?? []
);

const notInstalledTools = computed(() =>
  integrations.value?.filter(t => !t.installed) ?? []
);

async function fetchIntegrations() {
  try {
    integrations.value = await invoke<SkillToolInfo[]>('detect_skill_integrations');
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}

async function toggle(tool: SkillToolInfo) {
  togglingId.value = tool.id;
  try {
    if (tool.enabled) {
      await invoke('disable_skill_integration', { id: tool.id });
    } else {
      await invoke('enable_skill_integration', { id: tool.id });
    }
    await fetchIntegrations();
  } catch (e) {
    error.value = String(e);
  } finally {
    togglingId.value = null;
  }
}

onMounted(() => {
  fetchIntegrations();
});
</script>

<template>
  <div>
    <h2 class="mb-1 text-xs font-medium text-text-primary">Managed Skills</h2>
    <p class="mb-4 text-xs text-text-secondary">
      Choose which AI tools receive SKILL.md files when you install skills from the marketplace. Skills are independent of MCP server configs.
    </p>

    <div v-if="error" class="mb-3 rounded bg-status-error/10 px-3 py-2 text-xs text-status-error">
      {{ error }}
    </div>

    <div v-if="!integrations" class="text-xs text-text-muted">Detecting tools...</div>

    <template v-if="integrations">
      <!-- Installed tools -->
      <div v-if="installedTools.length" class="space-y-2">
        <div
          v-for="tool in installedTools"
          :key="tool.id"
          class="rounded border border-border bg-surface-1"
        >
          <div class="flex items-center justify-between px-3 py-2.5">
            <div class="min-w-0">
              <div class="text-xs font-medium text-text-primary">{{ tool.name }}</div>
              <div class="mt-0.5 truncate text-[10px] text-text-muted">{{ tool.skillsPath }}</div>
            </div>
            <div class="ml-3 flex shrink-0 items-center gap-2">
              <span
                v-if="tool.enabled"
                class="inline-flex items-center gap-1 rounded bg-status-connected/10 px-2 py-1 text-[11px] font-medium text-status-connected"
              >
                <span class="h-1.5 w-1.5 rounded-full bg-status-connected" />
                Managed
              </span>
              <button
                class="rounded px-3 py-1 text-[11px] transition-colors disabled:opacity-50"
                :class="tool.enabled
                  ? 'bg-surface-3 text-text-secondary hover:bg-surface-2'
                  : 'bg-accent text-white hover:bg-accent-hover font-medium'"
                :disabled="togglingId === tool.id"
                @click="toggle(tool)"
              >
                {{ togglingId === tool.id ? '...' : tool.enabled ? 'Disable' : 'Enable' }}
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Not installed tools (grayed out) -->
      <div v-if="notInstalledTools.length" class="mt-4">
        <h3 class="mb-2 font-mono text-[10px] font-medium tracking-wide text-text-muted uppercase">
          Not Detected
        </h3>
        <div class="space-y-2">
          <div
            v-for="tool in notInstalledTools"
            :key="tool.id"
            class="rounded border border-border/50 bg-surface-1 opacity-50"
          >
            <div class="flex items-center justify-between px-3 py-2.5">
              <div class="min-w-0">
                <div class="text-xs text-text-muted">{{ tool.name }}</div>
                <div class="mt-0.5 truncate text-[10px] text-text-muted">{{ tool.skillsPath }}</div>
              </div>
              <span class="text-[10px] text-text-muted">Not installed</span>
            </div>
          </div>
        </div>
      </div>

      <div v-if="!installedTools.length && !notInstalledTools.length" class="text-xs text-text-muted">
        No supported AI tools detected.
      </div>
    </template>
  </div>
</template>
