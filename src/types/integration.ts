export interface ExistingMcpServer {
  name: string;
  transport: string;
  command?: string;
  args?: string[];
  url?: string;
}

export interface AiToolInfo {
  id: string;
  name: string;
  installed: boolean;
  enabled: boolean;
  configPath: string;
  configuredPort: number;
  existingServers: ExistingMcpServer[];
}
