export interface PluginComponent {
  /** Category label, e.g. "Skills", "Agents", "Commands", "Hooks", "MCP Servers" */
  category: string;
  /** Individual item names within this category */
  items: string[];
}

export interface PluginInfo {
  id: string;
  name: string;
  description: string;
  marketplace: string;
  version?: string;
  installCount?: number;
  isRemote: boolean;
  installed: boolean;
  enabled: boolean;
  scope?: string;
  /** What this plugin includes — grouped by category with individual item names */
  components: PluginComponent[];
}
