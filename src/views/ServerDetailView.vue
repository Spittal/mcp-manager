<script setup lang="ts">
import { useServersStore } from '@/stores/servers';
import { storeToRefs } from 'pinia';
import { computed } from 'vue';

const store = useServersStore();
const { servers, selectedServerId } = storeToRefs(store);

const selectedServer = computed(() =>
  servers.value.find(s => s.id === selectedServerId.value)
);
</script>

<template>
  <div v-if="selectedServer" class="flex h-full flex-col">
    <header class="flex items-center gap-3 border-b border-border px-4 py-3">
      <span
        class="h-2 w-2 rounded-full"
        :class="{
          'bg-status-connected': selectedServer.status === 'connected',
          'bg-status-connecting': selectedServer.status === 'connecting',
          'bg-status-error': selectedServer.status === 'error',
          'bg-status-disconnected': !selectedServer.status || selectedServer.status === 'disconnected',
        }"
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

    <div class="flex-1 overflow-y-auto p-4">
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
        </div>
      </section>

      <section>
        <h2 class="mb-2 font-mono text-xs font-medium tracking-wide text-text-muted uppercase">Tools</h2>
        <p class="text-xs text-text-muted">Connect to discover available tools.</p>
      </section>
    </div>
  </div>

  <div v-else class="flex h-full items-center justify-center text-text-muted">
    <div class="text-center">
      <p class="mb-1 text-sm">No server selected</p>
      <p class="text-xs">Select a server from the sidebar or <router-link to="/add" class="text-accent hover:text-accent-hover">add one</router-link>.</p>
    </div>
  </div>
</template>
