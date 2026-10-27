import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fromPersisted } from './ssh/adapter';
import type { PersistedServer } from './types';

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

beforeEach(() => {
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
