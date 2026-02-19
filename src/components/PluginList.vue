<script setup lang="ts">
import { useRouter } from 'vue-router';
import { usePluginsStore } from '@/stores/plugins';
import { useServersStore } from '@/stores/servers';
import { useSkillsStore } from '@/stores/skills';
import { storeToRefs } from 'pinia';

const router = useRouter();
const store = usePluginsStore();
const serversStore = useServersStore();
const skillsStore = useSkillsStore();
const { installedPlugins, selectedPluginId } = storeToRefs(store);

function onSelect(id: string) {
  store.selectPlugin(id);
  serversStore.selectedServerId = null;
  skillsStore.clearSelection();
  router.push('/plugins');
}
</script>

<template>
  <div>
    <div
      v-for="plugin in installedPlugins"
      :key="plugin.id"
      class="flex cursor-pointer items-center gap-2 border-b border-border/50 px-3 py-2 transition-colors hover:bg-surface-2"
      :class="selectedPluginId === plugin.id ? 'bg-surface-2' : ''"
      @click="onSelect(plugin.id)"
    >
      <span
        class="h-1.5 w-1.5 shrink-0 rounded-full"
        :class="plugin.enabled ? 'bg-status-connected' : 'bg-surface-3'"
      />
      <span class="truncate text-xs" :class="plugin.enabled ? '' : 'text-text-muted'">{{ plugin.name }}</span>
    </div>
    <div
      v-if="installedPlugins.length === 0"
      class="px-3 py-6 text-center text-xs text-text-muted"
    >
      No plugins installed
    </div>
  </div>
</template>
