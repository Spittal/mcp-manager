import { defineStore } from 'pinia';
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { ServerConfig, ServerConfigInput } from '@/types/server';

export const useServersStore = defineStore('servers', () => {
  const servers = ref<ServerConfig[]>([]);
  const selectedServerId = ref<string | null>(null);

  async function loadServers() {
    try {
      servers.value = await invoke<ServerConfig[]>('list_servers');
    } catch (e) {
      console.error('Failed to load servers:', e);
    }
  }

  async function addServer(input: ServerConfigInput) {
    try {
      const server = await invoke<ServerConfig>('add_server', { input });
      servers.value.push(server);
      selectedServerId.value = server.id;
    } catch (e) {
      console.error('Failed to add server:', e);
    }
  }

  async function removeServer(id: string) {
    try {
      await invoke('remove_server', { id });
      servers.value = servers.value.filter(s => s.id !== id);
      if (selectedServerId.value === id) {
        selectedServerId.value = servers.value[0]?.id ?? null;
      }
    } catch (e) {
      console.error('Failed to remove server:', e);
    }
  }

  async function connectServer(id: string) {
    const server = servers.value.find(s => s.id === id);
    if (server) server.status = 'connecting';
    try {
      await invoke('connect_server', { id });
    } catch (e) {
      console.error('Failed to connect server:', e);
      if (server) server.status = 'error';
    }
  }

  async function disconnectServer(id: string) {
    try {
      await invoke('disconnect_server', { id });
    } catch (e) {
      console.error('Failed to disconnect server:', e);
    }
  }

  function selectServer(id: string) {
    selectedServerId.value = id;
  }

  function updateServerStatus(id: string, status: ServerConfig['status']) {
    const server = servers.value.find(s => s.id === id);
    if (server) server.status = status;
  }

  return {
    servers,
    selectedServerId,
    loadServers,
    addServer,
    removeServer,
    connectServer,
    disconnectServer,
    selectServer,
    updateServerStatus,
  };
});
