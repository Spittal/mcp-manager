<script setup lang="ts">
import { useServersStore } from '@/stores/servers';
import { computed } from 'vue';

const store = useServersStore();
const connectedCount = computed(() =>
  store.servers.filter(s => s.status === 'connected').length
);
</script>

<template>
  <footer class="flex items-center justify-between border-t border-border bg-surface-1 px-3 py-1 text-[11px] text-text-muted">
    <span>{{ connectedCount }} connected</span>
    <span class="font-mono">MCP Manager</span>
  </footer>
</template>
