import type { ThemeId } from '@lightguard/lightui';

export type ServerStatus = 'online' | 'warning' | 'error' | 'offline' | 'connecting';
export type AuthKind = 'key' | 'password' | 'agent';
export type LogStatus = 'success' | 'warning' | 'failed';
export type LogSource = 'Web' | '桌面客户端';

export interface ServerMetrics {
  cpu: number;
  mem: number;
  disk: number;
  down: number;
  up: number;
}

export interface ServerHistory {
  cpu: number[];
  mem: number[];
  net: number[];
}

export interface DiskInfo {
  mount: string;
  fs: string;
  type: string;
  total: number;
  used: number;
  inodePercent: number;
}

export interface InterfaceInfo {
  name: string;
  ip: string;
  mac: string;
  speed: string;
  down: number;
  up: number;
  rxTotal: number;
  txTotal: number;
}

export interface EncryptionInfo {
  cipher: string;
  kex: string;
  mac: string;
}

export interface ServerSample {
  ts: number;
  cpu: number;
  mem: number;
  down: number;
  up: number;
  processes: number;
}

export interface ProcessInfo {
  pid: number;
  name: string;
  user: string;
  cpu: number;
  mem: number;
  state: string;
}

export interface ServerError {
  code: string;
  message: string;
  hint: string;
}

export interface Server {
  id: string;
  name: string;
  hostname: string;
  role: string;
  ip: string;
  port: number;
  region: string;
  group: string;
  tags: string[];
  note: string;
  status: ServerStatus;
  os: string;
  kernel: string;
  arch: string;
  cpuModel: string;
  cpuCores: number;
  cpuFreq: string;
  memTotal: number;
  memType: string;
  diskTotal: number;
  user: string;
  auth: AuthKind;
  authLabel: string;
  fingerprint: string;
  uptime: string;
  bootTime: string;
  timezone: string;
  lastSync: string;
  lastSeen: string;
  latency: number;
  load: [number, number, number];
  temp: number;
  warning: string;
  notice: string;
  error: ServerError | null;
  needsAuth: boolean;
  metrics: ServerMetrics;
  history: ServerHistory;
  cpuCoresLoad: number[];
  memAvailable: number;
  swapTotal: number;
  swapUsed: number;
  disks: DiskInfo[];
  interfaces: InterfaceInfo[];
  gateway: string;
  dns: string;
  publicIp: string;
  mtu: string;
  processes: ProcessInfo[];
  processCount: number;
  sampledAt: string;
  sessionId: string;
  startedAt: string;
  hostKeyState: 'new' | 'known' | '';
  encryption: EncryptionInfo | null;
  samples: ServerSample[];
}

export interface PersistedServer {
  id: string;
  name: string;
  hostname?: string;
  role: string;
  ip: string;
  port: number;
  region: string;
  group: string;
  tags: string[];
  note: string;
  user: string;
  auth: AuthKind;
  authLabel: string;
  fingerprint: string;
  lastSync: string;
  lastSeen: string;
  cpuCores: number;
  memTotal: number;
  diskTotal: number;
  os: string;
  kernel: string;
  arch: string;
  cpuModel: string;
  cpuFreq: string;
  timezone: string;
}

export interface LogEntry {
  id: string;
  ts: number;
  serverId: string;
  serverName: string;
  ip: string;
  port: number;
  action: string;
  status: LogStatus;
  duration: string;
  source: LogSource;
  code: string;
  auth: string;
  client: string;
  session: string;
  fingerprint: string;
  hint: string;
}

export interface ServerDraft {
  name: string;
  ip: string;
  port: number;
  user: string;
  auth: AuthKind;
  group: string;
  tags: string[];
  role: string;
  note: string;
}

export interface AppSettings {
  interval: number;
  autoRefresh: boolean;
  hostKey: boolean;
  clipboardClear: boolean;
  notifyFailure: boolean;
  notifyThreshold: boolean;
  idleLock: number;
}

export interface ThemeChoices {
  console: ThemeId;
  desktop: ThemeId;
}

export interface BackupSummary {
  at: string;
  hosts: number;
  encrypted: boolean;
}

export interface PersistedState {
  version: number;
  themes: ThemeChoices;
  settings: AppSettings;
  servers: Server[];
  logs: LogEntry[];
  backup: BackupSummary | null;
}

export type RouteName = 'overview' | 'connections' | 'settings' | 'desktop';

export interface Route {
  name: RouteName;
  id?: string;
}

export interface BackupConnection {
  id: string;
  name: string;
  role: string;
  ip: string;
  port: number;
  user: string;
  auth: AuthKind;
  group: string;
  tags: string[];
  note: string;
}

export interface BackupPayload {
  appVersion: string;
  scope: { settings: boolean; connections: boolean };
  settings: AppSettings;
  themes: ThemeChoices;
  connections: BackupConnection[];
}

export interface BackupEnvelope {
  format: 'lightguard-backup';
  version: 1;
  createdAt: string;
  encrypted: boolean;
  kdf?: { name: 'PBKDF2-SHA256'; iterations: number; salt: string };
  cipher?: { name: 'AES-256-GCM'; iv: string };
  payload: BackupPayload | string;
}
