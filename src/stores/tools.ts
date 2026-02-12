import { defineStore } from 'pinia';
import { ref, computed } from 'vue';
import type { McpTool } from '@/types/mcp';

export const useToolsStore = defineStore('tools', () => {
  const tools = ref<McpTool[]>([]);
  const searchQuery = ref('');

  const filteredTools = computed(() => {
    if (!searchQuery.value) return tools.value;
    const q = searchQuery.value.toLowerCase();
    return tools.value.filter(
      t =>
        t.name.toLowerCase().includes(q) ||
        t.description?.toLowerCase().includes(q) ||
        t.serverName.toLowerCase().includes(q)
    );
  });

  function setTools(serverId: string, serverName: string, newTools: McpTool[]) {
    // Remove old tools for this server, add new ones
    tools.value = [
      ...tools.value.filter(t => t.serverId !== serverId),
      ...newTools.map(t => ({ ...t, serverId, serverName })),
    ];
  }

  function clearToolsForServer(serverId: string) {
    tools.value = tools.value.filter(t => t.serverId !== serverId);
  }

  return { tools, searchQuery, filteredTools, setTools, clearToolsForServer };
});
