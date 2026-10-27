import { describe, expect, it } from 'vitest';
import { HISTORY_POINTS } from '../data';
import { applySnapshot, formatUptime, newServer } from './adapter';
import type { ConnectResult, ServerSnapshot } from './backend';

function snapshot(overrides: Partial<ServerSnapshot> = {}): ServerSnapshot {
  return {
    hostname: 'web-prod-01',
    os: 'Debian GNU/Linux 12',
    kernel: '6.1.0-18-amd64',
    arch: 'x86_64',
    timezone: 'CST',
    uptimeSeconds: 90061,
    bootTime: '2026-10-24 09:00',
    cpuModel: 'Intel Xeon E5-2680 v4',
    cpuCores: 4,
    cpuFreq: '2400 MHz',
    cpuPercent: 37.4,
    cpuCoresLoad: [31.2, 42.7, 22.1, 53.5],
    load: [0.42, 0.5, 0.61],
    tempC: 52.6,
    memTotalGb: 15.6,
    memAvailableGb: 6.2,
    memPercent: 60.3,
    swapTotalGb: 2,
    swapUsedGb: 0.12,
    diskTotalGb: 240,
    diskPercent: 71.2,
    disks: [
      { mount: '/', fs: '/dev/sda1', kind: 'ext4', totalGb: 240, usedGb: 170.9, inodePercent: 8 },
    ],
    downMbps: 1.234,
    upMbps: 0.456,
    interfaces: [
      {
        name: 'eth0',
        ip: '192.168.5.4',
        mac: 'aa:bb:cc:dd:ee:ff',
        speed: '1 Gbps',
        rxTotalGb: 24.5,
        txTotalGb: 8.25,
        downMbps: 1.2,
        upMbps: 0.4,
      },
    ],
    gateway: '192.168.5.1',
    dns: '223.5.5.5',
    publicIp: '—',
    mtu: '1500',
    processes: [
      { pid: 742, name: 'sshd', user: 'root', cpu: 0.4, mem: 0.9, state: 'S' },
      { pid: 901, name: 'nginx', user: 'www', cpu: 2.1, mem: 1.7, state: 'S' },
    ],
    processCount: 128,
    sampledAt: '2026-10-27T15:30:00+08:00',
    ...overrides,
  };
}

function result(overrides: Partial<ConnectResult> = {}): ConnectResult {
  return {
    sessionId: 'sess_1',
    startedAt: '2026-10-27T15:00:00+08:00',
    fingerprint: 'SHA256:abc',
    hostKeyState: 'new',
    authLabel: '密码',
    latencyMs: 12,
    snapshot: snapshot(),
    encryption: { cipher: 'chacha20-poly1305', kex: 'curve25519-sha256', mac: 'hmac-sha2-256' },
    ...overrides,
  };
}

const meta = {
  serverId: 'srv_1',
  auth: 'password' as const,
  draft: {
    name: '生产 Web',
    ip: '192.168.5.4',
    port: 22,
    user: 'admin',
    auth: 'password' as const,
    group: '生产',
    tags: ['web'],
    role: 'Web',
    note: '',
  },
};

describe('formatUptime', () => {
  it('formats days, hours and minutes', () => {
    expect(formatUptime(90061)).toBe('1 天 1 小时');
    expect(formatUptime(3660)).toBe('1 小时 1 分钟');
    expect(formatUptime(120)).toBe('2 分钟');
    expect(formatUptime(0)).toBe('—');
  });
});

describe('newServer', () => {
  it('maps a real snapshot into the view model', () => {
    const server = newServer(meta, result());

    expect(server.name).toBe('生产 Web');
    expect(server.hostname).toBe('web-prod-01');
    expect(server.os).toContain('Debian');
    expect(server.status).toBe('online');
    expect(server.metrics).toEqual({ cpu: 37, mem: 60, disk: 71, down: 1.2, up: 0.5 });
    expect(server.disks[0]).toMatchObject({ mount: '/', fs: '/dev/sda1', type: 'ext4', inodePercent: 8 });
    expect(server.interfaces[0].rxTotal).toBe(24.5);
    expect(server.processCount).toBe(128);
    expect(server.sessionId).toBe('sess_1');
    expect(server.hostKeyState).toBe('new');
    expect(server.encryption?.cipher).toBe('chacha20-poly1305');
    expect(server.samples).toHaveLength(1);
    expect(server.history.cpu).toEqual([37]);
  });

  it('flags high resource usage as warning', () => {
    const server = newServer(meta, result({ snapshot: snapshot({ diskPercent: 94 }) }));
    expect(server.status).toBe('warning');
    expect(server.warning).toContain('磁盘');
  });

  it('keeps empty collections when the collector returns nothing', () => {
    const server = newServer(
      { ...meta, draft: { ...meta.draft, name: '' } },
      result({
        snapshot: snapshot({
          hostname: '',
          disks: [],
          interfaces: [],
          processes: [],
          processCount: 0,
        } as Partial<ServerSnapshot>),
      }),
    );

    expect(server.name).toBe('192.168.5.4');
    expect(server.hostname).toBe('');
    expect(server.disks).toEqual([]);
    expect(server.interfaces).toEqual([]);
    expect(server.processes).toEqual([]);
    expect(server.processCount).toBe(0);
  });
});

describe('applySnapshot', () => {
  it('appends history and caps at the retention limit', () => {
    let server = newServer(meta, result());
    for (let index = 0; index < HISTORY_POINTS + 10; index += 1) {
      server = applySnapshot(
        server,
        snapshot({ cpuPercent: index % 100, downMbps: index * 0.1, sampledAt: '' }),
      );
    }

    expect(server.history.cpu).toHaveLength(HISTORY_POINTS);
    expect(server.history.net).toHaveLength(HISTORY_POINTS);
    expect(server.samples).toHaveLength(HISTORY_POINTS);
    expect(server.error).toBeNull();
    expect(server.needsAuth).toBe(false);
  });

  it('recomputes warning state and clears stale errors', () => {
    const server = newServer(meta, result());
    server.error = { code: 'TIMEOUT', message: '超时', hint: '' };
    server.status = 'error';

    applySnapshot(server, snapshot({ memPercent: 91, diskPercent: 40 }));
    expect(server.status).toBe('warning');
    expect(server.warning).toContain('内存');
    expect(server.error).toBeNull();
  });
});
