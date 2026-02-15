export type EmbeddingProvider = 'ollama' | 'openai';

export interface EmbeddingConfig {
  provider: EmbeddingProvider;
  model: string;
  dimensions: number;
}

export interface EmbeddingConfigStatus {
  config: EmbeddingConfig;
  hasOpenaiKey: boolean;
  pulledOllamaModels: string[];
}

export interface EmbeddingModelInfo {
  model: string;
  displayName: string;
  dimensions: number;
  sizeLabel: string;
  quality: 'high' | 'medium' | 'light';
  recommended?: boolean;
}

export const OLLAMA_MODELS: EmbeddingModelInfo[] = [
  { model: 'mxbai-embed-large', displayName: 'mxbai-embed-large', dimensions: 1024,
    sizeLabel: '~670MB download, ~1.2GB RAM', quality: 'high', recommended: true },
  { model: 'nomic-embed-text', displayName: 'nomic-embed-text', dimensions: 768,
    sizeLabel: '~274MB download, ~0.5GB RAM', quality: 'medium' },
  { model: 'snowflake-arctic-embed:335m', displayName: 'snowflake-arctic-embed', dimensions: 1024,
    sizeLabel: '~670MB download, ~1.2GB RAM', quality: 'high' },
  { model: 'all-minilm', displayName: 'all-minilm', dimensions: 384,
    sizeLabel: '~23MB download, ~0.1GB RAM', quality: 'light' },
];

export const OPENAI_MODELS: EmbeddingModelInfo[] = [
  { model: 'text-embedding-3-small', displayName: 'text-embedding-3-small', dimensions: 1536,
    sizeLabel: '$0.02 / 1M tokens', quality: 'high', recommended: true },
  { model: 'text-embedding-3-large', displayName: 'text-embedding-3-large', dimensions: 3072,
    sizeLabel: '$0.13 / 1M tokens', quality: 'high' },
];
