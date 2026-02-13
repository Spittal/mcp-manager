import type { ServerStatus } from '@/types/server';

export function statusColor(status: ServerStatus | string | undefined, enabled = true): string {
  if (!enabled) return 'bg-surface-3';
  switch (status) {
    case 'connected': return 'bg-status-connected';
    case 'connecting': return 'bg-status-connecting';
    case 'error': return 'bg-status-error';
    default: return 'bg-status-disconnected';
  }
}

export function statusLabel(status: ServerStatus | string | undefined): string {
  return status ?? 'disconnected';
}
