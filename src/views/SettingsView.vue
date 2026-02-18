<script setup lang="ts">
import { ref } from 'vue';
import SettingsIntegrations from '@/components/SettingsIntegrations.vue';
import SettingsDiscovery from '@/components/SettingsDiscovery.vue';
import SettingsMemory from '@/components/SettingsMemory.vue';
import SettingsProxy from '@/components/SettingsProxy.vue';

type Section = 'integrations' | 'discovery' | 'memory' | 'proxy';

const sections: { id: Section; label: string }[] = [
  { id: 'integrations', label: 'MCP Configs' },
  { id: 'discovery', label: 'Discovery' },
  { id: 'memory', label: 'Memory' },
  { id: 'proxy', label: 'Proxy' },
];

const active = ref<Section>('integrations');
</script>

<template>
  <div class="flex h-full flex-col">
    <header class="border-b border-border px-4 py-3">
      <h1 class="text-sm font-medium">Settings</h1>
    </header>
    <div class="flex min-h-0 flex-1">
      <!-- Sidebar nav -->
      <nav class="w-40 shrink-0 border-r border-border py-2">
        <button
          v-for="section in sections"
          :key="section.id"
          class="block w-full px-4 py-1.5 text-left text-xs transition-colors"
          :class="active === section.id
            ? 'text-text-primary bg-surface-1 font-medium'
            : 'text-text-muted hover:text-text-secondary'"
          @click="active = section.id"
        >
          {{ section.label }}
        </button>
      </nav>

      <!-- Content -->
      <div class="flex-1 overflow-y-auto p-5">
        <SettingsIntegrations v-if="active === 'integrations'" />
        <SettingsDiscovery v-if="active === 'discovery'" />
        <SettingsMemory v-if="active === 'memory'" />
        <SettingsProxy v-if="active === 'proxy'" />
      </div>
    </div>
  </div>
</template>
