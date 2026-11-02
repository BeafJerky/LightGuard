export type UpdatePhase =
  | 'idle'
  | 'checking'
  | 'up-to-date'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'installing'
  | 'error';

export type UpdateErrorKind = 'network' | 'manifest' | 'signature' | 'disk' | 'unknown';

export type UpdateFailedAction = 'check' | 'download' | 'install' | 'relaunch';

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  date?: string;
  body?: string;
  size?: number;
  url?: string;
}

export interface DownloadProgress {
  downloaded: number;
  total: number | null;
}

export interface UpdaterBridge {
  check(): Promise<UpdateInfo | null>;
  download(
    info: UpdateInfo,
    onProgress: (progress: DownloadProgress) => void,
  ): Promise<void>;
  downloadAndInstall?(
    info: UpdateInfo,
    onProgress: (progress: DownloadProgress) => void,
  ): Promise<void>;
  install(info: UpdateInfo): Promise<void>;
}

export interface UpdateError {
  kind: UpdateErrorKind;
  message: string;
}

export interface UpdateState {
  phase: UpdatePhase;
  info: UpdateInfo | null;
  progress: DownloadProgress;
  error: UpdateError | null;
  lastCheckedAt: number | null;
  lastManualCheck: boolean;
  failedAction: UpdateFailedAction | null;
}
