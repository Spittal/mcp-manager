export interface RedisHealth {
  ok: boolean;
  latencyMs: number;
  usedMemoryHuman: string | null;
  connectedClients: number | null;
  uptimeInSeconds: number | null;
  dbKeys: number | null;
  error: string | null;
}

export interface ProcessStats {
  name: string;
  command: string;
  pid: number;
  cpuPercent: number;
  memoryBytes: number;
}

export interface ProxyHealth {
  running: boolean;
  port: number;
}

export interface SystemStatusResponse {
  proxy: ProxyHealth;
  redis: RedisHealth | null;
  processes: ProcessStats[];
  serverCount: number;
  connectedCount: number;
  checkedAt: number;
}

export interface HealthCheckEntry {
  timestamp: number;
  proxyOk: boolean;
  redisOk: boolean;
}
