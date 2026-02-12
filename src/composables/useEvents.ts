import { onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useServersStore } from '@/stores/servers';
import { useToolsStore } from '@/stores/tools';
import type { ServerStatus } from '@/types/server';
import type { McpTool } from '@/types/mcp';

interface ServerStatusPayload {
  serverId: string;
  status: ServerStatus;
  error?: string;
  lastConnected?: string;
}

interface ServerErrorPayload {
  serverId: string;
  error: string;
}

interface ToolsUpdatedPayload {
  serverId: string;
  serverName: string;
  tools: McpTool[];
}

export function useEvents() {
  const unlisteners: UnlistenFn[] = [];

  onMounted(async () => {
    const serversStore = useServersStore();
    const toolsStore = useToolsStore();

    unlisteners.push(
      await listen<ServerStatusPayload>('server-status-changed', (event) => {
        serversStore.updateServerStatus(event.payload.serverId, event.payload.status);
        if (event.payload.status === 'error' && event.payload.error) {
          serversStore.setError(event.payload.serverId, event.payload.error);
        }
      })
    );

    unlisteners.push(
      await listen<ServerErrorPayload>('server-error', (event) => {
        serversStore.setError(event.payload.serverId, event.payload.error);
      })
    );

    unlisteners.push(
      await listen<ToolsUpdatedPayload>('tools-updated', (event) => {
        toolsStore.setTools(
          event.payload.serverId,
          event.payload.serverName,
          event.payload.tools
        );
      })
    );
  });

  onUnmounted(() => {
    unlisteners.forEach((unlisten) => unlisten());
  });
}
