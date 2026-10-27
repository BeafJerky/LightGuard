import type { AuthKind } from '../types';

export type BackendAuth = Extract<AuthKind, 'password' | 'key'>;

export type HostKeyState = 'new' | 'known';

export interface SnapshotDisk {
  mount: string;
  fs: string;
  kind: string;
  totalGb: number;
  usedGb: number;
  inodePercent: number;
}

export interface SnapshotInterface {
  name: string;
  ip: string;
  mac: string;
  speed: string;
  rxTotalGb: number;
  txTotalGb: number;
  downMbps: number;
  upMbps: number;
}

export interface SnapshotProcess {
  pid: number;
  name: string;
  user: string;
  cpu: number;
  mem: number;
  state: string;
}

export interface EncryptionInfo {
  cipher: string;
  kex: string;
  mac: string;
}

export interface ServerSnapshot {
  hostname: string;
  os: string;
  kernel: string;
  arch: string;
  timezone: string;
  uptimeSeconds: number;
  bootTime: string;
  cpuModel: string;
  cpuCores: number;
  cpuFreq: string;
  cpuPercent: number;
  cpuCoresLoad: number[];
  load: [number, number, number];
  tempC: number;
  memTotalGb: number;
  memAvailableGb: number;
  memPercent: number;
  swapTotalGb: number;
  swapUsedGb: number;
  diskTotalGb: number;
  diskPercent: number;
  disks: SnapshotDisk[];
  downMbps: number;
  upMbps: number;
  interfaces: SnapshotInterface[];
  gateway: string;
  dns: string;
  publicIp: string;
  mtu: string;
  processes: SnapshotProcess[];
  processCount: number;
  sampledAt: string;
}

export interface ConnectInput {
  serverId: string;
  host: string;
  port: number;
  user: string;
  auth: BackendAuth;
  secret: string;
  passphrase?: string;
  remember: boolean;
  verifyHostKey: boolean;
}

export interface ConnectResult {
  sessionId: string;
  startedAt: string;
  fingerprint: string;
  hostKeyState: HostKeyState;
  authLabel: string;
  latencyMs: number;
  snapshot: ServerSnapshot;
  encryption?: EncryptionInfo;
}

export interface ReconnectInput {
  serverId: string;
  host: string;
  port: number;
  user: string;
  verifyHostKey: boolean;
}

export interface CredentialStatus {
  saved: boolean;
  auth: BackendAuth | null;
}

export interface SessionStatus {
  connected: boolean;
  sessionId?: string;
}

export type TerminalEvent =
  | { type: 'data'; data: string }
  | { type: 'title'; title: string }
  | { type: 'exit'; code: number | null }
  | { type: 'closed'; reason: string };

export interface TerminalOpenInput {
  serverId: string;
  cols: number;
  rows: number;
  onEvent: (event: TerminalEvent) => void;
}

export interface StatusEvent {
  serverId: string;
  state: 'closed' | 'auth-failed' | 'reconnected';
  code?: string;
  message?: string;
}

export type CommandErrorCode =
  | 'INVALID_INPUT'
  | 'HOST_UNREACHABLE'
  | 'TIMEOUT'
  | 'HANDSHAKE_FAILED'
  | 'HOST_KEY_MISMATCH'
  | 'AUTH_FAILED'
  | 'SESSION_NOT_FOUND'
  | 'SESSION_CLOSED'
  | 'COLLECT_FAILED'
  | 'TERMINAL_FAILED'
  | 'CREDENTIAL_ERROR'
  | 'INTERNAL'
  | 'UNSUPPORTED';

export type ConnectStage = 'resolve' | 'tcp' | 'handshake' | 'hostKey' | 'auth' | 'collect';

export class SshCommandError extends Error {
  readonly code: CommandErrorCode;
  readonly hint: string;
  readonly stage?: ConnectStage;

  constructor(code: CommandErrorCode, message: string, hint = '', stage?: ConnectStage) {
    super(message);
    this.name = 'SshCommandError';
    this.code = code;
    this.hint = hint;
    this.stage = stage;
  }
}

const CODES = new Set<string>([
  'INVALID_INPUT',
  'HOST_UNREACHABLE',
  'TIMEOUT',
  'HANDSHAKE_FAILED',
  'HOST_KEY_MISMATCH',
  'AUTH_FAILED',
  'SESSION_NOT_FOUND',
  'SESSION_CLOSED',
  'COLLECT_FAILED',
  'TERMINAL_FAILED',
  'CREDENTIAL_ERROR',
  'INTERNAL',
  'UNSUPPORTED',
]);

const STAGES = new Set<string>(['resolve', 'tcp', 'handshake', 'hostKey', 'auth', 'collect']);

export function toSshError(error: unknown): SshCommandError {
  if (error instanceof SshCommandError) return error;
  if (error && typeof error === 'object') {
    const raw = error as Record<string, unknown>;
    const code = typeof raw.code === 'string' && CODES.has(raw.code) ? (raw.code as CommandErrorCode) : 'INTERNAL';
    const message = typeof raw.message === 'string' && raw.message ? raw.message : '操作失败';
    const hint = typeof raw.hint === 'string' ? raw.hint : '';
    const stage =
      typeof raw.stage === 'string' && STAGES.has(raw.stage) ? (raw.stage as ConnectStage) : undefined;
    return new SshCommandError(code, message, hint, stage);
  }
  if (typeof error === 'string' && error) {
    return new SshCommandError('INTERNAL', error);
  }
  return new SshCommandError('INTERNAL', '操作失败');
}

export const UNSUPPORTED_MESSAGE = '真实 SSH 仅桌面版可用';
export const UNSUPPORTED_HINT = '请在 LightGuard 桌面客户端中连接服务器。';

export interface SshBackend {
  readonly available: boolean;
  connect(input: ConnectInput): Promise<ConnectResult>;
  reconnect(input: ReconnectInput): Promise<ConnectResult>;
  snapshot(serverId: string): Promise<ServerSnapshot>;
  sessionStatus(serverId: string): Promise<SessionStatus>;
  disconnect(serverId: string): Promise<void>;
  saveCredentials(serverId: string, auth: BackendAuth, secret: string, passphrase?: string): Promise<void>;
  credentialStatus(serverId: string): Promise<CredentialStatus>;
  forgetCredentials(serverId: string): Promise<void>;
  forgetHostKey(host: string, port: number): Promise<void>;
  terminalOpen(input: TerminalOpenInput): Promise<{ terminalId: string; title: string }>;
  terminalWrite(serverId: string, terminalId: string, data: string): Promise<void>;
  terminalResize(serverId: string, terminalId: string, cols: number, rows: number): Promise<void>;
  terminalClose(serverId: string, terminalId: string): Promise<void>;
  resetLocalData(): Promise<void>;
  onStatus(handler: (event: StatusEvent) => void): () => void;
}
