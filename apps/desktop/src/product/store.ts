import { computed, reactive } from 'vue';
import { themeOptions } from '@lightguard/lightui';
import type { ThemeId } from '@lightguard/lightui';
import { resetRenderSettings } from '../render/settings';
import { APP_VERSION, AUTH_LABELS, MAX_LOGS, round, uid } from './data';
import {
  applySnapshot,
  fromPersisted,
  markConnected,
  newServer,
  ssh,
  toPersisted,
  toSshError,
  type BackendAuth,
  type ConnectResult,
  type ServerSnapshot,
  type StatusEvent,
} from './ssh';
import {
  LEGACY_BACKUP_KEY,
  LEGACY_STORE_KEY,
  activatePersistence,
  clearLegacyAppKeys,
  isPersistenceActive,
  loadStoredState,
  replaceStoredState,
  resetStoredState,
  writePersistence,
  type StoredState,
} from './persistence';
import type {
  AppSettings,
  BackupConnection,
  BackupPayload,
  BackupSummary,
  LogEntry,
  LogSource,
  PersistedServer,
  PersistedState,
  Server,
  ServerDraft,
  ThemeChoices,
} from './types';

const STORE_KEY = LEGACY_STORE_KEY;
const STORE_VERSION = 2;
const BACKUP_SUMMARY_KEY = LEGACY_BACKUP_KEY;

const THEME_IDS = new Set<string>(themeOptions.map((option) => option.id));

export const DEFAULT_SETTINGS: AppSettings = {
  interval: 5,
  autoRefresh: true,
  hostKey: true,
  clipboardClear: true,
  notifyFailure: true,
  notifyThreshold: true,
  idleLock: 30,
  closeBehavior: 'ask',
};

export const DEFAULT_THEMES: ThemeChoices = {
  console: 'dark',
  desktop: 'pixel',
};

export const backendAvailable = ssh.available;

function readTheme(value: unknown, fallback: ThemeId): ThemeId {
  return typeof value === 'string' && THEME_IDS.has(value) ? (value as ThemeId) : fallback;
}

function readSettings(value: unknown): AppSettings {
  const raw = (value ?? {}) as Partial<AppSettings>;
  return {
    interval: typeof raw.interval === 'number' && raw.interval >= 2 && raw.interval <= 60 ? raw.interval : DEFAULT_SETTINGS.interval,
    autoRefresh: typeof raw.autoRefresh === 'boolean' ? raw.autoRefresh : DEFAULT_SETTINGS.autoRefresh,
    hostKey: typeof raw.hostKey === 'boolean' ? raw.hostKey : DEFAULT_SETTINGS.hostKey,
    clipboardClear:
      typeof raw.clipboardClear === 'boolean' ? raw.clipboardClear : DEFAULT_SETTINGS.clipboardClear,
    notifyFailure:
      typeof raw.notifyFailure === 'boolean' ? raw.notifyFailure : DEFAULT_SETTINGS.notifyFailure,
    notifyThreshold:
      typeof raw.notifyThreshold === 'boolean'
        ? raw.notifyThreshold
        : DEFAULT_SETTINGS.notifyThreshold,
    idleLock: typeof raw.idleLock === 'number' ? raw.idleLock : DEFAULT_SETTINGS.idleLock,
    closeBehavior:
      raw.closeBehavior === 'tray' || raw.closeBehavior === 'quit'
        ? raw.closeBehavior
        : DEFAULT_SETTINGS.closeBehavior,
  };
}

function readThemes(value: unknown): ThemeChoices {
  const raw = (value ?? {}) as Partial<Record<keyof ThemeChoices, unknown>>;
  return {
    console: readTheme(raw.console, DEFAULT_THEMES.console),
    desktop: readTheme(raw.desktop, DEFAULT_THEMES.desktop),
  };
}

function normalizeBackup(value: unknown): BackupSummary | null {
  const candidate = value as Partial<BackupSummary> | null | undefined;
  if (
    candidate &&
    typeof candidate.at === 'string' &&
    typeof candidate.hosts === 'number' &&
    typeof candidate.encrypted === 'boolean'
  ) {
    return { at: candidate.at, hosts: candidate.hosts, encrypted: candidate.encrypted };
  }
  return null;
}

function readBackupSummary(fallback: unknown): BackupSummary | null {
  try {
    const raw = window.localStorage.getItem(BACKUP_SUMMARY_KEY);
    const parsed = raw ? normalizeBackup(JSON.parse(raw)) : null;
    if (parsed) return parsed;
  } catch {
    /* fall through to the app-state snapshot */
  }
  return normalizeBackup(fallback);
}

function readServers(value: unknown): Server[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((item): item is PersistedServer => {
      if (!item || typeof item !== 'object') return false;
      const raw = item as Partial<PersistedServer>;
      return typeof raw.id === 'string' && typeof raw.name === 'string' && typeof raw.ip === 'string';
    })
    .map(fromPersisted);
}

function readLogs(value: unknown): LogEntry[] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((item): item is LogEntry => {
      if (!item || typeof item !== 'object') return false;
      const raw = item as Partial<LogEntry>;
      return typeof raw.id === 'string' && typeof raw.ts === 'number' && typeof raw.action === 'string';
    })
    .slice(0, MAX_LOGS)
    .map((log) => ({ ...log, port: typeof log.port === 'number' ? log.port : 22 }));
}

function seedState(): PersistedState {
  return {
    version: STORE_VERSION,
    themes: { ...DEFAULT_THEMES },
    settings: { ...DEFAULT_SETTINGS },
    servers: [],
    logs: [],
    backup: readBackupSummary(null),
  };
}

function loadState(): PersistedState | null {
  try {
    const raw = window.localStorage.getItem(STORE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Record<string, unknown> | null;
    if (!parsed) return null;
    const version = typeof parsed.version === 'number' ? parsed.version : 0;
    const themes = readThemes(parsed.themes);
    const settings = readSettings(parsed.settings);
    if (version === STORE_VERSION) {
      return {
        version: STORE_VERSION,
        themes,
        settings,
        servers: readServers(parsed.servers),
        logs: readLogs(parsed.logs),
        backup: readBackupSummary(parsed.backup),
      };
    }
    // v1 held generated demo hosts and random metrics; keep preferences only.
    return {
      version: STORE_VERSION,
      themes,
      settings,
      servers: [],
      logs: [],
      backup: readBackupSummary(parsed.backup),
    };
  } catch {
    return null;
  }
}

export const state = reactive<PersistedState>(loadState() ?? seedState());

export const findServer = (id: string) => state.servers.find((server) => server.id === id);

export const fleet = computed(() => {
  const total = state.servers.length;
  let online = 0;
  let warning = 0;
  let error = 0;
  let offline = 0;
  let connecting = 0;
  let down = 0;
  let up = 0;
  for (const server of state.servers) {
    if (server.status === 'online') online += 1;
    else if (server.status === 'warning') warning += 1;
    else if (server.status === 'error') error += 1;
    else if (server.status === 'connecting') connecting += 1;
    else offline += 1;
    if (server.status === 'online' || server.status === 'warning') {
      down += server.metrics.down;
      up += server.metrics.up;
    }
  }
  return {
    total,
    online,
    warning,
    error,
    offline,
    connecting,
    attention: warning + error,
    sampled: online + warning,
    down: round(down, 1),
    up: round(up, 1),
  };
});

export const unreadEvents = computed(
  () => state.logs.filter((log) => log.status === 'failed' || log.status === 'warning').length,
);

function hydrateStored(stored: StoredState) {
  state.themes = readThemes(stored.themes);
  state.settings = readSettings(stored.settings);
  state.servers.splice(0, state.servers.length, ...readServers(stored.servers));
  state.logs.splice(0, state.logs.length, ...readLogs(stored.logs));
  state.backup = normalizeBackup(stored.backup);
}

export type StorageMode = 'web' | 'sqlite' | 'sqlite-migrated';

export async function bootstrapStore(): Promise<StorageMode> {
  const stored = await loadStoredState();
  if (!stored) return 'web';
  if (stored.initialized) {
    hydrateStored(stored);
    activatePersistence();
    return 'sqlite';
  }

  const legacy = loadState() ?? seedState();
  hydrateStored({
    initialized: false,
    themes: legacy.themes,
    settings: legacy.settings,
    servers: legacy.servers.map(toPersisted),
    logs: legacy.logs,
    backup: legacy.backup,
    render: null,
  });
  try {
    await replaceStoredState({
      themes: { ...state.themes },
      settings: { ...state.settings },
      servers: state.servers.map(toPersisted),
      logs: state.logs.slice(0, MAX_LOGS),
      backup: state.backup,
    });
  } catch {
    return 'web';
  }
  activatePersistence();
  clearLegacyAppKeys();
  return 'sqlite-migrated';
}

let persistTimer: number | undefined;

function schedulePersist() {
  if (isPersistenceActive()) return;
  if (persistTimer) window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    persistTimer = undefined;
    persistNow();
  }, 300);
}

function persistLocal() {
  try {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: STORE_VERSION,
        themes: state.themes,
        settings: state.settings,
        servers: state.servers.map(toPersisted),
        logs: state.logs.slice(0, MAX_LOGS),
        backup: state.backup,
      }),
    );
  } catch {
    /* storage can be unavailable in private mode; the session keeps working */
  }
}

export function persistNow() {
  if (persistTimer) {
    window.clearTimeout(persistTimer);
    persistTimer = undefined;
  }
  if (isPersistenceActive()) {
    void replaceStoredState({
      themes: { ...state.themes },
      settings: { ...state.settings },
      servers: state.servers.map(toPersisted),
      logs: state.logs.slice(0, MAX_LOGS),
      backup: state.backup,
    }).catch(() => {
      /* the write queue already reports the failure once */
    });
    return;
  }
  persistLocal();
}

function persistServer(server: Server) {
  if (isPersistenceActive()) writePersistence({ op: 'serverUpsert', server: toPersisted(server) });
  else schedulePersist();
}

function forgetPersistedServer(id: string) {
  if (isPersistenceActive()) writePersistence({ op: 'serverDelete', id });
  else schedulePersist();
}

function serverMetaFingerprint(server: Server) {
  // 连接轮询会刷新相对时间；只有元数据真正变化时才写库。
  return JSON.stringify({ ...toPersisted(server), lastSync: '', lastSeen: '' });
}

export function addLog(
  input: Pick<LogEntry, 'serverId' | 'action' | 'status'> & {
    duration?: string;
    source?: LogSource;
    code?: string;
    hint?: string;
    session?: string;
  },
): LogEntry {
  const server = findServer(input.serverId) ?? state.servers[0];
  const entry: LogEntry = {
    id: uid('log'),
    ts: Date.now(),
    serverId: server?.id ?? input.serverId,
    serverName: server?.name ?? input.serverId,
    ip: server?.ip ?? '—',
    port: server?.port ?? 22,
    action: input.action,
    status: input.status,
    duration: input.duration ?? '—',
    source: input.source ?? '桌面客户端',
    code: input.code ?? 'OK',
    auth: server?.authLabel ?? '—',
    client: `LightGuard Desktop ${APP_VERSION} · Tauri`,
    session: input.session ?? server?.sessionId ?? '—',
    fingerprint: server?.fingerprint ?? '—',
    hint: input.hint ?? '',
  };
  state.logs.unshift(entry);
  if (state.logs.length > MAX_LOGS) state.logs.length = MAX_LOGS;
  if (isPersistenceActive()) writePersistence({ op: 'logAppend', log: entry });
  else schedulePersist();
  return entry;
}

export function clearLogs() {
  state.logs.splice(0, state.logs.length);
  if (isPersistenceActive()) writePersistence({ op: 'logsClear' });
  else schedulePersist();
}

export function setTheme(scope: keyof ThemeChoices, theme: ThemeId) {
  if (THEME_IDS.has(theme)) state.themes[scope] = theme;
  if (isPersistenceActive()) writePersistence({ op: 'themesSet', themes: { ...state.themes } });
  else schedulePersist();
}

export function updateSettings(patch: Partial<AppSettings>) {
  Object.assign(state.settings, patch);
  if (isPersistenceActive()) writePersistence({ op: 'settingsSet', settings: { ...state.settings } });
  else schedulePersist();
}

export function setBackupSummary(summary: BackupSummary | null) {
  state.backup = summary;
  if (isPersistenceActive()) {
    writePersistence({ op: 'backupSet', summary });
    return;
  }
  try {
    if (summary) window.localStorage.setItem(BACKUP_SUMMARY_KEY, JSON.stringify(summary));
    else window.localStorage.removeItem(BACKUP_SUMMARY_KEY);
  } catch {
    /* summary is non-critical */
  }
  schedulePersist();
}

function durationSince(started: number) {
  return `${((Date.now() - started) / 1000).toFixed(2)}s`;
}

function applyError(server: Server, error: ReturnType<typeof toSshError>) {
  server.status = 'error';
  server.needsAuth = error.code === 'AUTH_FAILED' || error.code === 'CREDENTIAL_ERROR';
  server.error = { code: error.code, message: error.message, hint: error.hint };
}

export interface ConnectOptions {
  remember: boolean;
  verifyHostKey: boolean;
}

export interface ConnectOutcome {
  server: Server;
  result: ConnectResult;
  credentialsSaved: boolean;
}

export async function connectAndAdd(
  draft: ServerDraft,
  auth: BackendAuth,
  secret: string,
  passphrase: string,
  options: ConnectOptions,
): Promise<ConnectOutcome> {
  const serverId = uid('srv');
  const started = Date.now();
  const result = await ssh.connect({
    serverId,
    host: draft.ip.trim(),
    port: draft.port,
    user: draft.user.trim(),
    auth,
    secret,
    passphrase: passphrase || undefined,
    remember: options.remember,
    verifyHostKey: options.verifyHostKey,
  });
  const server = newServer({ serverId, draft, auth }, result);
  state.servers.push(server);
  addLog({
    serverId: server.id,
    action: '连接成功',
    status: 'success',
    duration: durationSince(started),
    code: 'OK',
    session: result.sessionId,
    hint: result.hostKeyState === 'new' ? '首次连接，已记录主机指纹。' : '',
  });
  let credentialsSaved = false;
  if (options.remember) {
    try {
      credentialsSaved = (await ssh.credentialStatus(server.id)).saved;
    } catch {
      credentialsSaved = false;
    }
  }
  persistNow();
  return { server, result, credentialsSaved };
}

export async function refreshServer(id: string): Promise<Server | null> {
  const server = findServer(id);
  if (!server) return null;
  const before = serverMetaFingerprint(server);
  try {
    const snapshot = await ssh.snapshot(id);
    applySnapshot(server, snapshot);
    if (serverMetaFingerprint(server) !== before) persistServer(server);
    return server;
  } catch (raw) {
    const error = toSshError(raw);
    if (error.code === 'SESSION_CLOSED' || error.code === 'SESSION_NOT_FOUND') {
      if (server.status !== 'error') {
        applyError(server, error);
        server.sessionId = '';
        addLog({
          serverId: server.id,
          action: '会话断开',
          status: 'warning',
          code: error.code,
          hint: error.hint,
        });
      }
    }
    return server;
  }
}

export async function retryServer(id: string): Promise<Server | null> {
  const server = findServer(id);
  if (!server) return null;
  if (server.status === 'connecting') return server;
  if (!ssh.available) {
    applyError(
      server,
      toSshError({ code: 'UNSUPPORTED', message: '真实 SSH 仅桌面版可用', hint: '请在桌面客户端中连接。' }),
    );
    return server;
  }
  server.status = 'connecting';
  server.error = null;
  const started = Date.now();
  try {
    const result = await ssh.reconnect({
      serverId: id,
      host: server.ip,
      port: server.port,
      user: server.user,
      verifyHostKey: state.settings.hostKey,
    });
    markConnected(server, result);
    applySnapshot(server, result.snapshot);
    server.needsAuth = false;
    addLog({
      serverId: id,
      action: '重新连接成功',
      status: 'success',
      duration: durationSince(started),
      session: result.sessionId,
    });
    persistServer(server);
    return server;
  } catch (raw) {
    const error = toSshError(raw);
    applyError(server, error);
    addLog({
      serverId: id,
      action: '连接失败',
      status: 'failed',
      duration: durationSince(started),
      code: error.code,
      hint: error.hint,
    });
    return server;
  }
}

export async function updateCredentials(
  id: string,
  auth: BackendAuth,
  secret: string,
  passphrase = '',
): Promise<Server | null> {
  const server = findServer(id);
  if (!server) return null;
  if (!ssh.available) {
    applyError(
      server,
      toSshError({ code: 'UNSUPPORTED', message: '真实 SSH 仅桌面版可用', hint: '请在桌面客户端中连接。' }),
    );
    return server;
  }
  server.status = 'connecting';
  server.error = null;
  const started = Date.now();
  try {
    const result = await ssh.connect({
      serverId: id,
      host: server.ip,
      port: server.port,
      user: server.user,
      auth,
      secret,
      passphrase: passphrase || undefined,
      remember: true,
      verifyHostKey: state.settings.hostKey,
    });
    markConnected(server, result);
    applySnapshot(server, result.snapshot);
    server.needsAuth = false;
    server.auth = auth;
    server.authLabel = AUTH_LABELS[auth];
    addLog({
      serverId: id,
      action: '凭据已更新',
      status: 'success',
      duration: durationSince(started),
      session: result.sessionId,
    });
    persistServer(server);
    return server;
  } catch (raw) {
    const error = toSshError(raw);
    applyError(server, error);
    addLog({
      serverId: id,
      action: '凭据更新失败',
      status: 'failed',
      duration: durationSince(started),
      code: error.code,
      hint: error.hint,
    });
    return server;
  }
}

export interface ServerEditPatch {
  name: string;
  ip: string;
  port: number;
  user: string;
  group: string;
  role: string;
  tags: string[];
  note: string;
}

export interface ServerEditResult {
  server: Server;
  connectionChanged: boolean;
  reconnected: boolean;
}

export async function editServer(id: string, patch: ServerEditPatch): Promise<ServerEditResult | null> {
  const server = findServer(id);
  if (!server) return null;
  const connectionChanged =
    server.ip !== patch.ip || server.port !== patch.port || server.user !== patch.user;
  const wasConnected = server.status === 'online' || server.status === 'warning';

  server.name = patch.name;
  server.ip = patch.ip;
  server.port = patch.port;
  server.user = patch.user;
  server.group = patch.group;
  server.role = patch.role;
  server.tags = patch.tags;
  server.note = patch.note;

  if (!connectionChanged) {
    addLog({ serverId: id, action: '编辑服务器', status: 'success' });
    persistNow();
    return { server, connectionChanged: false, reconnected: false };
  }

  if (!wasConnected) {
    server.status = 'offline';
    server.error = null;
    server.needsAuth = false;
    server.sessionId = '';
    addLog({
      serverId: id,
      action: '编辑服务器',
      status: 'success',
      hint: '连接信息已更新，将在下次连接时生效。',
    });
    persistNow();
    return { server, connectionChanged: true, reconnected: false };
  }

  if (!ssh.available) {
    applyError(
      server,
      toSshError({ code: 'UNSUPPORTED', message: '真实 SSH 仅桌面版可用', hint: '请在桌面客户端中重新连接。' }),
    );
    persistNow();
    return { server, connectionChanged: true, reconnected: false };
  }

  server.status = 'connecting';
  server.error = null;
  const started = Date.now();
  try {
    const result = await ssh.reconnect({
      serverId: id,
      host: server.ip,
      port: server.port,
      user: server.user,
      verifyHostKey: state.settings.hostKey,
    });
    markConnected(server, result);
    applySnapshot(server, result.snapshot);
    server.needsAuth = false;
    addLog({
      serverId: id,
      action: '连接信息已更新',
      status: 'success',
      duration: durationSince(started),
      session: result.sessionId,
    });
    persistNow();
    return { server, connectionChanged: true, reconnected: true };
  } catch (raw) {
    const error = toSshError(raw);
    applyError(server, error);
    addLog({
      serverId: id,
      action: '连接信息更新后重连失败',
      status: 'failed',
      duration: durationSince(started),
      code: error.code,
      hint: error.hint,
    });
    persistNow();
    return { server, connectionChanged: true, reconnected: false };
  }
}

export async function forgetCredentials(id: string) {
  try {
    await ssh.forgetCredentials(id);
  } catch {
    /* nothing to forget */
  }
}

export async function disconnectServer(id: string) {
  const server = findServer(id);
  if (ssh.available) {
    try {
      await ssh.disconnect(id);
    } catch {
      /* session may already be gone */
    }
  }
  if (!server) return;
  server.status = 'offline';
  server.needsAuth = false;
  server.error = null;
  server.sessionId = '';
  server.startedAt = '';
  server.hostKeyState = '';
  server.encryption = null;
  persistNow();
}

export async function removeServer(id: string) {
  const index = state.servers.findIndex((server) => server.id === id);
  if (index >= 0) state.servers.splice(index, 1);
  forgetPersistedServer(id);
  if (!ssh.available) return;
  try {
    await ssh.disconnect(id);
  } catch {
    /* session may already be gone */
  }
  try {
    await ssh.forgetCredentials(id);
  } catch {
    /* credentials may not exist */
  }
}

async function eachLimit<T>(items: T[], limit: number, task: (item: T) => Promise<void>) {
  let index = 0;
  const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (index < items.length) {
      const current = items[index];
      index += 1;
      await task(current);
    }
  });
  await Promise.all(runners);
}

export async function tickAll() {
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
  if (!ssh.available) return;
  const targets = state.servers.filter(
    (server) => server.status === 'online' || server.status === 'warning',
  );
  await eachLimit(targets, 2, async (server) => {
    await refreshServer(server.id);
  });
}

function onStatusEvent(event: StatusEvent) {
  const server = findServer(event.serverId);
  if (!server) return;
  if (event.state === 'closed') {
    if (server.status !== 'error') {
      addLog({
        serverId: server.id,
        action: '会话断开',
        status: 'warning',
        code: 'SESSION_CLOSED',
        hint: 'SSH 会话已断开，请重新连接。',
      });
    }
    server.status = 'error';
    server.needsAuth = false;
    server.error = {
      code: 'SESSION_CLOSED',
      message: 'SSH 会话已断开',
      hint: '请重新连接该服务器。',
    };
    server.sessionId = '';
  } else if (event.state === 'auth-failed') {
    server.status = 'error';
    server.needsAuth = true;
    server.error = {
      code: 'AUTH_FAILED',
      message: '身份认证失败',
      hint: '请更新密码或私钥后重试。',
    };
  }
}

ssh.onStatus(onStatusEvent);

async function importedServer(connection: BackupConnection): Promise<Server> {
  return fromPersisted({
    id: connection.id || uid('srv'),
    name: connection.name,
    role: connection.role || '通用服务器',
    ip: connection.ip,
    port: connection.port,
    region: 'SSH 直连',
    group: connection.group || '未分组',
    tags: connection.tags?.length ? connection.tags : ['自建'],
    note: connection.note ?? '',
    user: connection.user,
    auth: connection.auth,
    authLabel: AUTH_LABELS[connection.auth],
    fingerprint: '—',
    lastSync: '尚未连接',
    lastSeen: '尚未连接',
    cpuCores: 0,
    memTotal: 0,
    diskTotal: 0,
    os: '待读取',
    kernel: '—',
    arch: '—',
    cpuModel: '—',
    cpuFreq: '—',
    timezone: '—',
  });
}

export interface ImportResult {
  added: number;
  updated: number;
  skipped: number;
}

export async function applyBackup(
  payload: BackupPayload,
  strategy: 'merge' | 'replace',
  scope: { settings: boolean; connections: boolean },
): Promise<ImportResult> {
  const result: ImportResult = { added: 0, updated: 0, skipped: 0 };
  const before =
    strategy === 'replace'
      ? JSON.stringify({ servers: state.servers.map(toPersisted), settings: state.settings, themes: state.themes })
      : null;
  try {
    if (scope.connections) {
      const next = strategy === 'replace' ? [] : state.servers.slice();
      const connections = Array.isArray(payload.connections) ? payload.connections : [];
      for (const connection of connections) {
        const sameId = next.find((server) => server.id === connection.id);
        const sameHost = next.find(
          (server) =>
            server.ip === connection.ip &&
            Number(server.port) === Number(connection.port) &&
            server.user === connection.user,
        );
        if (sameId) {
          sameId.name = connection.name;
          sameId.role = connection.role;
          sameId.group = connection.group;
          sameId.tags = connection.tags?.length ? connection.tags : sameId.tags;
          sameId.note = connection.note ?? sameId.note;
          result.updated += 1;
        } else if (sameHost) {
          result.skipped += 1;
        } else {
          next.push(await importedServer(connection));
          result.added += 1;
        }
      }
      state.servers.splice(0, state.servers.length, ...next);
    }
    if (scope.settings) {
      state.settings =
        strategy === 'replace'
          ? readSettings(payload.settings)
          : { ...state.settings, ...readSettings(payload.settings) };
      state.themes = readThemes(payload.themes);
    }
    persistNow();
    return result;
  } catch (error) {
    if (before) {
      const snapshot = JSON.parse(before) as {
        servers: PersistedServer[];
        settings: AppSettings;
        themes: ThemeChoices;
      };
      state.servers.splice(0, state.servers.length, ...snapshot.servers.map(fromPersisted));
      state.settings = snapshot.settings;
      state.themes = snapshot.themes;
    }
    throw error;
  }
}

export async function clearLocalData() {
  stopLiveUpdates();
  if (ssh.available) {
    try {
      await ssh.resetLocalData();
    } catch {
      /* clear what we can */
    }
  }
  state.servers.splice(0, state.servers.length);
  state.logs.splice(0, state.logs.length);
  state.themes = { ...DEFAULT_THEMES };
  state.settings = { ...DEFAULT_SETTINGS };
  state.backup = null;
  try {
    window.localStorage.removeItem(BACKUP_SUMMARY_KEY);
  } catch {
    /* ignore */
  }
  if (isPersistenceActive()) {
    try {
      await resetStoredState();
    } catch {
      /* the in-memory reset still takes effect */
    }
  }
  resetRenderSettings();
  persistNow();
  restartLiveUpdates();
}

let liveTimer: number | undefined;

export function restartLiveUpdates() {
  stopLiveUpdates();
  if (!state.settings.autoRefresh) return;
  const ms = Math.max(2, state.settings.interval) * 1000;
  liveTimer = window.setInterval(() => {
    void tickAll();
  }, ms);
}

export function stopLiveUpdates() {
  if (liveTimer) {
    window.clearInterval(liveTimer);
    liveTimer = undefined;
  }
}

export type { BackendAuth, ConnectResult, ServerSnapshot };
