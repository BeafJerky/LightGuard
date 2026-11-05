import { describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';
import { flushPromises } from '@vue/test-utils';
import type { Server } from '../../types';
import type { SshBackend } from '../../ssh/backend';
import { MAX_TABS, useFileTabs } from './useFileTabs';

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
            ? [entry('b.txt')]
            : [],
      truncated: false,
    })),
    sftpStat: vi.fn(async (_serverId, path) => entry(path.split('/').pop() ?? 'x')),
    sftpDownload: vi.fn(async () => ({ status: 'saved' as const, targetPath: 'C:/tmp/a.txt', bytes: 2 })),
    sftpTransferCancel: vi.fn(async () => {}),
  } as unknown as SshBackend;
}

async function settle() {
  await flushPromises();
  await flushPromises();
  await flushPromises();
}

describe('useFileTabs', () => {
  it('creates one tab at the home directory', async () => {
    const tabs = useFileTabs(server(), backend());
    await settle();

    expect(tabs.tabs.value).toHaveLength(1);
    expect(tabs.active.value?.key).toBe(tabs.tabs.value[0].key);
    expect(tabs.active.value?.files.currentPath.value).toBe('/home/test');
    expect(tabs.active.value?.files.roots.value[0].expanded).toBe(true);
  });

  it('keeps navigation, history and selection per tab', async () => {
    const tabs = useFileTabs(server(), backend());
    await settle();
    const first = tabs.tabs.value[0];
    await first.files.open(first.files.entries.value[0]);
    await settle();
    first.files.select(first.files.entries.value[0]);
    expect(first.files.currentPath.value).toBe('/home/test/docs');

    const second = tabs.createTab('/home/test');
    await settle();
    second.files.select(second.files.entries.value[1]);
    expect(second.files.currentPath.value).toBe('/home/test');

    tabs.activate(first.key);
    expect(tabs.active.value?.key).toBe(first.key);
    expect(first.files.currentPath.value).toBe('/home/test/docs');
    expect(first.files.selectedPath.value).toBe('/home/test/b.txt');
    expect(first.files.historyIndex.value).toBe(1);

    tabs.activate(second.key);
    expect(second.files.currentPath.value).toBe('/home/test');
    expect(second.files.selectedPath.value).toBe('/home/test/a.txt');
    expect(first.files.selectedPath.value).toBe('/home/test/b.txt');
  });

  it('shares the tree and downloads across tabs', async () => {
    const tabs = useFileTabs(server(), backend());
    await settle();
    const first = tabs.tabs.value[0];
    const second = tabs.createTab('/home/test');
    await settle();

    await first.files.toggleNode('/home/test/docs');
    const docs = second.files.roots.value[0].children?.find((node) => node.path === '/home/test/docs');
    expect(docs?.expanded).toBe(true);

    await second.files.download(entry('a.txt'));
    await settle();
    expect(first.files.downloads.value).toHaveLength(1);
    expect(second.files.downloads.value[0].status).toBe('saved');
  });

  it('closes to the left neighbor and reports the last tab', async () => {
    const tabs = useFileTabs(server(), backend());
    await settle();
    const first = tabs.tabs.value[0];
    const second = tabs.createTab();
    await settle();
    const third = tabs.createTab();
    await settle();

    tabs.closeTab(third.key);
    expect(tabs.active.value?.key).toBe(second.key);
    tabs.closeTab(second.key);
    expect(tabs.active.value?.key).toBe(first.key);
    tabs.closeTab(first.key);
    expect(tabs.tabs.value).toHaveLength(0);
    expect(tabs.closed.value).toBe(true);
  });

  it('caps at eight tabs', async () => {
    const tabs = useFileTabs(server(), backend());
    await settle();
    for (let index = 0; index < MAX_TABS + 3; index += 1) tabs.createTab();
    await settle();
    expect(tabs.tabs.value).toHaveLength(MAX_TABS);
  });

  it('shares view preferences but keeps sort and filter per tab', async () => {
    window.localStorage.removeItem('lg.files.icons');
    const tabs = useFileTabs(server(), backend());
    await settle();
    const first = tabs.tabs.value[0];
    const second = tabs.createTab();
    await settle();

    first.files.setMode('grid');
    first.files.setIconScheme('line');
    second.files.toggleSort('size');
    second.files.view.query = 'a';

    expect(second.files.view.mode).toBe('grid');
    expect(second.files.iconScheme.value).toBe('line');
    expect(first.files.view.sortKey).toBe('name');
    expect(first.files.view.query).toBe('');
  });

  it('resets every tab when the session changes', async () => {
    const state = reactive(server());
    const tabs = useFileTabs(state, backend());
    await settle();
    const tab = tabs.tabs.value[0];
    await tab.files.navigate('/home/test/docs');
    await settle();

    state.sessionId = 's2';
    await settle();

    expect(tab.files.currentPath.value).toBe('/home/test');
  });
});
