import { computed } from 'vue';
import { useMemoriesStore } from '@/stores/memories';
import type { Nodes, Edges } from 'v-network-graph';

const TYPE_COLORS: Record<string, string> = {
  semantic: '#6366f1',
  episodic: '#f59e0b',
  message: '#10b981',
};

export function useMemoryGraph() {
  const store = useMemoriesStore();

  const nodes = computed<Nodes>(() => {
    const result: Nodes = {};
    for (const m of store.items) {
      const label = m.text.length > 40 ? m.text.slice(0, 40) + '...' : m.text;
      result[m.id] = { name: label, memoryType: m.memoryType };
    }
    return result;
  });

  const edges = computed<Edges>(() => {
    const result: Edges = {};
    const items = store.items;
    let edgeId = 0;

    for (let i = 0; i < items.length; i++) {
      for (let j = i + 1; j < items.length; j++) {
        const a = items[i];
        const b = items[j];

        const sharedTopics = a.topics.filter((t) => b.topics.includes(t));
        const sharedEntities = a.entities.filter((e) => b.entities.includes(e));
        const weight = sharedTopics.length + sharedEntities.length;

        if (weight > 0) {
          result[`e${edgeId++}`] = {
            source: a.id,
            target: b.id,
            weight,
          };
        }
      }
    }
    return result;
  });

  const nodeColor = (nodeId: string) => {
    const node = nodes.value[nodeId];
    return TYPE_COLORS[node?.memoryType] ?? '#6b7280';
  };

  return { nodes, edges, nodeColor, TYPE_COLORS };
}
