<script setup lang="ts">
import { ref, computed } from 'vue';
import { useServersStore } from '@/stores/servers';
import { storeToRefs } from 'pinia';
import ToolBrowser from '@/components/ToolBrowser.vue';
import LogViewer from '@/components/LogViewer.vue';

const store = useServersStore();
const { servers, selectedServerId } = storeToRefs(store);

const selectedServer = computed(() =>
  servers.value.find((s) => s.id === selectedServerId.value)
);

type Tab = 'overview' | 'tools' | 'logs';
const activeTab = ref<Tab>('overview');

function statusColor(status: string | undefined): string {
  switch (status) {
    case 'connected': return 'bg-status-connected';
    case 'connecting': return 'bg-status-connecting';
    case 'error': return 'bg-status-error';
    default: return 'bg-status-disconnected';
  }
}

function statusLabel(status: string | undefined): string {
  return status ?? 'disconnected';
}

function formatDate(iso: string | undefined): string {
  if (!iso) return 'Never';
  return new Date(iso).toLocaleString();
}
</script>

<template>
  <div v-if="selectedServer" class="flex h-full flex-col">
    <!-- Header -->
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <span
        class="h-2 w-2 rounded-full"
        :class="statusColor(selectedServer.status)"
      />
      <h1 class="text-sm font-medium">{{ selectedServer.name }}</h1>
      <span class="font-mono text-xs text-text-muted">{{ selectedServer.transport }}</span>
      <div class="ml-auto flex gap-2">
        <button
          v-if="selectedServer.status !== 'connected'"
          class="rounded bg-accent px-3 py-1 text-xs text-white transition-colors hover:bg-accent-hover"
          @click="store.connectServer(selectedServer.id)"
        >
          Connect
        </button>
        <button
          v-else
          class="rounded bg-surface-3 px-3 py-1 text-xs text-text-secondary transition-colors hover:bg-surface-2"
          @click="store.disconnectServer(selectedServer.id)"
        >
          Disconnect
        </button>
      </div>
    </header>

    <!-- Tabs -->
    <div class="flex border-b border-border">
      <button
        v-for="tab in (['overview', 'tools', 'logs'] as const)"
        :key="tab"
        class="border-b-2 px-4 py-2 text-xs transition-colors"
        :class="activeTab === tab
          ? 'border-accent text-text-primary'
          : 'border-transparent text-text-muted hover:text-text-secondary'"
        @click="activeTab = tab"
      >
        {{ tab.charAt(0).toUpperCase() + tab.slice(1) }}
      </button>
    </div>

    <!-- Tab content -->
    <div class="min-h-0 flex-1">
      <!-- Overview -->
      <div v-if="activeTab === 'overview'" class="h-full overflow-y-auto p-4">
        <section class="mb-6">
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Status</h2>
          <div class="rounded border border-border bg-surface-1 p-3">
            <div class="flex items-center gap-2">
              <span class="h-1.5 w-1.5 rounded-full" :class="statusColor(selectedServer.status)" />
              <span class="font-mono text-xs text-text-secondary capitalize">{{ statusLabel(selectedServer.status) }}</span>
            </div>
            <div class="mt-2 text-[11px] text-text-muted">
              Last connected: {{ formatDate(selectedServer.lastConnected) }}
            </div>
          </div>
        </section>

        <section class="mb-6">
          <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Configuration</h2>
          <div class="rounded border border-border bg-surface-1 p-3 font-mono text-xs text-text-secondary">
            <div v-if="selectedServer.command">
              <span class="text-text-muted">command:</span> {{ selectedServer.command }}
            </div>
            <div v-if="selectedServer.args?.length">
              <span class="text-text-muted">args:</span> {{ selectedServer.args.join(' ') }}
            </div>
            <div v-if="selectedServer.url">
              <span class="text-text-muted">url:</span> {{ selectedServer.url }}
            </div>
            <div v-if="selectedServer.env && Object.keys(selectedServer.env).length">
              <span class="text-text-muted">env:</span>
              <div v-for="(val, key) in selectedServer.env" :key="key" class="ml-3">
                {{ key }}={{ val }}
              </div>
            </div>
            <div v-if="selectedServer.tags?.length" class="mt-2">
              <span class="text-text-muted">tags:</span>
              <span
                v-for="tag in selectedServer.tags"
                :key="tag"
                class="ml-1 rounded bg-surface-3 px-1.5 py-0.5 text-[11px] text-text-secondary"
              >
                {{ tag }}
              </span>
            </div>
          </div>
        </section>
      </div>

      <!-- Tools -->
      <ToolBrowser v-if="activeTab === 'tools'" :server-id="selectedServer.id" />

      <!-- Logs -->
      <LogViewer v-if="activeTab === 'logs'" :server-id="selectedServer.id" />
    </div>
  </div>

  <div v-else class="flex h-full items-center justify-center text-text-muted">
    <div class="text-center">
      <p class="mb-1 text-sm">No server selected</p>
      <p class="text-xs">Select a server from the sidebar or <router-link to="/add" class="text-accent hover:text-accent-hover">add one</router-link>.</p>
    </div>
  </div>
</template>
