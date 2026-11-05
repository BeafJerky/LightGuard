export type IconScheme = 'auto' | 'line' | 'color';

export type ResolvedIconScheme = 'line' | 'color';

const STORAGE_KEY = 'lg.files.icons';

const BASE_THEMES = new Set(['light', 'dark', 'win11', 'win11-dark']);

export function resolveIconScheme(setting: IconScheme, themeId: string): ResolvedIconScheme {
  if (setting === 'line' || setting === 'color') return setting;
  return BASE_THEMES.has(themeId) ? 'color' : 'line';
}

function isIconScheme(value: unknown): value is IconScheme {
  return value === 'auto' || value === 'line' || value === 'color';
}

export function loadIconScheme(): IconScheme {
  if (typeof window === 'undefined') return 'auto';
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return isIconScheme(stored) ? stored : 'auto';
  } catch {
    return 'auto';
  }
}

export function saveIconScheme(scheme: IconScheme): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(STORAGE_KEY, scheme);
  } catch {
    // Private windows can reject storage; the choice then lasts for the session only.
  }
}
