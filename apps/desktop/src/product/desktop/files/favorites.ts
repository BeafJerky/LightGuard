const FAVORITES_PREFIX = 'lg.files.favorites.';
const RECENTS_PREFIX = 'lg.files.recents.';
const MAX_FAVORITES = 30;
const MAX_RECENTS = 20;

function read(key: string): string[] {
  try {
    const raw = localStorage.getItem(key);
    const value = raw ? (JSON.parse(raw) as unknown) : [];
    return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
  } catch {
    return [];
  }
}

function write(key: string, value: string[]): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* 隐私模式或配额不足时静默放弃，收藏只是本机便利功能。 */
  }
}

export function loadFavorites(serverId: string): string[] {
  return read(`${FAVORITES_PREFIX}${serverId}`).slice(0, MAX_FAVORITES);
}

export function toggleFavorite(serverId: string, path: string): boolean {
  const key = `${FAVORITES_PREFIX}${serverId}`;
  const current = read(key);
  const exists = current.includes(path);
  const next = exists
    ? current.filter((item) => item !== path)
    : [path, ...current.filter((item) => item !== path)].slice(0, MAX_FAVORITES);
  write(key, next);
  return !exists;
}

export function loadRecents(serverId: string): string[] {
  return read(`${RECENTS_PREFIX}${serverId}`).slice(0, MAX_RECENTS);
}

export function pushRecent(serverId: string, path: string): string[] {
  const key = `${RECENTS_PREFIX}${serverId}`;
  const next = [path, ...read(key).filter((item) => item !== path)].slice(0, MAX_RECENTS);
  write(key, next);
  return next;
}

export function removeRecent(serverId: string, path: string): string[] {
  const key = `${RECENTS_PREFIX}${serverId}`;
  const next = read(key).filter((item) => item !== path);
  write(key, next);
  return next;
}

export function removeFavorite(serverId: string, path: string): string[] {
  const key = `${FAVORITES_PREFIX}${serverId}`;
  const next = read(key).filter((item) => item !== path);
  write(key, next);
  return next;
}