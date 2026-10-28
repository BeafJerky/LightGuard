import { beforeEach, describe, expect, it, vi } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const STORE_KEY = 'lightguard.app.v1';
const BACKUP_KEY = 'lightguard.backup.last';

const persistedServer = {
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
};

function enableTauri() {
  (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
}

function disableTauri() {
  delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
}

function mockLoad(result: unknown) {
  invoke.mockReset();
  invoke.mockImplementation(async (command: string) => {
    if (command === 'db_load') return result;
    return undefined;
  });
}

async function loadModules() {
  vi.resetModules();
  const store = await import('./store');
  return { store };
}

beforeEach(async () => {
  // 上一个用例留下的 300ms 防抖写入会在本用例夹具落盘后触发；先取消它的计时器。
  (await import('./store')).cancelPendingPersist();
  window.localStorage.clear();
  invoke.mockReset();
  disableTauri();
});

describe('web fallback', () => {
  it('keeps localStorage persistence when Tauri is unavailable', async () => {
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        themes: { console: 'claude', desktop: 'nes' },
        settings: { interval: 12 },
        servers: [persistedServer],
        logs: [],
      }),
    );

    const { store } = await loadModules();
    await expect(store.bootstrapStore()).resolves.toBe('web');
    expect(store.state.servers).toHaveLength(1);
    expect(window.localStorage.getItem(STORE_KEY)).toBeTruthy();
    expect(invoke).not.toHaveBeenCalled();
  });
});

describe('sqlite bootstrap', () => {
  it('hydrates themes, settings and offline hosts from the database', async () => {
    enableTauri();
    mockLoad({
      initialized: true,
      themes: { console: 'claude', desktop: 'nes' },
      settings: { interval: 12, autoRefresh: false },
      servers: [persistedServer],
      logs: [],
      backup: null,
      render: null,
    });

    const { store } = await loadModules();
    await expect(store.bootstrapStore()).resolves.toBe('sqlite');

    expect(store.state.themes).toEqual({ console: 'claude', desktop: 'nes' });
    expect(store.state.settings.interval).toBe(12);
    expect(store.state.settings.autoRefresh).toBe(false);
    expect(store.state.servers).toHaveLength(1);
    expect(store.state.servers[0].status).toBe('offline');
    expect(store.state.servers[0].metrics).toEqual({ cpu: 0, mem: 0, disk: 0, down: 0, up: 0 });
  });

  it('imports legacy localStorage once and removes the legacy keys', async () => {
    enableTauri();
    window.localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        version: 2,
        themes: { console: 'dark', desktop: 'pixel' },
        settings: { interval: 5 },
        servers: [persistedServer],
        logs: [],
        backup: { at: '2026-10-27T00:00:00Z', hosts: 1, encrypted: false },
      }),
    );
    window.localStorage.setItem(
      BACKUP_KEY,
      JSON.stringify({ at: '2026-10-27T00:00:00Z', hosts: 1, encrypted: false }),
    );
    mockLoad({
      initialized: false,
      themes: null,
      settings: null,
      servers: [],
      logs: [],
      backup: null,
      render: null,
    });

    const { store } = await loadModules();
    await expect(store.bootstrapStore()).resolves.toBe('sqlite-migrated');

    const write = invoke.mock.calls.find(
      ([command, args]) =>
        command === 'db_write' &&
        (args as { op?: { op?: string } })?.op?.op === 'replace',
    );
    expect(write).toBeTruthy();
    const snapshot = (write?.[1] as { op: { snapshot: Record<string, unknown> } }).op.snapshot;
    expect(snapshot.servers).toHaveLength(1);
    expect((snapshot.servers as Array<Record<string, unknown>>)[0].id).toBe('srv_1');
    expect(window.localStorage.getItem(STORE_KEY)).toBeNull();
    expect(window.localStorage.getItem(BACKUP_KEY)).toBeNull();
    expect(store.state.servers[0].status).toBe('offline');
  });

  it('writes themes, settings and logs through granular operations', async () => {
    enableTauri();
    mockLoad({
      initialized: true,
      themes: { console: 'dark', desktop: 'pixel' },
      settings: { interval: 5 },
      servers: [persistedServer],
      logs: [],
      backup: null,
      render: null,
    });

    const { store } = await loadModules();
    await store.bootstrapStore();

    store.setTheme('console', 'nes');
    store.updateSettings({ interval: 9 });
    store.addLog({ serverId: 'srv_1', action: '测试写入', status: 'success' });

    await vi.waitFor(() => {
      const ops = invoke.mock.calls
        .filter(([command]) => command === 'db_write')
        .map(([, args]) => (args as { op: { op: string } }).op.op);
      expect(ops).toContain('themesSet');
      expect(ops).toContain('settingsSet');
      expect(ops).toContain('logAppend');
    });
  });

  it('migrates legacy render settings into an existing database', async () => {
    enableTauri();
    mockLoad({
      initialized: true,
      themes: { console: 'dark', desktop: 'pixel' },
      settings: null,
      servers: [],
      logs: [],
      backup: null,
      render: null,
    });
    window.localStorage.setItem('lg.render.v1', JSON.stringify({ global: 'canvas', tiers: {}, effects: { enabled: true, intensity: 'low', pauseWhenHidden: true }, a11yForceDom: false }));

    const { store } = await loadModules();
    await store.bootstrapStore();
    const { bootstrapRenderSettings } = await import('../render/settings');
    await bootstrapRenderSettings();

    const write = invoke.mock.calls.find(
      ([command, args]) =>
        command === 'db_write' &&
        (args as { op?: { op?: string } })?.op?.op === 'renderSet',
    );
    expect(write).toBeTruthy();
    expect(window.localStorage.getItem('lg.render.v1')).toBeNull();
  });
});
