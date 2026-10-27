import { computed, reactive, watch } from 'vue';
import { themeOptions } from '@lightguard/lightui';
import type { ThemeId } from '@lightguard/lightui';
import {
  APP_VERSION,
  AUTH_LABELS,
  MAX_LOGS,
  buildLogs,
  buildServers,
  clamp,
  defaultDisks,
  defaultInterfaces,
  historySeries,
  makeServer,
  rand,
  round,
  uid,
} from './data';
import type {
  AppSettings,
  BackupConnection,
  BackupPayload,
  BackupSummary,
  LogEntry,
  LogSource,
  PersistedState,
  Server,
  ServerDraft,
  ThemeChoices,
} from './types';

const STORE_KEY = 'lightguard.app.v1';
const STORE_VERSION = 1;
const BACKUP_SUMMARY_KEY = 'lightguard.backup.last';

const THEME_IDS = new Set<string>(themeOptions.map((option) => option.id));

export const DEFAULT_SETTINGS: AppSettings = {
  interval: 5,
  autoRefresh: true,
  hostKey: true,
  clipboardClear: true,
  notifyFailure: true,
  notifyThreshold: true,
  idleLock: 30,
};

export const DEFAULT_THEMES: ThemeChoices = {
  console: 'dark',
  desktop: 'pixel',
};

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
  };
}

function readThemes(value: unknown): ThemeChoices {
  const raw = (value ?? {}) as Partial<Record<keyof ThemeChoices, unknown>>;
  return {
    console: readTheme(raw.console, DEFAULT_THEMES.console),
    desktop: readTheme(raw.desktop, DEFAULT_THEMES.desktop),
  };
}

function readBackupSummary(fallback: unknown): BackupSummary | null {
  try {
    const raw = localStorage.getItem(BACKUP_SUMMARY_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<BackupSummary> | null;
      if (
        parsed &&
        typeof parsed.at === 'string' &&
        typeof parsed.hosts === 'number' &&
        typeof parsed.encrypted === 'boolean'
      ) {
        return { at: parsed.at, hosts: parsed.hosts, encrypted: parsed.encrypted };
      }
    }
  } catch {
    /* fall through to the app-state snapshot */
  }
  const candidate = fallback as Partial<BackupSummary> | null | undefined;
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

function seedState(): PersistedState {
  const servers = buildServers();
  return {
    version: STORE_VERSION,
    themes: { ...DEFAULT_THEMES },
    settings: { ...DEFAULT_SETTINGS },
    servers,
    logs: buildLogs(servers),
    backup: null,
  };
}

function loadState(): PersistedState | null {
  try {
    const raw = localStorage.getItem(STORE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PersistedState> | null;
    if (!parsed || parsed.version !== STORE_VERSION) return null;
    if (!Array.isArray(parsed.servers) || !parsed.servers.length) return null;
    const servers = parsed.servers as Server[];
    return {
      version: STORE_VERSION,
      themes: readThemes(parsed.themes),
      settings: readSettings(parsed.settings),
      servers,
      logs:
        Array.isArray(parsed.logs) && parsed.logs.length
          ? (parsed.logs as LogEntry[])
              .slice(0, MAX_LOGS)
              .map((log) => ({ ...log, port: typeof log.port === 'number' ? log.port : 22 }))
          : buildLogs(servers),
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
  let failed = 0;
  let connecting = 0;
  let cpu = 0;
  let mem = 0;
  let down = 0;
  let up = 0;
  let sampled = 0;
  const points = 24;
  const cpuSeries = new Array<number>(points).fill(0);
  const memSeries = new Array<number>(points).fill(0);
  for (const server of state.servers) {
    if (server.status === 'online') online += 1;
    else if (server.status === 'warning') warning += 1;
    else if (server.status === 'connecting') connecting += 1;
    else failed += 1;
    if (server.status === 'online' || server.status === 'warning') {
      cpu += server.metrics.cpu;
      mem += server.metrics.mem;
      down += server.metrics.down;
      up += server.metrics.up;
      sampled += 1;
      const cpuTail = server.history.cpu.slice(-points);
      const memTail = server.history.mem.slice(-points);
      const cpuOffset = points - cpuTail.length;
      const memOffset = points - memTail.length;
      for (let i = 0; i < points; i += 1) {
        cpuSeries[i] += i >= cpuOffset ? cpuTail[i - cpuOffset] ?? 0 : 0;
        memSeries[i] += i >= memOffset ? memTail[i - memOffset] ?? 0 : 0;
      }
    }
  }
  if (sampled) {
    for (let i = 0; i < points; i += 1) {
      cpuSeries[i] = round(cpuSeries[i] / sampled, 1);
      memSeries[i] = round(memSeries[i] / sampled, 1);
    }
  }
  return {
    total,
    online,
    warning,
    failed,
    connecting,
    healthy: online,
    attention: warning + failed,
    avgCpu: sampled ? Math.round(cpu / sampled) : 0,
    avgMem: sampled ? Math.round(mem / sampled) : 0,
    down: round(down, 1),
    up: round(up, 1),
    sampled,
    cpuSeries,
    memSeries,
  };
});

export const unreadEvents = computed(
  () => state.logs.filter((log) => log.status === 'failed' || log.status === 'warning').length,
);

let persistTimer: number | undefined;

function schedulePersist() {
  if (persistTimer) window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    persistTimer = undefined;
    persistNow();
  }, 300);
}

export function persistNow() {
  if (persistTimer) {
    window.clearTimeout(persistTimer);
    persistTimer = undefined;
  }
  try {
    localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: STORE_VERSION,
        themes: state.themes,
        settings: state.settings,
        servers: state.servers,
        logs: state.logs.slice(0, MAX_LOGS),
        backup: state.backup,
      }),
    );
  } catch {
    /* storage can be unavailable in private mode; the session keeps working */
  }
}

watch(
  () => [state.themes, state.settings, state.servers, state.logs, state.backup],
  schedulePersist,
  { deep: true },
);

function jitter(value: number, delta: number, min: number, max: number) {
  return round(clamp(value + rand(-delta, delta), min, max), 1);
}

export function tickServer(server: Server) {
  if (server.status !== 'online' && server.status !== 'warning') return;
  const metrics = server.metrics;
  metrics.cpu = jitter(metrics.cpu, 4, 2, 97);
  metrics.mem = jitter(metrics.mem, 2.4, 6, 96);
  metrics.down = jitter(metrics.down, Math.max(0.6, metrics.down * 0.12), 0, 900);
  metrics.up = jitter(metrics.up, Math.max(0.3, metrics.up * 0.12), 0, 400);
  server.history.cpu.push(metrics.cpu);
  server.history.mem.push(metrics.mem);
  server.history.net.push(metrics.down);
  if (server.history.cpu.length > 120) server.history.cpu.shift();
  if (server.history.mem.length > 120) server.history.mem.shift();
  if (server.history.net.length > 120) server.history.net.shift();
  server.latency = Math.round(jitter(server.latency, 3, 8, 120));
  server.load = [
    round(clamp(server.load[0] + rand(-0.12, 0.12), 0, 99), 2),
    round(clamp(server.load[1] + rand(-0.08, 0.08), 0, 99), 2),
    round(clamp(server.load[2] + rand(-0.05, 0.05), 0, 99), 2),
  ];
  server.temp = Math.round(jitter(server.temp, 1, 28, 85));
  server.lastSync = '刚刚';
  if (server.status === 'online' && (metrics.cpu > 93 || metrics.mem > 95)) {
    server.status = 'warning';
    server.warning = `资源使用率偏高：CPU ${metrics.cpu}% · 内存 ${metrics.mem}%。`;
  }
}

export function tickAll() {
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
  for (const server of state.servers) tickServer(server);
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
    source: input.source ?? 'Web',
    code: input.code ?? 'OK',
    auth: server?.authLabel ?? '—',
    client: `LightGuard Web ${APP_VERSION} · Chrome`,
    session: input.session ?? `lg_${Math.random().toString(36).slice(2, 12)}`,
    fingerprint: server?.fingerprint ?? '—',
    hint: input.hint ?? '',
  };
  state.logs.unshift(entry);
  if (state.logs.length > MAX_LOGS) state.logs.length = MAX_LOGS;
  return entry;
}

export function clearLogs() {
  state.logs.splice(0, state.logs.length);
}

export function setTheme(scope: keyof ThemeChoices, theme: ThemeId) {
  if (!THEME_IDS.has(theme)) return;
  state.themes[scope] = theme;
}

export function updateSettings(patch: Partial<AppSettings>) {
  Object.assign(state.settings, patch);
}

export function setBackupSummary(summary: BackupSummary | null) {
  state.backup = summary;
  try {
    if (summary) localStorage.setItem(BACKUP_SUMMARY_KEY, JSON.stringify(summary));
    else localStorage.removeItem(BACKUP_SUMMARY_KEY);
  } catch {
    /* summary is non-critical */
  }
}

export function generateFingerprint() {
  const groups: string[] = [];
  for (let i = 0; i < 8; i += 1) {
    groups.push(Math.random().toString(16).slice(2, 6).padEnd(4, '0'));
  }
  return `SHA256:${groups.join(' ')}`;
}

function draftMetrics() {
  return {
    cpu: Math.round(rand(14, 38)),
    mem: Math.round(rand(28, 58)),
    disk: Math.round(rand(16, 46)),
    down: round(rand(1, 9), 1),
    up: round(rand(0.4, 3.2), 1),
  };
}

export function addServer(draft: ServerDraft): Server {
  const metrics = draftMetrics();
  const server = makeServer({
    id: uid('srv'),
    name: draft.name.trim() || draft.ip,
    ip: draft.ip.trim(),
    port: draft.port,
    user: draft.user.trim() || 'deploy',
    auth: draft.auth,
    role: draft.role.trim() || '通用服务器',
    group: draft.group.trim() || '未分组',
    tags: draft.tags.length ? draft.tags : ['自建'],
    note: draft.note.trim(),
    os: 'Ubuntu 24.04.2 LTS',
    kernel: '6.8.0-45-generic',
    cpuModel: 'AMD EPYC 7R32',
    cpuCores: 4,
    memTotal: 8,
    diskTotal: 160,
    uptime: '1 天 2 小时',
    bootTime: '2026-10-26 09:18',
    lastSync: '刚刚',
    latency: Math.round(rand(14, 46)),
    load: [0.24, 0.31, 0.28],
    temp: 42,
    metrics,
    disks: defaultDisks(160, metrics.disk),
    interfaces: defaultInterfaces(draft.ip.trim(), metrics),
    fingerprint: generateFingerprint(),
  });
  server.status = 'online';
  state.servers.push(server);
  addLog({ serverId: server.id, action: '连接成功', status: 'success', duration: '1.24s' });
  return server;
}

export function removeServer(id: string) {
  const index = state.servers.findIndex((server) => server.id === id);
  if (index >= 0) state.servers.splice(index, 1);
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

export async function retryServer(id: string): Promise<Server | null> {
  const server = findServer(id);
  if (!server || server.status === 'connecting') return server ?? null;
  server.status = 'connecting';
  server.error = null;
  await sleep(1100 + Math.random() * 700);
  if (server.needsAuth) {
    server.status = 'error';
    server.error = {
      code: 'AUTH_FAILED',
      message: '身份认证失败',
      hint: '凭据可能已轮换，请更新密码或私钥后重试。',
    };
    addLog({
      serverId: server.id,
      action: '连接失败',
      status: 'failed',
      duration: '2.36s',
      code: 'AUTH_FAILED',
      hint: '服务器拒绝身份认证。请更新凭据后重试。',
    });
    return server;
  }
  server.status = 'online';
  server.error = null;
  server.warning = '';
  server.lastSync = '刚刚';
  server.lastSeen = '刚刚';
  server.uptime = server.uptime === '—' ? '0 天 1 小时' : server.uptime;
  server.bootTime = server.bootTime === '—' ? '刚刚' : server.bootTime;
  const metrics = server.metrics;
  metrics.cpu = Math.round(rand(16, 34));
  metrics.mem = Math.round(rand(28, 52));
  metrics.down = round(rand(1, 8), 1);
  metrics.up = round(rand(0.4, 3), 1);
  server.history = {
    cpu: historySeries(metrics.cpu, 4),
    mem: historySeries(metrics.mem, 3),
    net: historySeries(metrics.down, 1.4, 0.2, 90),
  };
  addLog({ serverId: server.id, action: '重新连接成功', status: 'success', duration: '1.08s' });
  return server;
}

export function updateCredentials(id: string, auth: Server['auth']) {
  const server = findServer(id);
  if (!server) return null;
  server.auth = auth;
  server.authLabel = AUTH_LABELS[auth];
  server.needsAuth = false;
  server.error = null;
  server.status = 'online';
  server.lastSync = '刚刚';
  server.lastSeen = '刚刚';
  server.uptime = server.uptime === '—' ? '0 天 1 小时' : server.uptime;
  server.bootTime = server.bootTime === '—' ? '刚刚' : server.bootTime;
  const metrics = server.metrics;
  metrics.cpu = Math.round(rand(12, 30));
  metrics.mem = Math.round(rand(24, 46));
  metrics.down = round(rand(0.8, 6), 1);
  metrics.up = round(rand(0.3, 2.4), 1);
  server.history = {
    cpu: historySeries(metrics.cpu, 4),
    mem: historySeries(metrics.mem, 3),
    net: historySeries(metrics.down, 1.2, 0.2, 90),
  };
  addLog({ serverId: server.id, action: '凭据已更新', status: 'success', duration: '0.42s' });
  return server;
}

function importedServer(connection: BackupConnection): Server {
  const auth = connection.auth;
  const metrics = { cpu: 0, mem: 0, disk: 0, down: 0, up: 0 };
  const server = makeServer({
    id: connection.id || uid('srv'),
    name: connection.name,
    role: connection.role || '通用服务器',
    ip: connection.ip,
    port: connection.port,
    user: connection.user,
    auth,
    group: connection.group || '未分组',
    tags: connection.tags?.length ? connection.tags : ['自建'],
    note: connection.note ?? '',
    status: 'offline',
    os: '待读取',
    kernel: '—',
    cpuModel: '—',
    cpuCores: 0,
    memTotal: 0,
    diskTotal: 0,
    uptime: '—',
    bootTime: '—',
    lastSync: '尚未连接',
    lastSeen: '尚未连接',
    latency: 0,
    load: [0, 0, 0],
    temp: 0,
    metrics,
    disks: [],
    interfaces: [],
    processes: [],
    gateway: '—',
    dns: '—',
    publicIp: '—',
    fingerprint: '—',
    needsAuth: true,
  });
  server.history = {
    cpu: new Array(120).fill(0),
    mem: new Array(120).fill(0),
    net: new Array(120).fill(0),
  };
  return server;
}

export interface ImportResult {
  added: number;
  updated: number;
  skipped: number;
}

export function applyBackup(
  payload: BackupPayload,
  strategy: 'merge' | 'replace',
  scope: { settings: boolean; connections: boolean },
): ImportResult {
  const result: ImportResult = { added: 0, updated: 0, skipped: 0 };
  const before = strategy === 'replace' ? JSON.stringify({ servers: state.servers, settings: state.settings, themes: state.themes }) : null;
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
          next.push(importedServer(connection));
          result.added += 1;
        }
      }
      state.servers.splice(0, state.servers.length, ...next);
    }
    if (scope.settings) {
      state.settings = strategy === 'replace' ? readSettings(payload.settings) : { ...state.settings, ...readSettings(payload.settings) };
      state.themes = readThemes(payload.themes);
    }
    return result;
  } catch (error) {
    if (before) {
      const snapshot = JSON.parse(before) as {
        servers: Server[];
        settings: AppSettings;
        themes: ThemeChoices;
      };
      state.servers.splice(0, state.servers.length, ...snapshot.servers);
      state.settings = snapshot.settings;
      state.themes = snapshot.themes;
    }
    throw error;
  }
}

export function resetDemo() {
  const seeded = seedState();
  state.themes = seeded.themes;
  state.settings = seeded.settings;
  state.servers.splice(0, state.servers.length, ...seeded.servers);
  state.logs.splice(0, state.logs.length, ...seeded.logs);
  state.backup = null;
  try {
    localStorage.removeItem(BACKUP_SUMMARY_KEY);
  } catch {
    /* ignore */
  }
  persistNow();
}

let liveTimer: number | undefined;

export function restartLiveUpdates() {
  stopLiveUpdates();
  if (!state.settings.autoRefresh) return;
  const ms = Math.max(2, state.settings.interval) * 1000;
  liveTimer = window.setInterval(tickAll, ms);
}

export function stopLiveUpdates() {
  if (liveTimer) {
    window.clearInterval(liveTimer);
    liveTimer = undefined;
  }
}
