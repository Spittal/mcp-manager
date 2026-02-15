export interface MemoryItem {
  id: string;
  text: string;
  memoryType: string;
  userId: string | null;
  sessionId: string | null;
  namespace: string | null;
  topics: string[];
  entities: string[];
  eventDate: string | null;
  createdAt: string;
  lastAccessed: string;
  updatedAt: string;
  pinned: boolean;
  distance: number | null;
}

export interface MemorySearchResult {
  memories: MemoryItem[];
  total: number;
  nextOffset: number | null;
}

export interface SearchFilters {
  memoryType?: string;
  topics?: string[];
  entities?: string[];
  namespace?: string;
  userId?: string;
  sessionId?: string;
}
