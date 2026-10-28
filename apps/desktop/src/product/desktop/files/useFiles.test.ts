import { describe, expect, it, vi } from 'vitest';
import type { Server } from '../../types';
import type {
  DownloadInput,
  DownloadResult,
  FileEntry,
  FileListing,
  SshBackend,
  TextPreview,
  TransferProgress,
} from '../../ssh/backend';
import { useFiles } from './useFiles';

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((settle, fail) => {
    resolve = settle;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function textPreview(content: string): TextPreview {
  return { content, encoding: 'utf-8', bytes: content.length, truncated: false };
}

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

  it('previews svg through the text endpoint', async () => {
    const sftpPreviewText = vi.fn(async () => textPreview('<svg/>'));
    const sftpPreviewImage = vi.fn(async () => ({ mime: 'image/svg+xml', base64: '', bytes: 0 }));
    const files = useFiles(server(), backend({ sftpPreviewText, sftpPreviewImage }));
    await files.boot();

    await files.loadPreview(entry('icon.svg'));

    expect(sftpPreviewText).toHaveBeenCalledWith('srv_1', '/home/test/icon.svg');
    expect(sftpPreviewImage).not.toHaveBeenCalled();
    expect(files.preview.mode).toBe('text');
    expect(files.preview.text?.content).toBe('<svg/>');
  });

  it('keeps a download waiting until the first progress event', async () => {
    const pending = deferred<DownloadResult>();
    let emit: ((event: TransferProgress) => void) | null = null;
    const sftpDownload = vi.fn((input: DownloadInput) => {
      emit = input.onEvent;
      return pending.promise;
    });
    const files = useFiles(server(), backend({ sftpDownload }));
    await files.boot();

    await files.download(entry('a.txt'));
    const task = files.downloads.value[0];
    expect(task.status).toBe('waiting');
    expect(task.message).toBe('等待中');
    expect(emit).not.toBeNull();

    emit!({ type: 'progress', transferred: 4, total: 10 });
    expect(task.status).toBe('running');
    expect(task.message).toBe('传输中');
    expect(task.transferred).toBe(4);

    pending.resolve({ status: 'saved', targetPath: 'C:/tmp/a.txt', bytes: 10 });
    await vi.waitFor(() => expect(task.status).toBe('saved'));
  });

  it('marks offline without clearing the list', async () => {
    const files = useFiles({ ...server(), status: 'offline' }, backend());
    await files.boot();
    expect(files.currentPath.value).toBe('/home/test');
    expect(files.offline.value).toBe(true);
    await files.refresh();
    expect(files.currentPath.value).toBe('/home/test');
  });

  it('keeps the forward entry after going back', async () => {
    const files = useFiles(server(), backend());
    await files.boot();
    await files.open(files.visibleRows.value[0]);
    expect(files.currentPath.value).toBe('/home/test/docs');

    await files.go(-1);
    expect(files.currentPath.value).toBe('/home/test');
    expect(files.canForward.value).toBe(true);

    await files.go(1);
    expect(files.currentPath.value).toBe('/home/test/docs');
    expect(files.canForward.value).toBe(false);
  });

  it('drops stale navigation responses', async () => {
    const pending = new Map<string, ReturnType<typeof deferred<FileListing>>>();
    const sftpList = vi.fn(
      (_serverId: string, path: string): Promise<FileListing> => {
        const response = deferred<FileListing>();
        pending.set(path, response);
        return response.promise;
      },
    );
    const files = useFiles(server(), backend({ sftpList }));

    const first = files.navigate('/a');
    const second = files.navigate('/b');
    pending.get('/b')?.resolve(listing('/b', [entry('b.txt')]));
    await second;
    pending.get('/a')?.resolve(listing('/a', [entry('a-only.txt')]));
    await first;

    expect(files.currentPath.value).toBe('/b');
    expect(files.visibleRows.value.map((item) => item.name)).toEqual(['b.txt']);
    expect(files.loading.value).toBe(false);
    expect(files.historyIndex.value).toBe(0);
  });

  it('drops stale preview responses', async () => {
    const slow = deferred<TextPreview>();
    const sftpPreviewText = vi.fn(
      (_serverId: string, path: string): Promise<TextPreview> =>
        path.endsWith('a.txt') ? slow.promise : Promise.resolve(textPreview('B content')),
    );
    const files = useFiles(server(), backend({ sftpPreviewText }));

    const first = files.loadPreview(entry('a.txt'));
    const second = files.loadPreview(entry('b.txt'));
    await second;
    slow.resolve(textPreview('A content'));
    await first;

    expect(files.preview.entry?.name).toBe('b.txt');
    expect(files.preview.text?.content).toBe('B content');
    expect(files.preview.loading).toBe(false);
  });

  it('drops an in-flight preview when another entry is selected', async () => {
    const slow = deferred<TextPreview>();
    const sftpPreviewText = vi.fn(() => slow.promise);
    const files = useFiles(server(), backend({ sftpPreviewText }));

    const opening = files.loadPreview(entry('a.txt'));
    files.select(entry('b.txt'));
    slow.resolve(textPreview('A content'));
    await opening;

    expect(files.preview.entry?.name).toBe('b.txt');
    expect(files.preview.text).toBeNull();
    expect(files.preview.loading).toBe(false);
  });

  it('drops an open whose symlink probe finishes after another selection', async () => {
    const pending = deferred<FileEntry>();
    const sftpStat = vi.fn(() => pending.promise);
    const files = useFiles(server(), backend({ sftpStat }));
    const link: FileEntry = { ...entry('link.txt'), kind: 'symlink', targetKind: 'file' };

    const opening = files.open(link);
    files.select(entry('b.txt'));
    pending.resolve({ ...entry('link.txt'), kind: 'file', targetKind: 'file' });
    await opening;

    expect(files.preview.entry?.name).toBe('b.txt');
    expect(files.preview.text).toBeNull();
    expect(files.previewOpen.value).toBe(false);
  });

  it('disables up at the filesystem root', async () => {
    const sftpList = vi.fn(
      async (_serverId: string, path: string): Promise<FileListing> =>
        listing(path, path === '/' ? [entry('etc', 'dir')] : [entry('docs', 'dir')]),
    );
    const files = useFiles(server(), backend({ sftpList }));
    await files.boot();
    expect(files.canUp.value).toBe(true);

    expect(await files.navigate('/')).toBe(true);
    expect(files.currentPath.value).toBe('/');
    expect(files.canUp.value).toBe(false);
    await expect(files.goUp()).resolves.toBe(false);
  });
});
