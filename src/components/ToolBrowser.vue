<script setup lang="ts">
import { ref, computed } from 'vue';
import { useToolsStore } from '@/stores/tools';
import { storeToRefs } from 'pinia';
import type { McpTool } from '@/types/mcp';

const props = defineProps<{
  serverId?: string;
}>();

const toolsStore = useToolsStore();
const { searchQuery } = storeToRefs(toolsStore);
const selectedTool = ref<McpTool | null>(null);

const displayedTools = computed(() => {
  let result = toolsStore.filteredTools;
  if (props.serverId) {
    result = result.filter((t) => t.serverId === props.serverId);
  }
  return result;
});

function formatSchema(schema: Record<string, unknown> | undefined): string {
  if (!schema) return 'No parameters';
  const props = schema.properties as Record<string, { type?: string }> | undefined;
  if (!props) return 'No parameters';
  const required = (schema.required as string[]) ?? [];
  return Object.entries(props)
    .map(([key, val]) => {
      const opt = required.includes(key) ? '' : '?';
      return `${key}${opt}: ${val.type ?? 'any'}`;
    })
    .join(', ');
}
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Search -->
    <div class="border-b border-border px-3 py-2">
      <input
        v-model="searchQuery"
        type="text"
        placeholder="Search tools..."
        class="w-full rounded border border-border bg-surface-1 px-2.5 py-1.5 font-mono text-xs text-text-primary outline-none transition-colors placeholder:text-text-muted focus:border-accent"
      />
    </div>

    <div class="flex min-h-0 flex-1">
      <!-- Tool list -->
      <div class="flex-1 overflow-y-auto">
        <div
          v-for="tool in displayedTools"
          :key="`${tool.serverId}-${tool.name}`"
          class="cursor-pointer border-b border-border/50 px-3 py-2 transition-colors hover:bg-surface-2"
          :class="{ 'bg-surface-2': selectedTool?.name === tool.name && selectedTool?.serverId === tool.serverId }"
          @click="selectedTool = tool"
        >
          <div class="flex items-baseline gap-2">
            <span class="font-mono text-xs font-medium text-text-primary">{{ tool.name }}</span>
            <span v-if="!serverId" class="text-[11px] text-text-muted">{{ tool.serverName }}</span>
          </div>
          <p v-if="tool.description" class="mt-0.5 text-[11px] leading-snug text-text-secondary line-clamp-2">
            {{ tool.description }}
          </p>
          <p class="mt-1 font-mono text-[11px] text-text-muted truncate">
            {{ formatSchema(tool.inputSchema) }}
          </p>
        </div>

        <div v-if="displayedTools.length === 0" class="px-3 py-6 text-center text-xs text-text-muted">
          {{ searchQuery ? 'No tools match your search' : 'No tools available' }}
        </div>
      </div>

      <!-- Detail panel -->
      <div
        v-if="selectedTool"
        class="w-72 shrink-0 overflow-y-auto border-l border-border bg-surface-1 p-3"
      >
        <div class="mb-3 flex items-center justify-between">
          <h3 class="font-mono text-xs font-medium text-text-primary">{{ selectedTool.name }}</h3>
          <button
            class="text-[11px] text-text-muted transition-colors hover:text-text-secondary"
            @click="selectedTool = null"
          >
            Close
          </button>
        </div>

        <div class="mb-3">
          <span class="font-mono text-[11px] text-text-muted">Server:</span>
          <span class="ml-1 text-[11px] text-text-secondary">{{ selectedTool.serverName }}</span>
        </div>

        <p v-if="selectedTool.description" class="mb-3 text-[11px] leading-relaxed text-text-secondary">
          {{ selectedTool.description }}
        </p>

        <div v-if="selectedTool.inputSchema">
          <h4 class="mb-1.5 font-mono text-[11px] font-medium tracking-wide text-text-muted uppercase">Input Schema</h4>
          <pre class="overflow-x-auto rounded border border-border bg-surface-0 p-2 font-mono text-[11px] leading-relaxed text-text-secondary">{{ JSON.stringify(selectedTool.inputSchema, null, 2) }}</pre>
        </div>
      </div>
    </div>
  </div>
</template>
