import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FileEntry } from '../../ssh/backend';
import { TreeCache, treeChildren } from './treeCache';

function dir(name: string): FileEntry {
  return {
    name,
    path: `/root/${name}`,
    kind: 'dir',
    size: 0,
    mtime: null,
    mode: 0o755,
    modeText: 'drwxr-xr-x',
    uid: 0,
    gid: 0,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe('tree cache', () => {
  it('keeps only directories and symlinks', () => {
    const children = treeChildren([
      dir('logs'),
      { ...dir('link'), kind: 'symlink', targetKind: 'dir' },
      { ...dir('a.txt'), kind: 'file' },
    ]);
    expect(children.map((item) => item.name)).toEqual(['logs', 'link']);
  });

  it('dedupes concurrent loads for one path', async () => {
    let release: (entries: FileEntry[]) => void = () => {};
    const loader = vi.fn(() => new Promise<FileEntry[]>((resolve) => {
      release = resolve;
    }));
    const cache = new TreeCache(loader, 30_000, 3);
    const first = cache.load('/root');
    const second = cache.load('/root');
    await Promise.resolve();
    expect(loader).toHaveBeenCalledTimes(1);
    release([dir('logs')]);
    await expect(first).resolves.toHaveLength(1);
    await expect(second).resolves.toHaveLength(1);
  });

  it('caches for thirty seconds', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
    const loader = vi.fn(async () => [dir('logs')]);
    const cache = new TreeCache(loader, 30_000, 3);
    await cache.load('/root');
    await cache.load('/root');
    expect(loader).toHaveBeenCalledTimes(1);
    vi.setSystemTime(1_030_001);
    await cache.load('/root');
    expect(loader).toHaveBeenCalledTimes(2);
  });

  it('limits concurrent loads to three', async () => {
    const releases: Array<() => void> = [];
    const loader = vi.fn((path: string) => new Promise<FileEntry[]>((resolve) => {
      releases.push(() => resolve([dir(path)]));
    }));
    const cache = new TreeCache(loader, 30_000, 3);
    const pending = ['/a', '/b', '/c', '/d'].map((path) => cache.load(path));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(loader).toHaveBeenCalledTimes(3);
    releases.shift()?.();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(loader).toHaveBeenCalledTimes(4);
    while (releases.length > 0) releases.shift()?.();
    await Promise.all(pending);
  });
});
