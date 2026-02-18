<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { DiscoveryStatus } from '@/types/discovery';
import ToggleCard from './ToggleCard.vue';

const status = ref<DiscoveryStatus | null>(null);
const toggling = ref(false);
const error = ref<string | null>(null);

async function load() {
  try {
    status.value = await invoke<DiscoveryStatus>('get_discovery_mode');
  } catch (e) {
    error.value = String(e);
  }
}

async function toggle() {
  if (!status.value) return;
  toggling.value = true;
  error.value = null;
  try {
    status.value = await invoke<DiscoveryStatus>('set_discovery_mode', {
      enabled: !status.value.enabled,
    });
  } catch (e) {
    error.value = String(e);
  } finally {
    toggling.value = false;
  }
}

onMounted(load);
</script>

<template>
  <div class="space-y-3">
    <div>
      <h2 class="text-xs font-medium text-text-primary">Tool Discovery Mode</h2>
      <p class="mt-1 text-[11px] text-text-muted leading-relaxed">
        Instead of exposing every tool from every server in your AI tool's context,
        discovery mode provides a single endpoint with 3 meta-tools:
        <strong>discover_tools</strong> (search for tools by keyword),
        <strong>call_tool</strong> (invoke a tool on a specific server), and
        <strong>list_servers</strong> (see connected servers).
        This dramatically reduces prompt bloat and avoids tool count limits.
      </p>
    </div>

    <ToggleCard
      v-if="status"
      label="Discovery Mode"
      :enabled="status.enabled"
      :toggling="toggling"
      :can-enable="true"
      enable-label="Enable"
      disable-label="Disable"
      @toggle="toggle"
    >
      <div v-if="status.enabled" class="border-t border-border px-3 py-2">
        <p class="text-[10px] text-text-muted">
          All managed config files now point to a single discovery endpoint.
          Your AI tool will use <code class="text-text-secondary">discover_tools</code> to find and call tools on-demand.
        </p>
      </div>
    </ToggleCard>

    <div v-if="error" class="rounded border border-status-error/30 bg-status-error/5 px-3 py-2">
      <p class="text-[11px] text-status-error">{{ error }}</p>
    </div>

    <div class="rounded border border-amber-500/20 bg-amber-500/5 px-3 py-2">
      <p class="text-[10px] text-amber-400">
        Experimental — toggling this immediately rewrites all managed integration config files.
      </p>
    </div>
  </div>
</template>
