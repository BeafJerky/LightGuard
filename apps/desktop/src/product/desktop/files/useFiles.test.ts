import { describe, expect, it, vi } from 'vitest';
import type { Server } from '../../types';
import type { FileEntry, FileListing, SshBackend } from '../../ssh/backend';
import { useFiles } from './useFiles';

function entry(name: string, kind: FileEntry['kind'] = 'file'): FileEntry {
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

function listing(path: string, entries: FileEntry[]): FileListing {
  return { path, canonicalPath: path, entries, truncated: false };
}

function server(): Server {
  return { id: 'srv_1', sessionId: 's1', status: 'online', disks: [{ mount: '/', fs: 'ext4', type: 'ext4', total: 1, used: 1, inodePercent: 1 }] } as Server;
}

function backend(overrides: Partial<SshBackend> = {}): SshBackend {
  return {
    available: true,
    sftpHome: vi.fn(async () => '/home/test'),
    sftpList: vi.fn(async (_, path) => listing(path, [entry('docs', 'dir'), entry('a.txt')])),
    sftpStat: vi.fn(async (_, path) => entry('a.txt')),
    sftpPreviewText: vi.fn(async () => ({ content: 'hi', encoding: 'utf-8' as const, bytes: 2, truncated: false })),
    sftpPreviewImage: vi.fn(async () => ({ mime: 'image/png', base64: '', bytes: 0 })),
    sftpDownload: vi.fn(async () => ({ status: 'saved' as const, targetPath: 'C:/tmp/a.txt', bytes: 2 })),
    sftpTransferCancel: vi.fn(async () => {}),
    ...overrides,
  } as SshBackend;
}

describe('useFiles', () => {
  it('boots into the home directory and expands the tree', async () => {
    const files = useFiles(server(), backend());
    await files.boot();
    expect(files.currentPath.value).toBe('/home/test');
    expect(files.visibleRows.value.map((item) => item.name)).toEqual(['docs', 'a.txt']);
    expect(files.roots.value[0].children?.map((item) => item.name)).toEqual(['docs']);
  });

  it('navigates, goes back and restores selection when possible', async () => {
    const files = useFiles(server(), backend());
    await files.boot();
    files.select(files.visibleRows.value[0]);
    await files.open(files.visibleRows.value[0]);
    expect(files.currentPath.value).toBe('/home/test/docs');
    expect(files.canBack.value).toBe(true);
    await files.go(-1);
    expect(files.currentPath.value).toBe('/home/test');
  });

  it('opens text preview for a text file', async () => {
    const files = useFiles(server(), backend());
    await files.boot();
    await files.open(entry('a.txt'));
    expect(files.previewOpen.value).toBe(true);
    expect(files.preview.text?.content).toBe('hi');
  });

  it('marks offline without clearing the list', async () => {
    const files = useFiles({ ...server(), status: 'offline' }, backend());
    await files.boot();
    expect(files.currentPath.value).toBe('/home/test');
    expect(files.offline.value).toBe(true);
    await files.refresh();
    expect(files.currentPath.value).toBe('/home/test');
  });
});
