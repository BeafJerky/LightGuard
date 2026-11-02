import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { DownloadProgress, UpdateInfo, UpdaterBridge } from './types';

export function createTauriBridge(): UpdaterBridge {
  return {
    async check(): Promise<UpdateInfo | null> {
      return invoke<UpdateInfo | null>('update_check');
    },
    async download(_info, onProgress) {
      const unlisten = await listen<DownloadProgress>('update://progress', (event) => {
        onProgress(event.payload);
      });
      try {
        await invoke('update_download');
      } finally {
        unlisten();
      }
    },
    async install(info) {
      await invoke('update_install', { version: info.version });
    },
    async cancel() {
      await invoke('update_cancel');
    },
    async defer() {
      await invoke('update_defer');
    },
  };
}
