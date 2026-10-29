import type { FileEntry } from '../../ssh/backend';

export interface TreeItem {
  name: string;
  path: string;
  kind: 'dir' | 'symlink';
  targetKind?: FileEntry['kind'];
}

export function treeChildren(entries: FileEntry[]): TreeItem[] {
  return entries
    .filter((entry) => entry.kind === 'dir' || entry.kind === 'symlink')
    .map((entry) => ({
      name: entry.name,
      path: entry.path,
      kind: entry.kind === 'symlink' ? 'symlink' : 'dir',
      targetKind: entry.targetKind,
    }));
}

interface CacheEntry {
  at: number;
  entries: FileEntry[];
}

type Loader = (path: string) => Promise<FileEntry[]>;

export class TreeCache {
  private cache = new Map<string, CacheEntry>();
  private inflight = new Map<string, Promise<FileEntry[]>>();
  private active = 0;
  private queue: Array<() => void> = [];

  constructor(
    private readonly loader: Loader,
    private readonly ttlMs = 30_000,
    private readonly maxConcurrent = 3,
  ) {}

  peek(path: string): FileEntry[] | null {
    const hit = this.cache.get(path);
    if (!hit) return null;
    if (Date.now() - hit.at > this.ttlMs) {
      this.cache.delete(path);
      return null;
    }
    return hit.entries;
  }

  load(path: string, force = false): Promise<FileEntry[]> {
    if (!force) {
      const hit = this.peek(path);
      if (hit) return Promise.resolve(hit);
    }
    const pending = this.inflight.get(path);
    if (pending) return pending;
    const task = this.run(path).finally(() => this.inflight.delete(path));
    this.inflight.set(path, task);
    return task;
  }

  clear(): void {
    this.cache.clear();
  }

  private async run(path: string): Promise<FileEntry[]> {
    await this.acquire();
    try {
      const entries = await this.loader(path);
      this.cache.set(path, { at: Date.now(), entries });
      return entries;
    } finally {
      this.release();
    }
  }

  private acquire(): Promise<void> {
    if (this.active < this.maxConcurrent) {
      this.active += 1;
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      this.queue.push(() => {
        this.active += 1;
        resolve();
      });
    });
  }

  private release(): void {
    this.active -= 1;
    this.queue.shift()?.();
  }
}
