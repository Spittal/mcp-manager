import { defineStore } from 'pinia';
import { ref } from 'vue';

export interface LogEntry {
  timestamp: string;
  serverId: string;
  level: 'info' | 'error' | 'debug';
  message: string;
}

export const useLogsStore = defineStore('logs', () => {
  const logs = ref<LogEntry[]>([]);
  const maxLogs = 1000;

  function addLog(entry: LogEntry) {
    logs.value.push(entry);
    if (logs.value.length > maxLogs) {
      logs.value = logs.value.slice(-maxLogs);
    }
  }

  function logsForServer(serverId: string): LogEntry[] {
    return logs.value.filter(l => l.serverId === serverId);
  }

  function clearLogs(serverId?: string) {
    if (serverId) {
      logs.value = logs.value.filter(l => l.serverId !== serverId);
    } else {
      logs.value = [];
    }
  }

  return { logs, addLog, logsForServer, clearLogs };
});
