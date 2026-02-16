import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { SystemStatusResponse, HealthCheckEntry } from '@/types/status';

export function useSystemStatus() {
  const status = ref<SystemStatusResponse | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const history = ref<HealthCheckEntry[]>([]);

  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    loading.value = true;
    error.value = null;

    try {
      const result = await invoke<SystemStatusResponse>('get_system_status');
      status.value = result;

      history.value.push({
        timestamp: Date.now(),
        proxyOk: result.proxy.running,
        redisOk: result.redis?.ok ?? false,
      });
      if (history.value.length > 60) {
        history.value = history.value.slice(-60);
      }
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  onMounted(() => {
    refresh();
    timer = setInterval(refresh, 10_000);
  });

  onUnmounted(() => {
    if (timer) clearInterval(timer);
  });

  return { status, loading, error, history, refresh };
}
