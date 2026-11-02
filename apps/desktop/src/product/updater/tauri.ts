import type { DownloadEvent, Update } from '@tauri-apps/plugin-updater';
import type { DownloadProgress, UpdateInfo, UpdaterBridge } from './types';

const handles = new Map<string, Update>();
let modulePromise: Promise<typeof import('@tauri-apps/plugin-updater')> | null = null;

function updaterModule() {
  modulePromise ??= import('@tauri-apps/plugin-updater');
  return modulePromise;
}

function platformMeta(update: Update): { size?: number; url?: string } {
  const raw = update.rawJson as {
    platforms?: Record<string, { size?: number; url?: string }>;
  };
  return Object.values(raw.platforms ?? {})[0] ?? {};
}

function progressFrom(
  event: DownloadEvent,
  state: { downloaded: number; total: number | null },
  onProgress: (progress: DownloadProgress) => void,
) {
  if (event.event === 'Started') {
    state.total = event.data.contentLength ?? state.total;
    state.downloaded = 0;
  } else if (event.event === 'Progress') {
    state.downloaded += event.data.chunkLength;
  } else {
    state.downloaded = state.total ?? state.downloaded;
  }
  onProgress({ downloaded: state.downloaded, total: state.total });
}

export function createTauriBridge(): UpdaterBridge {
  const handle = (info: UpdateInfo) => {
    const update = handles.get(info.version);
    if (!update) throw new Error('UPDATE_HANDLE_MISSING');
    return update;
  };

  return {
    async check() {
      const { check } = await updaterModule();
      const update = await check();
      if (!update) return null;
      handles.set(update.version, update);
      const meta = platformMeta(update);
      return {
        version: update.version,
        currentVersion: update.currentVersion,
        date: update.date,
        body: update.body,
        size: typeof meta.size === 'number' ? meta.size : undefined,
        url: typeof meta.url === 'string' ? meta.url : undefined,
      };
    },

    async download(info, onProgress) {
      const update = handle(info);
      const state = { downloaded: 0, total: info.size ?? null };
      await update.download((event) => progressFrom(event, state, onProgress));
    },

    async downloadAndInstall(info, onProgress) {
      const update = handle(info);
      const state = { downloaded: 0, total: info.size ?? null };
      await update.downloadAndInstall((event) => progressFrom(event, state, onProgress));
      handles.delete(info.version);
    },

    async install(info) {
      const update = handle(info);
      await update.install();
      handles.delete(info.version);
    },
  };
}
