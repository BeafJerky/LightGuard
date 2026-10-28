import type { FileEntry } from '../../ssh/backend';
import { fileTypeLabel } from './format';

export type SortKey = 'name' | 'size' | 'mtime' | 'type' | 'mode' | 'owner';
export type SortDirection = 'asc' | 'desc';

export interface ListViewState {
  sortKey: SortKey;
  sortDir: SortDirection;
  query: string;
  showHidden: boolean;
}

export function toggleSort(view: Pick<ListViewState, 'sortKey' | 'sortDir'>, key: SortKey) {
  return {
    sortKey: key,
    sortDir: view.sortKey === key && view.sortDir === 'asc' ? 'desc' as const : 'asc' as const,
  };
}

function isDirectory(entry: FileEntry): number {
  return (entry.targetKind ?? entry.kind) === 'dir' ? 0 : 1;
}

function compare(a: FileEntry, b: FileEntry, key: SortKey): number {
  switch (key) {
    case 'size':
      return a.size - b.size;
    case 'mtime':
      return (a.mtime ?? 0) - (b.mtime ?? 0);
    case 'mode':
      return a.modeText.localeCompare(b.modeText);
    case 'owner':
      return (a.uid ?? 0) - (b.uid ?? 0) || (a.gid ?? 0) - (b.gid ?? 0);
    case 'type':
      return fileTypeLabel(a).localeCompare(fileTypeLabel(b), 'zh-Hans-CN');
    default:
      return a.name.localeCompare(b.name, 'zh-Hans-CN', { numeric: true, sensitivity: 'base' });
  }
}

export function visibleEntries(entries: FileEntry[], view: ListViewState): FileEntry[] {
  const query = view.query.trim().toLowerCase();
  const filtered = entries.filter((entry) => {
    if (!view.showHidden && entry.name.startsWith('.')) return false;
    return !query || entry.name.toLowerCase().includes(query);
  });
  const direction = view.sortDir === 'asc' ? 1 : -1;
  return [...filtered].sort((a, b) => {
    const typeDelta = isDirectory(a) - isDirectory(b);
    if (typeDelta !== 0) return typeDelta;
    const delta = compare(a, b, view.sortKey);
    return delta === 0 ? a.name.localeCompare(b.name) * direction : delta * direction;
  });
}

export function moveSelection(
  entries: FileEntry[],
  currentPath: string,
  action: 'up' | 'down' | 'page-up' | 'page-down' | 'home' | 'end',
  pageSize: number,
): string | null {
  if (entries.length === 0) return null;
  const index = entries.findIndex((entry) => entry.path === currentPath);
  const base = index < 0 ? 0 : index;
  const page = Math.max(1, pageSize);
  const next = {
    up: Math.max(0, base - 1),
    down: Math.min(entries.length - 1, base + 1),
    'page-up': Math.max(0, base - page),
    'page-down': Math.min(entries.length - 1, base + page),
    home: 0,
    end: entries.length - 1,
  }[action];
  return entries[next]?.path ?? null;
}
