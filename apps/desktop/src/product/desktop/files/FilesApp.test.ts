// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';
import FilesApp from './FilesApp.vue';
import type { Server } from '../../types';
import { applyPermit, guardState, resetGuard } from '../../guard/store';

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
      // 新建成功后的刷新要能看到新目录，才能验证「创建后选中它」。
      entry('reports', 'dir'),
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
    writePreview: vi.fn(),
    writeExecute: vi.fn(),
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
    name: 'web-01',
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
  const wrapper = mount(FilesApp, {
    props: { server: online() },
    attachTo: document.body,
    // LgModal 会 Teleport 到 body；测试里还原成内联渲染，才能从 wrapper 里查对话框。
    global: { stubs: { teleport: true } },
  });
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
    resetGuard();
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

  it('navigates from breadcrumb segments and switches to the path editor with Ctrl+L', async () => {
    const wrapper = await mountOnline();
    const input = wrapper.find('.lgfiles-address input');
    expect(input.isVisible()).toBe(false);
    expect(wrapper.findAll('.lgfiles-crumb').map((item) => item.text())).toEqual(['/', 'home', 'test']);

    await wrapper.find('[data-crumb-path="/home"]').trigger('click');
    await flushPromises();
    await flushPromises();
    expect(backend.sftpList).toHaveBeenCalledWith('srv_1', '/home');
    expect(wrapper.findAll('.lgfiles-crumb').map((item) => item.text())).toEqual(['/', 'home']);

    const viewport = wrapper.find('.lgfiles-list__viewport').element as HTMLElement;
    viewport.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', ctrlKey: true, bubbles: true }));
    await flushPromises();
    expect(input.isVisible()).toBe(true);
    expect((input.element as HTMLInputElement).value).toBe('/home');
    expect(document.activeElement).toBe(input.element);

    await input.setValue('/home/test/docs');
    await wrapper.find('.lgfiles-address').trigger('submit');
    await flushPromises();
    await flushPromises();
    expect(wrapper.findAll('.lgfiles-crumb').map((item) => item.text())).toEqual([
      '/',
      'home',
      'test',
      'docs',
    ]);
    expect(input.isVisible()).toBe(false);
    wrapper.unmount();
  });

  it('cancels the path editor with Escape', async () => {
    const wrapper = await mountOnline();
    const listsBefore = backend.sftpList.mock.calls.length;
    const viewport = wrapper.find('.lgfiles-list__viewport').element as HTMLElement;
    viewport.dispatchEvent(new KeyboardEvent('keydown', { key: 'l', ctrlKey: true, bubbles: true }));
    await flushPromises();

    const input = wrapper.find('.lgfiles-address input');
    await input.setValue('/changed');
    await input.trigger('keydown', { key: 'Escape' });
    await flushPromises();

    expect(input.isVisible()).toBe(false);
    expect((input.element as HTMLInputElement).value).toBe('/home/test');
    // 树展开也会列目录；这里只断言取消编辑没有触发新的导航。
    expect(backend.sftpList.mock.calls.length).toBe(listsBefore);
    wrapper.unmount();
  });

  it('collapses a long path and opens the editor from the ellipsis', async () => {
    const wrapper = await mountOnline();
    const input = wrapper.find('.lgfiles-address input');
    await input.setValue('/home/alice/projects/lightguard/src/app');
    await wrapper.find('.lgfiles-address').trigger('submit');
    await flushPromises();
    await flushPromises();

    const more = wrapper.find('.lgfiles-crumb__more');
    expect(more.exists()).toBe(true);
    expect(wrapper.findAll('.lgfiles-crumb').map((item) => item.text())).toEqual([
      '/',
      'lightguard',
      'src',
      'app',
    ]);

    await more.trigger('click');
    await flushPromises();
    expect(input.isVisible()).toBe(true);
    expect((input.element as HTMLInputElement).value).toBe('/home/alice/projects/lightguard/src/app');
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

  it('splits navigation and actions across two toolbar rows', async () => {
    const wrapper = await mountOnline();
    const rows = wrapper.findAll('.lgfiles-toolbar__row');
    expect(rows).toHaveLength(2);
    const [first, second] = rows;

    expect(first.find('.lgfiles-address').exists()).toBe(true);
    expect(first.find('.lgfiles-search').exists()).toBe(true);
    expect(first.find('button[aria-label="后退"]').exists()).toBe(true);
    expect(first.find('button[aria-label="刷新"]').exists()).toBe(true);

    expect(second.find('[data-action="new-folder"]').exists()).toBe(true);
    expect(second.find('button[aria-label="排序"]').exists()).toBe(true);
    expect(second.find('button[aria-label="视图"]').exists()).toBe(true);
    expect(second.find('button[aria-label="预览"]').exists()).toBe(true);
    expect(second.find('button[aria-label="更多"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it('sorts from the toolbar menu and toggles direction on the active key', async () => {
    const wrapper = await mountOnline();
    const openSort = async () => {
      await wrapper.find('button[aria-label="排序"]').trigger('click');
      await flushPromises();
    };

    await openSort();
    const items = () => wrapper.findAll('[aria-label="排序"] + .lgfiles-menu button');
    expect(items()).toHaveLength(6);
    expect(items()[0].attributes('aria-checked')).toBe('true');
    expect(items()[0].text()).toContain('↑');

    const size = items().find((item) => item.text().includes('大小'))!;
    await size.trigger('click');
    await flushPromises();
    expect(wrapper.find('.lgfiles-menu').exists()).toBe(false);

    await openSort();
    const active = items().find((item) => item.attributes('aria-checked') === 'true')!;
    expect(active.text()).toContain('大小');
    expect(active.text()).toContain('↑');

    await active.trigger('click');
    await flushPromises();
    await openSort();
    const flipped = items().find((item) => item.attributes('aria-checked') === 'true')!;
    expect(flipped.text()).toContain('大小');
    expect(flipped.text()).toContain('↓');
    wrapper.unmount();
  });

  it('routes the locked new-folder button to the permit panel', async () => {
    const wrapper = await mountOnline();
    const button = wrapper.get('[data-action="new-folder"]');
    // 锁定态只做 aria-disabled：按钮必须仍然可聚焦，用户才能发现原因。
    expect(button.attributes('aria-disabled')).toBe('true');
    expect(button.attributes('disabled')).toBeUndefined();

    await button.trigger('click');
    await flushPromises();
    expect(guardState.permitPanelOpen).toBe(true);
    expect(wrapper.find('.new-folder').exists()).toBe(false);
    wrapper.unmount();
  });

  it('creates a folder from the toolbar, refreshes the list and restores focus', async () => {
    applyPermit({
      serverId: 'srv_1',
      locked: false,
      minutes: 15,
      expiresAtMs: Date.now() + 15 * 60_000,
      writeActive: false,
    });
    backend.writePreview.mockResolvedValueOnce({
      previewId: 'p1',
      kind: 'mkdir',
      path: '/home/test/reports',
      operation: '新建文件夹',
      target: '/home/test/reports',
      impact: '将在 /home/test/reports 创建文件夹',
      rollback: '执行后可在审计中回退（仅删除空目录）',
      irreversible: false,
      expiresAtMs: Date.now() + 60_000,
    });
    backend.writeExecute.mockResolvedValueOnce({ auditId: 1, path: '/home/test/reports' });

    const wrapper = await mountOnline();
    const button = wrapper.get('[data-action="new-folder"]');
    expect(button.attributes('aria-disabled')).toBeUndefined();
    (button.element as HTMLButtonElement).focus();

    await button.trigger('click');
    await flushPromises();
    // 对话框自己带主机名：弹层浮在整个桌面上，确认页必须能定位到哪台服务器。
    expect(wrapper.find('.new-folder').text()).toContain('web-01');
    await wrapper.get('input[aria-label="文件夹名称"]').setValue('reports');
    await wrapper.get('[data-action="preview"]').trigger('click');
    await flushPromises();
    expect(backend.writePreview).toHaveBeenCalledWith({
      serverId: 'srv_1',
      dir: '/home/test',
      name: 'reports',
    });
    expect(wrapper.find('.new-folder').text()).toContain('web-01 · /home/test/reports');

    const listingsBefore = backend.sftpList.mock.calls.length;
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();
    await flushPromises();

    expect(backend.writeExecute).toHaveBeenCalledWith('p1');
    // 刷新复用的是文件应用既有的列目录路径。
    expect(backend.sftpList.mock.calls.length).toBeGreaterThan(listingsBefore);
    expect(wrapper.find('.new-folder').exists()).toBe(false);
    expect(wrapper.find('[data-path="/home/test/reports"]').classes()).toContain('is-selected');
    // 关闭后焦点回到触发按钮。
    expect(document.activeElement).toBe(button.element);
    wrapper.unmount();
  });
});
