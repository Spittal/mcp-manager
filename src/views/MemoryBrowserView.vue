<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue';
import { useMemoriesStore } from '@/stores/memories';
import { useMemorySearch } from '@/composables/useMemorySearch';
import type { MemoryItem } from '@/types/memory';

const store = useMemoriesStore();
const { search, addTopicFilter, addEntityFilter, clearFilters } = useMemorySearch();

let debounceTimer: ReturnType<typeof setTimeout>;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
const scrollContainer = ref<HTMLElement | null>(null);

async function searchAndCheckIndexing() {
  await search();

  // If the store has the indexing flag (set by Data Management after import/format),
  // keep retrying until results appear, then clear the flag
  if (store.indexing) {
    if (store.items.length > 0) {
      store.indexing = false;
      clearTimeout(retryTimer);
    } else {
      retryTimer = setTimeout(() => searchAndCheckIndexing(), 3000);
    }
  }
}

function onScroll(e: Event) {
  if (!store.hasMore || store.loading) return;
  const el = e.target as HTMLElement;
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 100) {
    search(true);
  }
}

function onQueryInput() {
  clearTimeout(debounceTimer);
  clearTimeout(retryTimer);
  store.indexing = false;
  debounceTimer = setTimeout(() => search(), 300);
}

function selectMemory(m: MemoryItem) {
  store.selectedMemory = m;
}

function closeDetail() {
  store.selectedMemory = null;
}

function toggleType(type: string) {
  if (store.filters.memoryType === type) {
    const { memoryType: _, ...rest } = store.filters;
    store.filters = rest;
  } else {
    store.filters = { ...store.filters, memoryType: type };
  }
}

function removeTopic(topic: string) {
  const topics = (store.filters.topics ?? []).filter((t) => t !== topic);
  store.filters = { ...store.filters, topics: topics.length ? topics : undefined };
}

function removeEntity(entity: string) {
  const entities = (store.filters.entities ?? []).filter((e) => e !== entity);
  store.filters = { ...store.filters, entities: entities.length ? entities : undefined };
}

function formatRelativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString();
}

function copyJson() {
  if (store.selectedMemory) {
    navigator.clipboard.writeText(JSON.stringify(store.selectedMemory, null, 2));
  }
}

const memoryTypes = ['semantic', 'episodic', 'message'] as const;

const typeColors: Record<string, string> = {
  semantic: 'bg-accent text-white',
  episodic: 'bg-status-connecting text-surface-0',
  message: 'bg-status-connected text-surface-0',
};

watch(() => store.filters, () => {
  clearTimeout(retryTimer);
  store.indexing = false;
  search();
}, { deep: true });

onMounted(() => {
  searchAndCheckIndexing();
});

onUnmounted(() => {
  clearTimeout(debounceTimer);
  clearTimeout(retryTimer);
});
</script>

<template>
  <div class="flex h-full">
    <!-- List panel -->
    <div class="flex min-w-0 flex-1 flex-col">
      <header class="border-b border-border px-4 py-3">
        <h1 class="text-sm font-medium">Memories</h1>
      </header>

      <!-- Search + Filters -->
      <div class="border-b border-border p-3 space-y-2">
        <input
          :value="store.query"
          class="w-full rounded border border-border bg-surface-2 px-3 py-1.5 text-xs text-text-primary placeholder-text-muted outline-none focus:border-accent"
          placeholder="Search memories..."
          @input="(e) => { store.query = (e.target as HTMLInputElement).value; onQueryInput(); }"
        />
        <!-- Type filters -->
        <div class="flex flex-wrap gap-1">
          <button
            v-for="type in memoryTypes"
            :key="type"
            class="rounded-full border px-2.5 py-0.5 text-[11px] font-medium transition-colors"
            :class="store.filters.memoryType === type
              ? typeColors[type]
              : 'border-border text-text-muted hover:border-border-active hover:text-text-secondary'"
            @click="toggleType(type)"
          >
            {{ type }}
          </button>
        </div>
        <!-- Active topic/entity filters -->
        <div v-if="store.filters.topics?.length" class="flex flex-wrap gap-1">
          <button
            v-for="t in store.filters.topics"
            :key="t"
            class="rounded-full border border-border px-2 py-0.5 text-[10px] text-text-secondary hover:border-status-error hover:text-status-error"
            @click="removeTopic(t)"
          >
            {{ t }} &times;
          </button>
        </div>
        <div v-if="store.filters.entities?.length" class="flex flex-wrap gap-1">
          <button
            v-for="e in store.filters.entities"
            :key="e"
            class="rounded-full border border-accent/40 px-2 py-0.5 text-[10px] text-accent hover:border-status-error hover:text-status-error"
            @click="removeEntity(e)"
          >
            {{ e }} &times;
          </button>
        </div>
        <button
          v-if="store.filters.memoryType || store.filters.topics?.length || store.filters.entities?.length"
          class="text-[11px] text-text-muted hover:text-text-secondary underline"
          @click="clearFilters"
        >
          Clear filters
        </button>
      </div>

      <!-- Results -->
      <div ref="scrollContainer" class="flex-1 overflow-y-auto" @scroll="onScroll">
        <div v-if="store.indexing" class="py-12 text-center space-y-2">
          <span class="mx-auto block h-1.5 w-1.5 animate-pulse rounded-full bg-accent" />
          <p class="text-xs text-text-muted">Processing memories...</p>
          <p class="text-[10px] text-text-muted">The memory server is indexing. This may take a moment.</p>
        </div>
        <div v-else-if="store.loading && !store.items.length" class="py-12 text-center text-xs text-text-muted">
          Loading...
        </div>
        <div v-else-if="store.error" class="p-4 text-xs text-status-error">
          {{ store.error }}
        </div>
        <div v-else-if="!store.items.length" class="py-12 text-center text-xs text-text-muted">
          No memories found.
        </div>
        <div v-else class="p-3 space-y-1.5">
          <div class="text-[10px] text-text-muted mb-1">showing {{ store.items.length }}{{ store.hasMore ? '+' : '' }} memories</div>
          <!-- Memory cards -->
          <button
            v-for="m in store.items"
            :key="m.id"
            class="w-full rounded border border-border bg-surface-1 px-3 py-2.5 text-left transition-colors hover:bg-surface-2"
            :class="store.selectedMemory?.id === m.id ? 'border-accent' : ''"
            @click="selectMemory(m)"
          >
            <div class="flex items-start justify-between gap-2">
              <p class="text-xs text-text-primary line-clamp-3">{{ m.text }}</p>
              <span
                class="shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium"
                :class="typeColors[m.memoryType] ?? 'bg-surface-3 text-text-muted'"
              >
                {{ m.memoryType }}
              </span>
            </div>
            <div v-if="m.topics.length || m.entities.length" class="mt-1.5 flex flex-wrap gap-1">
              <span
                v-for="t in m.topics"
                :key="'t-' + t"
                class="rounded-full border border-border px-1.5 py-0 text-[10px] text-text-muted cursor-pointer hover:border-accent hover:text-accent"
                @click.stop="addTopicFilter(t)"
              >
                {{ t }}
              </span>
              <span
                v-for="e in m.entities"
                :key="'e-' + e"
                class="rounded-full border border-accent/30 px-1.5 py-0 text-[10px] text-accent/70 cursor-pointer hover:border-accent hover:text-accent"
                @click.stop="addEntityFilter(e)"
              >
                {{ e }}
              </span>
            </div>
            <div class="mt-1.5 flex items-center gap-2 text-[10px] text-text-muted">
              <span :title="m.createdAt">{{ formatRelativeTime(m.createdAt) }}</span>
              <span v-if="m.namespace">{{ m.namespace }}</span>
            </div>
          </button>

          <!-- Infinite scroll loading indicator -->
          <div
            v-if="store.loading && store.items.length"
            class="py-3 text-center text-[11px] text-text-muted"
          >
            Loading...
          </div>
        </div>
      </div>
    </div>

    <!-- Detail panel -->
    <div v-if="store.selectedMemory" class="w-80 border-l border-border overflow-y-auto bg-surface-1">
      <div class="p-4 space-y-4">
        <div class="flex items-start justify-between">
          <span
            class="rounded-full px-2 py-0.5 text-[10px] font-medium"
            :class="typeColors[store.selectedMemory.memoryType] ?? 'bg-surface-3 text-text-muted'"
          >
            {{ store.selectedMemory.memoryType }}
          </span>
          <button
            class="text-[11px] text-text-muted hover:text-text-secondary"
            @click="closeDetail"
          >
            Close
          </button>
        </div>

        <p class="text-xs leading-relaxed text-text-primary whitespace-pre-wrap">
          {{ store.selectedMemory.text }}
        </p>

        <div class="border-t border-border" />

        <div v-if="store.selectedMemory.topics.length" class="space-y-1">
          <span class="text-[10px] font-medium text-text-muted uppercase tracking-wide">Topics</span>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="t in store.selectedMemory.topics"
              :key="t"
              class="rounded-full border border-border px-1.5 py-0 text-[10px] text-text-secondary"
            >
              {{ t }}
            </span>
          </div>
        </div>

        <div v-if="store.selectedMemory.entities.length" class="space-y-1">
          <span class="text-[10px] font-medium text-text-muted uppercase tracking-wide">Entities</span>
          <div class="flex flex-wrap gap-1">
            <span
              v-for="e in store.selectedMemory.entities"
              :key="e"
              class="rounded-full border border-accent/30 px-1.5 py-0 text-[10px] text-accent"
            >
              {{ e }}
            </span>
          </div>
        </div>

        <div class="border-t border-border" />

        <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11px]">
          <dt class="text-text-muted">ID</dt>
          <dd class="font-mono text-text-secondary select-all truncate">{{ store.selectedMemory.id }}</dd>
          <dt class="text-text-muted">Created</dt>
          <dd class="text-text-secondary">{{ formatDate(store.selectedMemory.createdAt) }}</dd>
          <dt class="text-text-muted">Updated</dt>
          <dd class="text-text-secondary">{{ formatDate(store.selectedMemory.updatedAt) }}</dd>
          <template v-if="store.selectedMemory.eventDate">
            <dt class="text-text-muted">Event</dt>
            <dd class="text-text-secondary">{{ formatDate(store.selectedMemory.eventDate) }}</dd>
          </template>
          <template v-if="store.selectedMemory.namespace">
            <dt class="text-text-muted">Namespace</dt>
            <dd class="text-text-secondary">{{ store.selectedMemory.namespace }}</dd>
          </template>
          <template v-if="store.selectedMemory.userId">
            <dt class="text-text-muted">User</dt>
            <dd class="text-text-secondary">{{ store.selectedMemory.userId }}</dd>
          </template>
          <template v-if="store.selectedMemory.sessionId">
            <dt class="text-text-muted">Session</dt>
            <dd class="font-mono text-text-secondary truncate">{{ store.selectedMemory.sessionId }}</dd>
          </template>
          <dt class="text-text-muted">Pinned</dt>
          <dd class="text-text-secondary">{{ store.selectedMemory.pinned ? 'Yes' : 'No' }}</dd>
          <template v-if="store.selectedMemory.distance != null">
            <dt class="text-text-muted">Distance</dt>
            <dd class="text-text-secondary">{{ store.selectedMemory.distance!.toFixed(4) }}</dd>
          </template>
        </dl>

        <button
          class="rounded border border-border bg-surface-2 px-3 py-1 text-[11px] text-text-muted transition-colors hover:bg-surface-3 hover:text-text-secondary"
          @click="copyJson"
        >
          Copy JSON
        </button>
      </div>
    </div>
  </div>
</template>
