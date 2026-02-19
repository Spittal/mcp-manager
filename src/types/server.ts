export type ServerTransport = 'stdio' | 'http';
export type ServerStatus = 'connected' | 'connecting' | 'disconnected' | 'error';

export interface ServerConfig {
  id: string;
  name: string;
  enabled: boolean;
  transport: ServerTransport;
  // stdio
  command?: string;
  args?: string[];
  env?: Record<string, string>;
  // http
  url?: string;
  headers?: Record<string, string>;
  // metadata
  tags?: string[];
  status?: ServerStatus;
  lastConnected?: string;
  managed?: boolean;
  managedBy?: string;
  registryName?: string;
}

export type ServerConfigInput = Omit<ServerConfig, 'id' | 'status' | 'lastConnected'>;
