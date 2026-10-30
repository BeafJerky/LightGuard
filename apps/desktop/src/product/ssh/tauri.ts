import {
  type AlertFilterName,
  type AlertsChangedEvent,
  type AlertsListResult,
  type AuditEntry,
  type ConnectInput,
  type ConnectResult,
  type CredentialStatus,
  type DownloadInput,
  type DownloadResult,
  type FileEntry,
  type FileListing,
  type ImagePreview,
  type PermitEvent,
  type PermitStatus,
  type ReconnectInput,
  type ServerSnapshot,
  type SessionStatus,
  type SshBackend,
  type StatusEvent,
  type TerminalEvent,
  type TerminalOpenInput,
  type TextPreview,
  type TransferProgress,
  type WriteEvent,
  type WritePreview,
  type WritePreviewInput,
  type WriteResult,
  toSshError,
} from './backend';

type CoreModule = typeof import('@tauri-apps/api/core');
type EventModule = typeof import('@tauri-apps/api/event');

let coreModule: Promise<CoreModule> | null = null;
let eventModule: Promise<EventModule> | null = null;

const loadCore = () => (coreModule ??= import('@tauri-apps/api/core'));
const loadEvents = () => (eventModule ??= import('@tauri-apps/api/event'));

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await loadCore();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toSshError(error);
  }
}

// 事件监听按契约事件名统一装配：动态导入失败时静默降级，卸载时保证只取消一次。
export function listenEvent<T>(event: string, handler: (payload: T) => void): () => void {
  let disposed = false;
  let unlisten: (() => void) | null = null;
  void loadEvents()
    .then(({ listen }) => listen<T>(event, (payload) => handler(payload.payload)))
    .then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    })
    .catch(() => {
      /* best-effort events */
    });
  return () => {
    disposed = true;
    unlisten?.();
    unlisten = null;
  };
}

export const tauriBackend: SshBackend = {
  available: isTauriRuntime(),

  connect(input: ConnectInput) {
    return call<ConnectResult>('ssh_connect', { input });
  },

  reconnect(input: ReconnectInput) {
    return call<ConnectResult>('ssh_reconnect', { input });
  },

  snapshot(serverId: string) {
    return call<ServerSnapshot>('ssh_snapshot', { serverId });
  },

  sessionStatus(serverId: string) {
    return call<SessionStatus>('ssh_session_status', { serverId });
  },

  disconnect(serverId: string) {
    return call<void>('ssh_disconnect', { serverId });
  },

  saveCredentials(serverId, auth, secret, passphrase) {
    return call<void>('ssh_save_credentials', { serverId, auth, secret, passphrase });
  },

  credentialStatus(serverId: string) {
    return call<CredentialStatus>('ssh_credential_status', { serverId });
  },

  forgetCredentials(serverId: string) {
    return call<void>('ssh_forget_credentials', { serverId });
  },

  forgetHostKey(host: string, port: number) {
    return call<void>('ssh_forget_host_key', { host, port });
  },

  async terminalOpen(input: TerminalOpenInput) {
    const { Channel } = await loadCore();
    const channel = new Channel<TerminalEvent>();
    // Rust 侧按字节分块并以 Base64 传输；这里用流式解码避免多字节字符被截断。
    const decoder = new TextDecoder();
    channel.onmessage = (event) => {
      if (event.type === 'data') {
        const bytes = Uint8Array.from(atob(event.data), (char) => char.charCodeAt(0));
        input.onEvent({ type: 'data', data: decoder.decode(bytes, { stream: true }) });
        return;
      }
      input.onEvent(event);
    };
    return call<{ terminalId: string; title: string }>('ssh_terminal_open', {
      input: { serverId: input.serverId, cols: input.cols, rows: input.rows },
      onEvent: channel,
    });
  },

  terminalWrite(serverId, terminalId, data) {
    return call<void>('ssh_terminal_write', { serverId, terminalId, data });
  },

  terminalResize(serverId, terminalId, cols, rows) {
    return call<void>('ssh_terminal_resize', { serverId, terminalId, cols, rows });
  },

  terminalClose(serverId, terminalId) {
    return call<void>('ssh_terminal_close', { serverId, terminalId });
  },

  sftpHome(serverId: string) {
    return call<string>('sftp_home', { serverId });
  },

  sftpList(serverId: string, path: string) {
    return call<FileListing>('sftp_list', { serverId, path });
  },

  sftpStat(serverId: string, path: string) {
    return call<FileEntry>('sftp_stat', { serverId, path });
  },

  sftpPreviewText(serverId: string, path: string) {
    return call<TextPreview>('sftp_preview_text', { serverId, path });
  },

  sftpPreviewImage(serverId: string, path: string) {
    return call<ImagePreview>('sftp_preview_image', { serverId, path });
  },

  async sftpDownload(input: DownloadInput) {
    const { Channel } = await loadCore();
    const channel = new Channel<TransferProgress>();
    channel.onmessage = (event) => input.onEvent(event);
    return call<DownloadResult>('sftp_download', {
      serverId: input.serverId,
      path: input.path,
      kind: input.kind,
      transferId: input.transferId,
      onEvent: channel,
    });
  },

  sftpTransferCancel(transferId: string) {
    return call<void>('sftp_transfer_cancel', { transferId });
  },

  resetLocalData() {
    return call<void>('ssh_reset_local_data');
  },

  permitStatus(serverId: string) {
    return call<PermitStatus>('permit_status', { serverId });
  },

  permitUnlock(serverId: string, minutes: number) {
    return call<PermitStatus>('permit_unlock', { serverId, minutes });
  },

  permitLock(serverId: string) {
    return call<PermitStatus>('permit_lock', { serverId });
  },

  writePreview(input: WritePreviewInput) {
    return call<WritePreview>('write_preview', { ...input });
  },

  writeExecute(previewId: string) {
    return call<WriteResult>('write_execute', { previewId });
  },

  writeRollback(auditId: number) {
    return call<WriteResult>('write_rollback', { auditId });
  },

  auditList(limit: number, offset: number) {
    return call<AuditEntry[]>('audit_list', { limit, offset });
  },

  alertsList(filter: AlertFilterName, limit: number, offset: number) {
    return call<AlertsListResult>('alerts_list', { filter, limit, offset });
  },

  alertsIgnore(alertId: number, reason: string, untilMs: number) {
    return call<void>('alerts_ignore', { alertId, reason, untilMs });
  },

  alertsMute(serverId: string, ruleId: string | null, untilMs: number) {
    return call<void>('alerts_mute', { serverId, ruleId, untilMs });
  },

  testNotification() {
    return call<boolean>('notify_test');
  },

  onStatus(handler: (event: StatusEvent) => void) {
    return listenEvent<StatusEvent>('ssh://status', handler);
  },

  onPermit(handler: (event: PermitEvent) => void) {
    return listenEvent<PermitEvent>('ssh://permit', handler);
  },

  onWrite(handler: (event: WriteEvent) => void) {
    return listenEvent<WriteEvent>('guard://write', handler);
  },

  onAlertsChanged(handler: (event: AlertsChangedEvent) => void) {
    return listenEvent<AlertsChangedEvent>('alerts://changed', handler);
  },
};
