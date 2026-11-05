import { describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';
import type { Server } from '../../types';
import type { SshBackend } from '../../ssh/backend';
import { useFileSession } from './useFileSession';

function entry(name: string, kind: 'file' | 'dir' = 'file') {
  return {
    name,
    path: `/home/test/${name}`,
    kind,
    size: 10,
    mtime: 0,
    mode: kind === 'dir' ? 0o755 : 0o644,
    modeText: kind === 'dir' ? 'drwxr-xr-x' : '-rw-r--r--',
    uid: 1000,
    gid: 1000,
    targetKind: kind,
  };
}

function server(overrides: Partial<Server> = {}): Server {
  return {
    id: 'srv_1',
    name: 'web-01',
    sessionId: 's1',
    status: 'online',
    disks: [{ mount: '/', fs: 'ext4', type: 'ext4', total: 1, used: 1, inodePercent: 1 }],
    ...overrides,
  } as Server;
}

function backend(): SshBackend {
  return {
    available: true,
    sftpHome: vi.fn(async () => '/home/test'),
    sftpList: vi.fn(async (_serverId, path) => ({
      path,
      canonicalPath: path,
      entries:
        path === '/home/test'
          ? [entry('docs', 'dir'), entry('a.txt')]
          : path === '/home/test/docs'
            ? [entry('inner', 'dir')]
            : [],
      truncated: false,
    })),
    sftpStat: vi.fn(async (_serverId, path) => entry(path.split('/').pop() ?? 'x')),
    sftpDownload: vi.fn(async () => ({ status: 'saved' as const, targetPath: 'C:/tmp/a.txt', bytes: 2 })),
    sftpTransferCancel: vi.fn(async () => {}),
  } as unknown as SshBackend;
}

describe('useFileSession', () => {
  it('boots home and roots once for concurrent callers', async () => {
    const client = backend();
    const session = useFileSession(server(), client);
    await Promise.all([session.ensureBoot(), session.ensureBoot()]);

    expect(client.sftpHome).toHaveBeenCalledTimes(1);
    expect(session.homePath.value).toBe('/home/test');
    expect(session.roots.value.map((node) => node.path)).toEqual(['/home/test', '/']);
    expect(session.roots.value[0].expanded).toBe(true);
  });

  it('shares tree expansion and downloads', async () => {
    const session = useFileSession(server(), backend());
    await session.ensureBoot();
    await session.expandNode('/home/test/docs');

    const docs = session.roots.value[0].children?.find((node) => node.path === '/home/test/docs');
    expect(docs?.children?.map((node) => node.name)).toEqual(['inner']);

    session.enqueueDownload(entry('a.txt'), 'file');
    await Promise.resolve();
    await Promise.resolve();
    expect(session.downloads.value[0].status).toBe('saved');
  });

  it('persists view mode and icon scheme', () => {
    window.localStorage.removeItem('lg.files.view');
    window.localStorage.removeItem('lg.files.icons');
    const session = useFileSession(server(), backend());

    session.setMode('grid');
    session.setIconScheme('line');
    expect(window.localStorage.getItem('lg.files.view')).toBe('grid');
    expect(window.localStorage.getItem('lg.files.icons')).toBe('line');
    expect(session.mode.value).toBe('grid');
    expect(session.showHidden.value).toBe(false);
  });

  it('bumps generation when the session changes', async () => {
    const state = reactive(server());
    const session = useFileSession(state, backend());
    await session.ensureBoot();
    const before = session.generation.value;

    state.sessionId = 's2';
    await Promise.resolve();
    await Promise.resolve();

    expect(session.generation.value).toBe(before + 1);
    expect(session.homePath.value).toBe('/home/test');
  });
});
