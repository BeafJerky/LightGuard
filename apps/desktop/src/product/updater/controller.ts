import { reactive } from 'vue';
import type {
  DownloadProgress,
  UpdateErrorKind,
  UpdateFailedAction,
  UpdateInfo,
  UpdateState,
  UpdaterBridge,
} from './types';

function initialState(): UpdateState {
  return {
    phase: 'idle',
    info: null,
    progress: { downloaded: 0, total: null },
    error: null,
    lastCheckedAt: null,
    lastManualCheck: false,
    failedAction: null,
  };
}

export function classifyUpdateError(error: unknown): UpdateErrorKind {
  const message = (error instanceof Error ? error.message : String(error ?? '')).toLowerCase();
  if (/signature|minisign|verify/.test(message)) return 'signature';
  if (/json|manifest|deserialize|platform|release/.test(message)) return 'manifest';
  if (/disk|space|write file|rename/.test(message)) return 'disk';
  if (/network|request|connect|timeout|dns|http|status/.test(message)) return 'network';
  return 'unknown';
}

export function updateErrorMessage(kind: UpdateErrorKind): string {
  switch (kind) {
    case 'signature':
      return '更新包签名校验失败，已拒绝安装。';
    case 'manifest':
      return '更新清单无效或未包含当前平台。';
    case 'disk':
      return '更新包无法写入磁盘，请检查剩余空间。';
    case 'network':
      return '无法访问更新服务器，请检查网络或代理。';
    default:
      return '更新失败，请稍后重试。';
  }
}

export class UpdateController {
  readonly state = reactive<UpdateState>(initialState());
  private downloadedVersion = '';
  private runId = 0;

  constructor(private readonly bridge: UpdaterBridge) {}

  async check(options: { manual?: boolean } = {}): Promise<void> {
    if (
      this.state.phase === 'checking' ||
      this.state.phase === 'downloading' ||
      this.state.phase === 'ready' ||
      this.state.phase === 'installing'
    ) {
      return;
    }

    const manual = options.manual === true;
    this.state.lastManualCheck = manual;
    this.state.phase = 'checking';
    this.state.error = null;
    this.state.failedAction = null;
    try {
      const info = await this.bridge.check();
      this.state.lastCheckedAt = Date.now();
      if (!info) {
        this.state.info = null;
        this.state.phase = 'up-to-date';
        return;
      }
      if (info.version !== this.state.info?.version) this.downloadedVersion = '';
      this.state.info = info;
      if (info.cached) {
        this.downloadedVersion = info.version;
        this.state.phase = 'ready';
        this.state.progress = {
          downloaded: info.size ?? 0,
          total: info.size ?? null,
        };
        return;
      }
      this.state.phase = 'available';
    } catch (error) {
      if (!manual) {
        this.state.phase = 'idle';
        this.state.error = null;
        this.state.info = null;
        this.downloadedVersion = '';
        return;
      }
      this.setError(error, 'check');
    }
  }

  async download(): Promise<void> {
    const info = this.state.info;
    if (!info || (this.state.phase !== 'available' && this.state.phase !== 'error')) return;

    const runId = ++this.runId;
    this.state.phase = 'downloading';
    this.state.error = null;
    this.state.failedAction = null;
    this.state.progress = { downloaded: 0, total: info.size ?? null };
    const onProgress = (progress: DownloadProgress) => {
      if (runId === this.runId) this.state.progress = progress;
    };

    try {
      await this.bridge.download(info, onProgress);
      // 取消可能刚好与下载完成竞争；此时 Rust 已缓存安装包，不能把状态改成 ready。
      if (runId !== this.runId) return;
      this.downloadedVersion = info.version;
      this.state.phase = 'ready';
      this.state.progress = {
        downloaded: info.size ?? this.state.progress.downloaded,
        total: info.size ?? null,
      };
    } catch (error) {
      if (runId !== this.runId) return;
      this.setError(error, 'download');
    }
  }

  async cancel(): Promise<void> {
    if (this.state.phase !== 'downloading') return;
    // 先让在途请求失效，避免它的 reject 覆盖下面设置的状态。
    this.runId += 1;
    // 状态在 await 之前复位，避免 IPC 卡住或失败时把界面困在 downloading。
    const info = this.state.info;
    this.state.phase = info ? 'available' : 'idle';
    this.state.progress = { downloaded: 0, total: info?.size ?? null };
    this.state.error = null;
    this.state.failedAction = null;
    try {
      await this.bridge.cancel?.();
    } catch {
      // 取消失败也要回到可重试态；错误态会把用户困在下载中。
    }
  }

  async defer(): Promise<void> {
    if (this.state.phase !== 'ready') return;
    await this.bridge.defer?.();
  }

  async install(): Promise<boolean> {
    const info = this.state.info;
    if (this.state.phase === 'installing' || !info || this.downloadedVersion !== info.version) {
      return false;
    }

    this.state.phase = 'installing';
    this.state.error = null;
    this.state.failedAction = null;
    try {
      await this.bridge.install(info);
      return true;
    } catch (error) {
      this.setError(error, 'install');
      return false;
    }
  }

  reportRelaunchFailure(): void {
    this.state.phase = 'error';
    this.state.failedAction = 'relaunch';
    this.state.error = {
      kind: 'unknown',
      message: '更新已安装，但自动重启失败，请手动关闭并重新打开 LightGuard。',
    };
  }

  private setError(error: unknown, action: UpdateFailedAction) {
    const kind = classifyUpdateError(error);
    this.state.error = { kind, message: updateErrorMessage(kind) };
    this.state.failedAction = action;
    this.state.phase = 'error';
  }
}
