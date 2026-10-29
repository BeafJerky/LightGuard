import { describe, expect, it, vi } from 'vitest';
import type { DockerContainer, DockerStatsSample } from './api';
import {
  filterContainers,
  mergeStats,
  normalizeLayout,
  readLayout,
  sortContainers,
  writeLayout,
} from './containers';

function row(patch: Partial<DockerContainer> = {}): DockerContainer {
  return {
    id: 'a'.repeat(64),
    shortId: 'aaaaaaaaaaaa',
    name: 'shop-web-1',
    image: 'nginx:1.27',
    command: 'nginx -g daemon off;',
    state: 'running',
    status: 'Up 3 hours',
    createdAt: '2026-10-29 09:00:00 +0800 CST',
    project: 'shop',
    service: 'web',
    ports: '0.0.0.0:8080->80/tcp',
    cpuPercent: null,
    memUsage: '',
    memLimit: '',
    memPercent: null,
    netRx: '',
    netTx: '',
    ...patch,
  };
}

function sample(patch: Partial<DockerStatsSample> = {}): DockerStatsSample {
  return {
    id: 'a'.repeat(64),
    cpuPercent: 3.2,
    memUsage: '52.1MiB',
    memLimit: '1GiB',
    memPercent: 5.1,
    netRx: '1.2MB',
    netTx: '0.4MB',
    ...patch,
  };
}

describe('docker containers', () => {
  it('keeps only known layouts', () => {
    expect(normalizeLayout('table')).toBe('table');
    expect(normalizeLayout('card')).toBe('card');
    expect(normalizeLayout('')).toBe('card');
    expect(normalizeLayout(null)).toBe('card');
  });

  it('merges stats by full id', () => {
    const other = row({ id: 'b'.repeat(64), name: 'shop-db-1' });
    const merged = mergeStats([row(), other], [sample()]);
    expect(merged[0].cpuPercent).toBe(3.2);
    expect(merged[0].memUsage).toBe('52.1MiB');
    expect(merged[1].cpuPercent).toBeNull();
  });

  it('filters by name, image and project', () => {
    const rows = [row(), row({ id: 'b'.repeat(64), name: 'shop-db-1', image: 'postgres:16', project: 'shop' })];
    expect(filterContainers(rows, 'web')).toHaveLength(1);
    expect(filterContainers(rows, 'POSTGRES')).toHaveLength(1);
    expect(filterContainers(rows, 'shop')).toHaveLength(2);
    expect(filterContainers(rows, '  ')).toHaveLength(2);
  });

  it('sorts running containers first and then by name', () => {
    const rows = [
      row({ id: 'c'.repeat(64), name: 'z-exited', state: 'exited' }),
      row({ id: 'b'.repeat(64), name: 'b-running' }),
      row({ id: 'd'.repeat(64), name: 'a-running' }),
    ];
    expect(sortContainers(rows).map((entry) => entry.name)).toEqual([
      'a-running',
      'b-running',
      'z-exited',
    ]);
  });

  it('persists the layout and survives storage failures', () => {
    writeLayout('table');
    expect(readLayout()).toBe('table');
    writeLayout('card');
    expect(readLayout()).toBe('card');
    const getter = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    expect(readLayout()).toBe('card');
    getter.mockRestore();
  });
});
