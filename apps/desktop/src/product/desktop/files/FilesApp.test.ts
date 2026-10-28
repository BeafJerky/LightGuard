// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import FilesApp from './FilesApp.vue';
import type { Server } from '../../types';

const { backend } = vi.hoisted(() => {
  const entry = (name: string, kind: string) => ({
    name,
    path: `/home/test/${name}`,
    kind,
    size: 1,
    mtime: 0,
    mode: kind === 'dir' ? 0o755 : 0o644,
    modeText: kind === 'dir' ? 'drwxr-xr-x' : '-rw-r--r--',
    uid: 1000,
    gid: 1000,
    targetKind: kind,
  });
  const listings: Record<string, unknown[]> = {
    '/home/test': [
      entry('docs', 'dir'),
      entry('a.txt', 'file'),
      entry('logs', 'symlink'),
    ],
    '/home/test/docs': [entry('b.txt', 'file')],
  };
  const backend = {
    available: true,
    sftpHome: vi.fn(async () => '/home/test'),
    sftpList: vi.fn(async (_serverId: string, path: string) => ({
      path,
      canonicalPath: path,
      entries: listings[path] ?? [],
      truncated: false,
    })),
    sftpStat: vi.fn(async () => entry('docs', 'dir')),
    sftpPreviewText: vi.fn(async () => ({
      content: '',
      encoding: 'utf-8' as const,
      bytes: 0,
      truncated: false,
    })),
    sftpPreviewImage: vi.fn(async () => ({ mime: 'image/png', base64: '', bytes: 0 })),
    sftpDownload: vi.fn(async (_input: { path: string; kind: string }) => ({
      status: 'saved' as const,
      targetPath: 'C:/tmp/x',
      bytes: 1,
    })),
    sftpTransferCancel: vi.fn(async () => {}),
  };
  return { backend };
});

vi.mock('../../ssh', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../ssh')>();
  return { ...actual, ssh: backend };
});

function server(overrides: Partial<Server> = {}): Server {
  return {
    id: 'srv_1',
    sessionId: '',
    status: 'offline',
    disks: [],
    ...overrides,
  } as Server;
}

function online(): Server {
  return server({ sessionId: 's1', status: 'online' });
}

async function mountOnline() {
  const wrapper = mount(FilesApp, { props: { server: online() }, attachTo: document.body });
  await flushPromises();
  await flushPromises();
  return wrapper;
}

async function openContextDownload(wrapper: Awaited<ReturnType<typeof mountOnline>>, path: string) {
  await wrapper.find(`[data-path="${path}"]`).trigger('contextmenu', { clientX: 40, clientY: 40 });
  await flushPromises();
  const button = wrapper
    .findAll('.lgfiles-context button')
    .find((item) => item.text().includes('下载'))!;
  await button.trigger('click');
  await flushPromises();
}

describe('FilesApp', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.removeItem('lg.files.view');
  });

  it('asks to connect when there is no session', () => {
    const wrapper = mount(FilesApp, { props: { server: server() } });
    expect(wrapper.text()).toContain('请先连接服务器');
    expect(wrapper.find('.lgfiles-toolbar').exists()).toBe(false);
  });

  it('shows offline overlay when the session dropped', () => {
    const wrapper = mount(FilesApp, {
      props: { server: server({ sessionId: 's1', status: 'offline' }) },
    });
    expect(wrapper.text()).toContain('连接已断开');
  });

  it('removes its window listeners when unmounted', () => {
    const add = vi.spyOn(window, 'addEventListener');
    const remove = vi.spyOn(window, 'removeEventListener');
    const wrapper = mount(FilesApp, { props: { server: server() } });
    wrapper.unmount();

    const added = add.mock.calls.map(([type]) => type);
    const removed = remove.mock.calls.map(([type]) => type);
    expect(added).toEqual(expect.arrayContaining(['keydown', 'pointerdown']));
    expect(removed).toEqual(expect.arrayContaining(['keydown', 'pointerdown']));

    add.mockRestore();
    remove.mockRestore();
  });

  it('keeps keyboard navigation alive after entering a folder', async () => {
    const wrapper = await mountOnline();
    const address = wrapper.find('.lgfiles-address input').element as HTMLInputElement;
    expect(address.value).toBe('/home/test');

    await wrapper.find('[data-path="/home/test/docs"]').trigger('dblclick');
    await flushPromises();
    await flushPromises();
    expect(address.value).toBe('/home/test/docs');

    const viewport = wrapper.find('.lgfiles-list__viewport').element as HTMLElement;
    expect(document.activeElement).toBe(viewport);

    document.activeElement?.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true }),
    );
    await flushPromises();
    await flushPromises();
    expect(address.value).toBe('/home/test');

    wrapper.unmount();
  });

  it('confirms a directory download from the context menu', async () => {
    const wrapper = await mountOnline();
    await openContextDownload(wrapper, '/home/test/docs');

    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
    expect(wrapper.text()).toContain('符号链接按原样打包');
    expect(backend.sftpDownload).not.toHaveBeenCalled();

    const cancel = wrapper
      .findAll('.lgfiles-confirm__actions button')
      .find((item) => item.text() === '取消')!;
    await cancel.trigger('click');
    await flushPromises();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(backend.sftpDownload).not.toHaveBeenCalled();

    await openContextDownload(wrapper, '/home/test/docs');
    const confirm = wrapper
      .findAll('.lgfiles-confirm__actions button')
      .find((item) => item.text() === '继续下载')!;
    await confirm.trigger('click');
    await flushPromises();

    expect(backend.sftpDownload).toHaveBeenCalledTimes(1);
    expect(backend.sftpDownload.mock.calls[0][0]).toMatchObject({
      path: '/home/test/docs',
      kind: 'dir',
    });
    wrapper.unmount();
  });

  it('confirms a directory download from the preview button', async () => {
    const wrapper = await mountOnline();
    await wrapper.find('[data-path="/home/test/logs"]').trigger('contextmenu', { clientX: 40, clientY: 40 });
    await flushPromises();
    const preview = wrapper
      .findAll('.lgfiles-context button')
      .find((item) => item.text() === '预览')!;
    await preview.trigger('click');
    await flushPromises();

    const download = wrapper.find('.lgfiles-preview__actions button[aria-label="下载"]');
    expect(download.exists()).toBe(true);
    await download.trigger('click');
    await flushPromises();

    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
    expect(backend.sftpDownload).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('downloads a file without the directory confirmation', async () => {
    const wrapper = await mountOnline();
    await openContextDownload(wrapper, '/home/test/a.txt');

    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(backend.sftpDownload).toHaveBeenCalledTimes(1);
    expect(backend.sftpDownload.mock.calls[0][0]).toMatchObject({
      path: '/home/test/a.txt',
      kind: 'file',
    });
    wrapper.unmount();
  });

  it('switches between details, list and icon views from the toolbar', async () => {
    const wrapper = await mountOnline();
    expect(wrapper.find('.lgfiles-list__head').exists()).toBe(true);

    const openView = async () => {
      await wrapper.find('button[aria-label="视图"]').trigger('click');
      await flushPromises();
    };

    await openView();
    const list = wrapper.findAll('.lgfiles-menu button').find((item) => item.text().includes('列表'))!;
    expect(list.attributes('aria-checked')).toBe('false');
    await list.trigger('click');
    await flushPromises();
    expect(wrapper.find('.lgfiles-list__head').exists()).toBe(false);
    expect(wrapper.findAll('.lgfiles-list__row.is-compact').length).toBeGreaterThan(0);
    expect(window.localStorage.getItem('lg.files.view')).toBe('list');

    await openView();
    const grid = wrapper.findAll('.lgfiles-menu button').find((item) => item.text().includes('小图标'))!;
    await grid.trigger('click');
    await flushPromises();
    expect(wrapper.findAll('.lgfiles-grid__cell').length).toBeGreaterThan(0);
    expect(window.localStorage.getItem('lg.files.view')).toBe('grid');

    await openView();
    const details = wrapper
      .findAll('.lgfiles-menu button')
      .find((item) => item.text().includes('详细信息'))!;
    expect(details.attributes('aria-checked')).toBe('false');
    await details.trigger('click');
    await flushPromises();
    expect(wrapper.find('.lgfiles-list__head').exists()).toBe(true);
    expect(window.localStorage.getItem('lg.files.view')).toBe('details');

    wrapper.unmount();
  });
});
