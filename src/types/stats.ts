export interface ToolStats {
  totalCalls: number;
  errors: number;
  totalDurationMs: number;
}

export interface ToolCallEntry {
  tool: string;
  client: string;
  durationMs: number;
  isError: boolean;
  /** Unix timestamp in seconds. */
  timestamp: number;
}

export interface ServerStats {
  totalCalls: number;
  errors: number;
  totalDurationMs: number;
  tools: Record<string, ToolStats>;
  clients: Record<string, number>;
  recentCalls: ToolCallEntry[];
}
