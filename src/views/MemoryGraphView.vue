<script setup lang="ts">
import { onMounted, onUnmounted, reactive, ref, computed } from 'vue';
import { ForceLayout } from 'v-network-graph/lib/force-layout';
import type { EventHandlers } from 'v-network-graph';
import { useMemoriesStore } from '@/stores/memories';
import { useMemoryGraph } from '@/composables/useMemoryGraph';
import type { MemoryItem } from '@/types/memory';

const store = useMemoriesStore();
const { nodes, edges, nodeColor, TYPE_COLORS } = useMemoryGraph();

const selectedNodeId = ref<string | null>(null);
const selectedMemory = computed<MemoryItem | null>(() => {
  if (!selectedNodeId.value) return null;
  return store.items.find((m) => m.id === selectedNodeId.value) ?? null;
});

const configs = reactive({
  view: {
    autoPanAndZoomOnLoad: 'fit-content' as const,
    layoutHandler: new ForceLayout({
      positionFixedByDrag: true,
      positionFixedByClickWithAltKey: true,
    }),
  },
  node: {
    normal: {
      radius: 16,
      color: (node: string) => nodeColor(node),
    },
    hover: {
      radius: 18,
      color: (node: string) => nodeColor(node),
    },
    label: {
      fontSize: 10,
      color: '#a1a1aa',
    },
    selectable: true,
  },
  edge: {
    normal: {
      color: '#27272a',
      width: (edge: string) => {
        const e = edges.value[edge];
        return e ? Math.min(e.weight * 1.5, 8) : 1;
      },
    },
  },
});

const eventHandlers: EventHandlers = {
  'node:click': ({ node }) => {
    selectedNodeId.value = node;
  },
  'view:click': () => {
    selectedNodeId.value = null;
  },
};

const selectedNodes = computed({
  get: () => (selectedNodeId.value ? [selectedNodeId.value] : []),
  set: (v: string[]) => {
    selectedNodeId.value = v[0] ?? null;
  },
});

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

let pollTimer: ReturnType<typeof setInterval>;

onMounted(() => {
  if (!store.items.length) store.search();
  pollTimer = setInterval(() => {
    if (!store.loading) store.search();
  }, 10000);
});

onUnmounted(() => {
  clearInterval(pollTimer);
});
</script>

<template>
  <div class="flex h-full">
    <div class="flex min-w-0 flex-1 flex-col">
      <!-- Toolbar -->
      <div class="flex items-center gap-4 border-b border-border px-4 py-2">
        <span class="text-[11px] text-text-muted">
          {{ Object.keys(nodes).length }} nodes, {{ Object.keys(edges).length }} edges
        </span>
        <div class="ml-auto flex items-center gap-3">
          <div
            v-for="(color, type) in TYPE_COLORS"
            :key="type"
            class="flex items-center gap-1.5"
          >
            <span class="h-2.5 w-2.5 rounded-full" :style="{ backgroundColor: color }" />
            <span class="text-[10px] text-text-muted capitalize">{{ type }}</span>
          </div>
        </div>
      </div>

      <!-- Graph area -->
      <div v-if="store.loading && !store.items.length" class="flex flex-1 items-center justify-center text-xs text-text-muted">
        Loading...
      </div>
      <div v-else-if="store.error" class="flex flex-1 items-center justify-center text-xs text-status-error">
        {{ store.error }}
      </div>
      <div v-else-if="!store.items.length" class="flex flex-1 items-center justify-center text-xs text-text-muted">
        No memories found.
      </div>
      <v-network-graph
        v-else
        class="flex-1"
        :nodes="nodes"
        :edges="edges"
        :configs="configs"
        :selected-nodes="selectedNodes"
        :event-handlers="eventHandlers"
      />
    </div>

    <!-- Detail sidebar -->
    <div v-if="selectedMemory" class="w-72 border-l border-border overflow-y-auto bg-surface-1">
      <div class="p-4 space-y-3">
        <div class="flex items-start justify-between">
          <span class="rounded-full bg-accent px-2 py-0.5 text-[10px] font-medium text-white">
            {{ selectedMemory.memoryType }}
          </span>
          <button
            class="text-[11px] text-text-muted hover:text-text-secondary"
            @click="selectedNodeId = null"
          >
            Close
          </button>
        </div>

        <p class="text-xs leading-relaxed text-text-primary whitespace-pre-wrap">
          {{ selectedMemory.text }}
        </p>

        <div v-if="selectedMemory.topics.length" class="space-y-1">
          <span class="text-[10px] font-medium text-text-muted">Topics</span>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="t in selectedMemory.topics"
              :key="t"
              class="rounded-full border border-border px-1.5 text-[10px] text-text-secondary"
            >{{ t }}</span>
          </div>
        </div>

        <div v-if="selectedMemory.entities.length" class="space-y-1">
          <span class="text-[10px] font-medium text-text-muted">Entities</span>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="e in selectedMemory.entities"
              :key="e"
              class="rounded-full border border-accent/30 px-1.5 text-[10px] text-accent"
            >{{ e }}</span>
          </div>
        </div>

        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
          <dt class="text-text-muted">Created</dt>
          <dd class="text-text-secondary">{{ formatDate(selectedMemory.createdAt) }}</dd>
          <dt v-if="selectedMemory.namespace" class="text-text-muted">Namespace</dt>
          <dd v-if="selectedMemory.namespace" class="text-text-secondary">{{ selectedMemory.namespace }}</dd>
        </dl>
      </div>
    </div>
  </div>
</template>
