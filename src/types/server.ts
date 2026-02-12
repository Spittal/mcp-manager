export type ServerTransport = 'stdio' | 'http';
export type ServerStatus = 'connected' | 'connecting' | 'disconnected' | 'error';
export type AuthType = 'none' | 'bearer' | 'oauth';

export interface OAuthConfig {
  authUrl: string;
  tokenUrl: string;
  clientId: string;
  scopes: string[];
}

export interface ServerAuth {
  type: AuthType;
  tokenRef?: string; // keychain reference, never the actual token
  oauthConfig?: OAuthConfig;
}

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
  auth?: ServerAuth;
  // metadata
  tags?: string[];
  status?: ServerStatus;
  lastConnected?: string;
}

export type ServerConfigInput = Omit<ServerConfig, 'id' | 'status' | 'lastConnected'>;
