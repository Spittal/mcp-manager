import { onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useLogsStore, type LogEntry } from '@/stores/logs';

interface ServerLogPayload {
  serverId: string;
  level: LogEntry['level'];
  message: string;
}

export function useServerLogs() {
  let unlisten: UnlistenFn | null = null;

  onMounted(async () => {
    const logsStore = useLogsStore();

    unlisten = await listen<ServerLogPayload>('server-log', (event) => {
      logsStore.addLog({
        timestamp: new Date().toISOString(),
        serverId: event.payload.serverId,
        level: event.payload.level,
        message: event.payload.message,
      });
    });
  });

  onUnmounted(() => {
    unlisten?.();
  });
}
