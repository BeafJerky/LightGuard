import { resolveTheme, type ThemeId, type ThemeMode } from '@lightguard/lightui';

export interface AppIconModes {
  window: ThemeMode;
  tray: ThemeMode;
}

export interface AppIconBridge {
  setIcons(modes: AppIconModes): Promise<void>;
}

export function resolveIconModes(consoleTheme: ThemeId, prefersDark: boolean): AppIconModes {
  return {
    window: resolveTheme(consoleTheme, prefersDark).mode,
    tray: prefersDark ? 'dark' : 'light',
  };
}

let bridgePromise: Promise<AppIconBridge> | null = null;

export function loadAppIconBridge(): Promise<AppIconBridge> {
  bridgePromise ??= import('@tauri-apps/api/core').then(({ invoke }) => ({
    setIcons: (modes) =>
      invoke<void>('set_app_icons', { windowMode: modes.window, trayMode: modes.tray }),
  }));
  return bridgePromise;
}

export async function syncAppIcons(
  consoleTheme: ThemeId,
  prefersDark: boolean,
  bridge?: AppIconBridge,
): Promise<void> {
  try {
    const api = bridge ?? (await loadAppIconBridge());
    await api.setIcons(resolveIconModes(consoleTheme, prefersDark));
  } catch (error) {
    // 图标切换是外观增强，失败不能影响主题本身。
    console.warn('[lightguard] 应用图标切换失败', error);
  }
}
