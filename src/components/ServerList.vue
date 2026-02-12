<script setup lang="ts">
import { useServersStore } from '@/stores/servers';
import { storeToRefs } from 'pinia';

const store = useServersStore();
const { servers, selectedServerId } = storeToRefs(store);

function statusColor(status: string): string {
  switch (status) {
    case 'connected': return 'bg-status-connected';
    case 'connecting': return 'bg-status-connecting';
    case 'error': return 'bg-status-error';
    default: return 'bg-status-disconnected';
  }
}
</script>

<template>
  <div class="flex-1 overflow-y-auto">
    <div
      v-for="server in servers"
      :key="server.id"
      class="group flex cursor-pointer items-center gap-2 border-b border-border/50 px-3 py-2 transition-colors hover:bg-surface-2"
      :class="{ 'bg-surface-2': selectedServerId === server.id }"
      @click="store.selectServer(server.id)"
    >
      <span
        class="h-1.5 w-1.5 shrink-0 rounded-full"
        :class="statusColor(server.status ?? 'disconnected')"
      />
      <span class="truncate text-xs">{{ server.name }}</span>
    </div>
    <div
      v-if="servers.length === 0"
      class="px-3 py-6 text-center text-xs text-text-muted"
    >
      No servers configured
    </div>
  </div>
</template>
