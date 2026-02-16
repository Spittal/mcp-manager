import { invoke } from '@tauri-apps/api/core';
import { useMemoriesStore } from '@/stores/memories';
import type { MemorySearchResult } from '@/types/memory';

export function useMemorySearch() {
  const store = useMemoriesStore();

  async function search(append = false) {
    store.loading = true;
    store.error = null;

    try {
      const result = await invoke<MemorySearchResult>('search_memories', {
        text: store.query,
        limit: 20,
        offset: append ? store.offset : 0,
        memoryType: store.filters.memoryType ?? null,
        topics: store.filters.topics?.length ? store.filters.topics : null,
        entities: store.filters.entities?.length ? store.filters.entities : null,
        namespace: store.filters.namespace ?? null,
        userId: store.filters.userId ?? null,
        sessionId: store.filters.sessionId ?? null,
      });

      if (append) {
        store.items.push(...result.memories);
      } else {
        store.items = result.memories;
      }
      store.total = result.total;
      store.offset = result.nextOffset ?? store.items.length;
    } catch (e) {
      store.error = String(e);
    } finally {
      store.loading = false;
    }
  }

  function addTopicFilter(topic: string) {
    const current = store.filters.topics ?? [];
    if (!current.includes(topic)) {
      store.filters = { ...store.filters, topics: [...current, topic] };
      search();
    }
  }

  function addEntityFilter(entity: string) {
    const current = store.filters.entities ?? [];
    if (!current.includes(entity)) {
      store.filters = { ...store.filters, entities: [...current, entity] };
      search();
    }
  }

  function clearFilters() {
    store.filters = {};
    search();
  }

  return { search, addTopicFilter, addEntityFilter, clearFilters };
}
