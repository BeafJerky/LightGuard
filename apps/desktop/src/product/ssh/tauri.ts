import {
  type ConnectInput,
  type ConnectResult,
  type CredentialStatus,
  type ReconnectInput,
  type ServerSnapshot,
  type SessionStatus,
  type SshBackend,
  type StatusEvent,
  type TerminalEvent,
  type TerminalOpenInput,
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

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await loadCore();
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toSshError(error);
  }
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

  resetLocalData() {
    return call<void>('ssh_reset_local_data');
  },

  onStatus(handler: (event: StatusEvent) => void) {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void loadEvents()
      .then(({ listen }) => listen<StatusEvent>('ssh://status', (event) => handler(event.payload)))
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        /* status events are best-effort */
      });
    return () => {
      disposed = true;
      unlisten?.();
      unlisten = null;
    };
  },
};
