import { defineStore } from 'pinia';
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { MemoryItem, MemorySearchResult, SearchFilters } from '@/types/memory';

const PAGE_SIZE = 50;

export const useMemoriesStore = defineStore('memories', () => {
  const items = ref<MemoryItem[]>([]);
  const total = ref(0);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const query = ref('');
  const filters = ref<SearchFilters>({});
  const offset = ref(0);
  const hasMore = ref(false);
  const selectedMemory = ref<MemoryItem | null>(null);
  /** Set by Data Management after import/format to signal Browse should retry until results appear */
  const indexing = ref(false);

  function reset() {
    items.value = [];
    total.value = 0;
    offset.value = 0;
    hasMore.value = false;
    error.value = null;
  }

  async function search(append = false) {
    loading.value = true;
    error.value = null;

    try {
      const currentOffset = append ? offset.value : 0;
      const result = await invoke<MemorySearchResult>('search_memories', {
        text: query.value,
        limit: PAGE_SIZE,
        offset: currentOffset,
        memoryType: filters.value.memoryType ?? null,
        topics: filters.value.topics?.length ? filters.value.topics : null,
        entities: filters.value.entities?.length ? filters.value.entities : null,
        namespace: filters.value.namespace ?? null,
        userId: filters.value.userId ?? null,
        sessionId: filters.value.sessionId ?? null,
      });

      if (append) {
        items.value.push(...result.memories);
      } else {
        items.value = result.memories;
      }
      // Sort newest first — the API doesn't guarantee order
      items.value.sort((a, b) => new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime());
      total.value = result.total;
      hasMore.value = result.memories.length === PAGE_SIZE;
      offset.value = items.value.length;
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function loadMore() {
    if (!hasMore.value || loading.value) return;
    await search(true);
  }

  function addTopicFilter(topic: string) {
    const current = filters.value.topics ?? [];
    if (!current.includes(topic)) {
      filters.value = { ...filters.value, topics: [...current, topic] };
      search();
    }
  }

  function addEntityFilter(entity: string) {
    const current = filters.value.entities ?? [];
    if (!current.includes(entity)) {
      filters.value = { ...filters.value, entities: [...current, entity] };
      search();
    }
  }

  function clearFilters() {
    filters.value = {};
    search();
  }

  function toggleTypeFilter(type: string) {
    if (filters.value.memoryType === type) {
      const { memoryType: _, ...rest } = filters.value;
      filters.value = rest;
    } else {
      filters.value = { ...filters.value, memoryType: type };
    }
    search();
  }

  function removeTopic(topic: string) {
    const topics = (filters.value.topics ?? []).filter((t) => t !== topic);
    filters.value = { ...filters.value, topics: topics.length ? topics : undefined };
    search();
  }

  function removeEntity(entity: string) {
    const entities = (filters.value.entities ?? []).filter((e) => e !== entity);
    filters.value = { ...filters.value, entities: entities.length ? entities : undefined };
    search();
  }

  return {
    items, total, loading, error, query, filters, offset, hasMore, selectedMemory, indexing,
    reset, search, loadMore, addTopicFilter, addEntityFilter, clearFilters,
    toggleTypeFilter, removeTopic, removeEntity,
  };
});
