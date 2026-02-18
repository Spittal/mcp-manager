import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { MemoryItem, SearchFilters } from '@/types/memory';

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

  return { items, total, loading, error, query, filters, offset, hasMore, selectedMemory, indexing, reset };
});
