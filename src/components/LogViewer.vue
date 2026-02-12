<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { useLogsStore, type LogEntry } from '@/stores/logs';

const props = defineProps<{
  serverId?: string;
}>();

const logsStore = useLogsStore();
const levelFilter = ref<LogEntry['level'] | 'all'>('all');
const scrollContainer = ref<HTMLElement | null>(null);
const autoScroll = ref(true);

const filteredLogs = computed(() => {
  let result = props.serverId
    ? logsStore.logsForServer(props.serverId)
    : logsStore.logs;

  if (levelFilter.value !== 'all') {
    result = result.filter((l) => l.level === levelFilter.value);
  }

  return result;
});

function levelClass(level: LogEntry['level']): string {
  switch (level) {
    case 'error': return 'text-status-error';
    case 'debug': return 'text-text-muted';
    default: return 'text-status-connected';
  }
}

function levelLabel(level: LogEntry['level']): string {
  return level.toUpperCase().padEnd(5);
}

function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function handleScroll() {
  if (!scrollContainer.value) return;
  const el = scrollContainer.value;
  const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
  autoScroll.value = atBottom;
}

watch(
  () => filteredLogs.value.length,
  async () => {
    if (!autoScroll.value) return;
    await nextTick();
    if (scrollContainer.value) {
      scrollContainer.value.scrollTop = scrollContainer.value.scrollHeight;
    }
  }
);
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Toolbar -->
    <div class="flex items-center gap-2 border-b border-border px-3 py-1.5">
      <span class="font-mono text-[11px] text-text-muted">Filter:</span>
      <button
        v-for="level in (['all', 'info', 'error', 'debug'] as const)"
        :key="level"
        class="rounded px-2 py-0.5 font-mono text-[11px] transition-colors"
        :class="levelFilter === level
          ? 'bg-surface-3 text-text-primary'
          : 'text-text-muted hover:text-text-secondary'"
        @click="levelFilter = level"
      >
        {{ level }}
      </button>
      <button
        class="ml-auto text-[11px] text-text-muted transition-colors hover:text-text-secondary"
        @click="logsStore.clearLogs(serverId)"
      >
        Clear
      </button>
    </div>

    <!-- Log output -->
    <div
      ref="scrollContainer"
      class="flex-1 overflow-y-auto bg-surface-0 p-2 font-mono text-[11px] leading-relaxed"
      @scroll="handleScroll"
    >
      <div v-if="filteredLogs.length === 0" class="py-6 text-center text-text-muted">
        No log entries
      </div>
      <div
        v-for="(log, i) in filteredLogs"
        :key="i"
        class="flex gap-2 hover:bg-surface-1"
      >
        <span class="shrink-0 text-text-muted">{{ formatTimestamp(log.timestamp) }}</span>
        <span class="shrink-0" :class="levelClass(log.level)">{{ levelLabel(log.level) }}</span>
        <span class="min-w-0 break-all text-text-secondary">{{ log.message }}</span>
      </div>
    </div>
  </div>
</template>
