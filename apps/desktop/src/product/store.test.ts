import { beforeEach, describe, expect, it, vi } from 'vitest';
import { buildBackupPayload } from './backup';
import { fromPersisted } from './ssh/adapter';
import type { LogEntry, PersistedServer } from './types';

const STORE_KEY = 'lightguard.app.v1';

async function loadStore() {
  vi.resetModules();
  return import('./store');
}

function persistedServer(overrides: Record<string, unknown> = {}) {
  return {
    id: 'srv_1',
    name: '生产 Web',
    hostname: 'web-prod-01',
    role: 'Web',
    ip: '192.168.5.4',
    port: 22,
    region: 'SSH 直连',
    group: '生产',
    tags: ['web'],
    note: '',
    user: 'admin',
    auth: 'password',
    authLabel: '密码',
    fingerprint: 'SHA256:abc',
    lastSync: '刚刚',
    lastSeen: '刚刚',
    cpuCores: 4,
    memTotal: 16,
    diskTotal: 240,
    os: 'Debian 12',
    kernel: '6.1.0',
    arch: 'x86_64',
    cpuModel: 'Xeon',
    cpuFreq: '2.4 GHz',
    timezone: 'CST',
    ...overrides,
  };
}

beforeEach(async () => {
  // 上一个用例留下的 300ms 防抖写入会在本用例夹具落盘后触发；先取消它的计时器。
  (await import('./store')).cancelPendingPersist();
  window.localStorage.clear();
});

describe('store migration', () => {
  it('starts empty on first launch', async () => {
    const store = await loadStore();
    expect(store.state.servers).toEqual([]);
    expect(store.state.logs).toEqual([]);
    expect(store.fleet.value.total).toBe(0);
  });

  it('drops v1 demo servers and keeps themes and settings', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 1,
        themes: { console: 'claude', desktop: 'nes' },
        settings: { interval: 12, autoRefresh: false, hostKey: false },
        servers: [{ id: 'demo-1', name: '演示主机', ip: '10.0.0.1' }],
        logs: [{ id: 'log-1', ts: Date.now(), action: '连接成功' }],
      }),
    );

    const store = await loadStore();

    expect(store.state.version).toBe(2);
    expect(store.state.servers).toEqual([]);
    expect(store.state.logs).toEqual([]);
    expect(store.state.themes).toEqual({ console: 'claude', desktop: 'nes' });
    expect(store.state.settings.interval).toBe(12);
    expect(store.state.settings.autoRefresh).toBe(false);
    expect(store.state.settings.hostKey).toBe(false);
  });

  it('hydrates v2 metadata as offline hosts without live metrics', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        themes: { console: 'dark', desktop: 'pixel' },
        settings: { interval: 5 },
        servers: [persistedServer()],
        logs: [],
      }),
    );

    const store = await loadStore();
    expect(store.state.servers).toHaveLength(1);

    const [server] = store.state.servers;
    expect(server.hostname).toBe('web-prod-01');
    expect(server.status).toBe('offline');
    expect(server.sessionId).toBe('');
    expect(server.metrics).toEqual({ cpu: 0, mem: 0, disk: 0, down: 0, up: 0 });
    expect(server.samples).toEqual([]);
    expect(server.disks).toEqual([]);
    expect(server.error).toBeNull();
  });

  it('falls back to a clean state when storage is corrupt', async () => {
    window.localStorage.setItem(STORE_KEY, '{not-json');
    const store = await loadStore();
    expect(store.state.servers).toEqual([]);
    expect(store.state.themes).toEqual(store.DEFAULT_THEMES);
  });
});

describe('server editing', () => {
  it('updates metadata without disturbing a live session', async () => {
    const store = await loadStore();
    const server = fromPersisted(persistedServer() as unknown as PersistedServer);
    server.status = 'online';
    server.sessionId = 'sess_1';
    store.state.servers.push(server);

    const result = await store.editServer(server.id, {
      name: '生产 API',
      ip: server.ip,
      port: server.port,
      user: server.user,
      group: '预发',
      role: 'API',
      tags: ['api', 'prod'],
      note: '更新后的备注',
    });

    expect(result?.connectionChanged).toBe(false);
    expect(result?.server.status).toBe('online');
    expect(result?.server.sessionId).toBe('sess_1');
    expect(server.name).toBe('生产 API');
    expect(server.group).toBe('预发');
    expect(server.tags).toEqual(['api', 'prod']);
    const parsed = JSON.parse(window.localStorage.getItem(STORE_KEY) as string);
    expect(parsed.servers[0].name).toBe('生产 API');
    expect(parsed.servers[0].group).toBe('预发');
  });

  it('keeps an offline host offline when connection details change', async () => {
    const store = await loadStore();
    const server = fromPersisted(persistedServer() as unknown as PersistedServer);
    store.state.servers.push(server);

    const result = await store.editServer(server.id, {
      name: server.name,
      ip: '192.168.5.5',
      port: 2222,
      user: 'deploy',
      group: server.group,
      role: server.role,
      tags: server.tags,
      note: server.note,
    });

    expect(result?.connectionChanged).toBe(true);
    expect(result?.server.status).toBe('offline');
    expect(result?.server.sessionId).toBe('');
    expect(server.ip).toBe('192.168.5.5');
    expect(server.port).toBe(2222);
    expect(server.user).toBe('deploy');
  });
});

describe('window close behavior', () => {
  it('defaults to asking on every close', async () => {
    const store = await loadStore();
    expect(store.state.settings.closeBehavior).toBe('ask');
  });

  it('restores a remembered behavior and rejects unknown values', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({ version: 2, settings: { closeBehavior: 'tray' }, servers: [], logs: [] }),
    );
    let store = await loadStore();
    expect(store.state.settings.closeBehavior).toBe('tray');

    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({ version: 2, settings: { closeBehavior: 'always' }, servers: [], logs: [] }),
    );
    store = await loadStore();
    expect(store.state.settings.closeBehavior).toBe('ask');
  });

  it('persists a newly chosen behavior for the next launch', async () => {
    const store = await loadStore();
    store.updateSettings({ closeBehavior: 'quit' });
    store.persistNow();

    const parsed = JSON.parse(window.localStorage.getItem(STORE_KEY) as string);
    expect(parsed.settings.closeBehavior).toBe('quit');
  });

  it('normalizes the update source on write and load', async () => {
    let store = await loadStore();
    store.updateSettings({ updateSource: 'http://mirror.example.com/latest.json' });
    expect(store.state.settings.updateSource).toBe('auto');

    store.updateSettings({ updateSource: 'https://mirror.example.com/latest.json' });
    expect(store.state.settings.updateSource).toBe('https://mirror.example.com/latest.json');
    store.cancelPendingPersist();

    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        settings: { updateSource: 'http://mirror.example.com/latest.json' },
        servers: [],
        logs: [],
      }),
    );
    store = await loadStore();
    expect(store.state.settings.updateSource).toBe('auto');
  });

  it('flushes the remembered behavior before quitting', async () => {
    const store = await loadStore();
    store.updateSettings({ closeBehavior: 'tray' });
    await store.flushSettings();

    const parsed = JSON.parse(window.localStorage.getItem(STORE_KEY) as string);
    expect(parsed.settings.closeBehavior).toBe('tray');
  });
});

describe('desktop status bar mode', () => {
  it('defaults to a floating bar', async () => {
    const store = await loadStore();
    expect(store.state.settings.desktopBar).toBe('floating');
  });

  it('restores a saved full-width bar and rejects unknown values', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({ version: 2, settings: { desktopBar: 'full' }, servers: [], logs: [] }),
    );
    let store = await loadStore();
    expect(store.state.settings.desktopBar).toBe('full');

    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({ version: 2, settings: { desktopBar: 'wide' }, servers: [], logs: [] }),
    );
    store = await loadStore();
    expect(store.state.settings.desktopBar).toBe('floating');
  });

  it('persists a switched mode for the next launch', async () => {
    const store = await loadStore();
    store.updateSettings({ desktopBar: 'full' });
    store.persistNow();

    const parsed = JSON.parse(window.localStorage.getItem(STORE_KEY) as string);
    expect(parsed.settings.desktopBar).toBe('full');
  });
});

describe('desktop dock mode', () => {
  it('defaults to a full-width dock', async () => {
    const store = await loadStore();
    expect(store.state.settings.desktopDock).toBe('full');
  });

  it('restores a saved floating dock and rejects unknown values', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({ version: 2, settings: { desktopDock: 'floating' }, servers: [], logs: [] }),
    );
    let store = await loadStore();
    expect(store.state.settings.desktopDock).toBe('floating');

    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({ version: 2, settings: { desktopDock: 'wide' }, servers: [], logs: [] }),
    );
    store = await loadStore();
    expect(store.state.settings.desktopDock).toBe('full');
  });

  it('persists a switched dock mode for the next launch', async () => {
    const store = await loadStore();
    store.updateSettings({ desktopDock: 'floating' });
    store.persistNow();

    const parsed = JSON.parse(window.localStorage.getItem(STORE_KEY) as string);
    expect(parsed.settings.desktopDock).toBe('floating');
  });
});

describe('desktop chrome auto hide', () => {
  it('defaults to keeping the bar and dock visible', async () => {
    const store = await loadStore();
    expect(store.state.settings.desktopBarAutoHide).toBe(false);
    expect(store.state.settings.desktopDockAutoHide).toBe(false);
  });

  it('restores saved flags and rejects non-boolean values', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        settings: { desktopBarAutoHide: true, desktopDockAutoHide: true },
        servers: [],
        logs: [],
      }),
    );
    let store = await loadStore();
    expect(store.state.settings.desktopBarAutoHide).toBe(true);
    expect(store.state.settings.desktopDockAutoHide).toBe(true);

    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        settings: { desktopBarAutoHide: 'yes', desktopDockAutoHide: 1 },
        servers: [],
        logs: [],
      }),
    );
    store = await loadStore();
    expect(store.state.settings.desktopBarAutoHide).toBe(false);
    expect(store.state.settings.desktopDockAutoHide).toBe(false);
  });

  it('persists a switched flag for the next launch', async () => {
    const store = await loadStore();
    store.updateSettings({ desktopDockAutoHide: true });
    store.persistNow();

    const parsed = JSON.parse(window.localStorage.getItem(STORE_KEY) as string);
    expect(parsed.settings.desktopDockAutoHide).toBe(true);
  });
});

describe('permit and alert settings', () => {
  it('normalizes nested values from persisted state', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        settings: {
          permit: { defaultMinutes: 30 },
          alerts: { rules: { cpu: { threshold: 500 } } },
        },
        servers: [],
        logs: [],
      }),
    );
    const store = await loadStore();
    expect(store.state.settings.permit.defaultMinutes).toBe(30);
    expect(store.state.settings.alerts.rules.cpu.threshold).toBe(100);
    expect(store.state.settings.alerts.rules.memory.threshold).toBe(90);
  });
});

describe('persistence', () => {
  it('persists only whitelisted metadata', async () => {
    const store = await loadStore();
    const server = fromPersisted(persistedServer() as unknown as PersistedServer);
    server.metrics = { cpu: 88, mem: 70, disk: 50, down: 1, up: 1 };
    server.sessionId = 'sess_1';
    store.state.servers.push(server);

    store.persistNow();

    const raw = window.localStorage.getItem(STORE_KEY);
    expect(raw).toBeTruthy();
    const parsed = JSON.parse(raw as string);
    expect(parsed.version).toBe(2);
    expect(parsed.servers[0].sessionId).toBeUndefined();
    expect(parsed.servers[0].metrics).toBeUndefined();
    expect(parsed.servers[0].fingerprint).toBe('SHA256:abc');
  });
});

it('normalizes autostart to false unless it is a real boolean', async () => {
  window.localStorage.setItem(
    STORE_KEY,
    JSON.stringify({
      version: 2,
      themes: { console: 'dark', desktop: 'pixel' },
      settings: { autostart: 'yes' },
      servers: [],
      logs: [],
    }),
  );

  const store = await loadStore();
  expect(store.state.settings.autostart).toBe(false);
});

it('keeps the local autostart preference when importing a backup', async () => {
  const store = await loadStore();
  store.updateSettings({ autostart: true });
  const payload = buildBackupPayload({
    settings: { ...store.DEFAULT_SETTINGS, autostart: false, interval: 9 },
    themes: store.state.themes,
    servers: [],
    includeSettings: true,
    includeConnections: false,
  });

  await store.applyBackup(payload, 'replace', { settings: true, connections: false });

  expect(store.state.settings.interval).toBe(9);
  expect(store.state.settings.autostart).toBe(true);
});

function logEntry(overrides: Partial<LogEntry> = {}): LogEntry {
  return {
    id: 'log_1',
    ts: 1_000,
    serverId: 'srv_1',
    serverName: '生产 Web',
    ip: '192.168.5.4',
    port: 22,
    action: '连接',
    status: 'failed',
    duration: '1.2s',
    source: '桌面客户端',
    code: 'AUTH_FAILED',
    auth: '密码',
    client: 'LightGuard Desktop',
    session: '—',
    fingerprint: 'SHA256:abc',
    hint: '',
    ...overrides,
  };
}

describe('connection log unread badge', () => {
  it('counts only exceptions newer than the seen watermark', async () => {
    const store = await loadStore();
    store.state.logs.splice(
      0,
      0,
      logEntry({ id: 'log_failed', ts: 1_000, status: 'failed' }),
      logEntry({ id: 'log_warning', ts: 2_000, status: 'warning' }),
      logEntry({ id: 'log_success', ts: 3_000, status: 'success' }),
    );

    expect(store.unreadEvents.value).toBe(2);
    store.updateSettings({ logsSeenAt: 1_500 });
    expect(store.unreadEvents.value).toBe(1);
    store.updateSettings({ logsSeenAt: 3_000 });
    expect(store.unreadEvents.value).toBe(0);
  });

  it('clears the badge on view and counts failures that arrive afterwards', async () => {
    const store = await loadStore();
    store.state.logs.splice(
      0,
      0,
      logEntry({ id: 'log_failed', ts: 1_000 }),
      logEntry({ id: 'log_warning', ts: 2_000, status: 'warning' }),
    );

    store.markLogsSeen(2_500);
    expect(store.unreadEvents.value).toBe(0);
    expect(store.state.settings.logsSeenAt).toBe(2_500);

    store.state.logs.unshift(logEntry({ id: 'log_new', ts: 3_000 }));
    expect(store.unreadEvents.value).toBe(1);
  });

  it('uses the newest exception timestamp when the clock is behind', async () => {
    const store = await loadStore();
    store.state.logs.splice(0, 0, logEntry({ id: 'log_failed', ts: 2_000 }));

    store.markLogsSeen(500);
    expect(store.state.settings.logsSeenAt).toBe(2_000);
    expect(store.unreadEvents.value).toBe(0);
  });

  it('keeps the local watermark when importing a backup', async () => {
    const store = await loadStore();
    store.updateSettings({ logsSeenAt: 2_000 });
    const payload = buildBackupPayload({
      settings: { ...store.DEFAULT_SETTINGS, interval: 9 },
      themes: store.state.themes,
      servers: [],
      includeSettings: true,
      includeConnections: false,
    });

    expect((payload.settings as Record<string, unknown>).logsSeenAt).toBeUndefined();

    await store.applyBackup(payload, 'replace', { settings: true, connections: false });

    expect(store.state.settings.interval).toBe(9);
    expect(store.state.settings.logsSeenAt).toBe(2_000);
  });
});
