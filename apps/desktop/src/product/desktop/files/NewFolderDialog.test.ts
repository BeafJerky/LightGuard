// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushPromises, mount } from '@vue/test-utils';

const mocks = vi.hoisted(() => ({
  writePreview: vi.fn(),
  writeExecute: vi.fn(),
  permitStatus: vi.fn(),
  onPermit: vi.fn(() => () => {}),
  onWrite: vi.fn(() => () => {}),
}));

vi.mock('../../ssh', () => ({
  ssh: mocks,
  toSshError: (error: unknown) => error,
}));

import { guardState, resetGuard } from '../../guard/store';
import NewFolderDialog from './NewFolderDialog.vue';

// 预览有效期按 60 秒算，所以夹具必须给出未来的 `expiresAtMs`：
// 过期预览会（按设计要求）把「确认创建」换成「重新预览」。
function preview(overrides: Record<string, unknown> = {}) {
  return {
    previewId: 'p1',
    kind: 'mkdir' as const,
    path: '/data/reports',
    operation: '新建文件夹',
    target: '/data/reports',
    impact: '将在 /data/reports 创建文件夹',
    rollback: '执行后可在审计中回退（仅删除空目录）',
    irreversible: false,
    expiresAtMs: Date.now() + 60_000,
    ...overrides,
  };
}

function permission(): Record<string, unknown> {
  return {
    serverId: 'srv_1',
    locked: false,
    minutes: 15,
    expiresAtMs: Date.now() + 15 * 60_000,
    writeActive: false,
  };
}

function mountDialog(serverId = 'srv_1', serverName = 'web-01') {
  return mount(NewFolderDialog, {
    props: { open: true, serverId, serverName, dir: '/data' },
    attachTo: document.body,
    // LgModal 会 Teleport 到 body：测试里还原成内联渲染，才能用 wrapper 查询内容。
    global: { stubs: { teleport: true } },
  });
}

async function previewName(wrapper: Awaited<ReturnType<typeof mountDialog>>, value: string) {
  await wrapper.get('input[aria-label="文件夹名称"]').setValue(value);
  await wrapper.get('[data-action="preview"]').trigger('click');
  await flushPromises();
}

beforeEach(() => {
  resetGuard();
  vi.clearAllMocks();
  mocks.permitStatus.mockResolvedValue(permission());
});

afterEach(() => {
  document.body.innerHTML = '';
  vi.useRealTimers();
});

describe('new folder dialog', () => {
  it('validates the name before calling the backend', async () => {
    const wrapper = mountDialog();
    await wrapper.get('input[aria-label="文件夹名称"]').setValue('a/b');
    await wrapper.get('[data-action="preview"]').trigger('click');
    expect(wrapper.text()).toContain('不能包含');
    expect(mocks.writePreview).not.toHaveBeenCalled();
    wrapper.unmount();
  });

  it('previews four sections then executes', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockResolvedValueOnce({ auditId: 9, path: '/data/reports' });
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    expect(mocks.writePreview).toHaveBeenCalledWith({
      serverId: 'srv_1',
      dir: '/data',
      name: 'reports',
    });
    expect(wrapper.text()).toContain('/data/reports');
    expect(wrapper.text()).toContain('回退');
    // 影响的四个字段：操作、目标、影响、回退。
    expect(wrapper.findAll('.new-folder__section')).toHaveLength(4);

    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();
    expect(mocks.writeExecute).toHaveBeenCalledWith('p1');
    expect(wrapper.emitted('created')?.[0]).toEqual(['reports']);
    wrapper.unmount();
  });

  it('shows message and hint on failure and links to the audit', async () => {
    mocks.writePreview.mockRejectedValueOnce(
      Object.assign(new Error('目标已存在'), { code: 'PREVIEW_FAILED', hint: '换一个名称后重试' }),
    );
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    expect(wrapper.text()).toContain('目标已存在');
    expect(wrapper.text()).toContain('换一个名称后重试');

    await wrapper.get('[data-action="view-audit"]').trigger('click');
    expect(wrapper.emitted('viewAudit')).toHaveLength(1);
    wrapper.unmount();
  });

  it('re-previews instead of reusing the preview a failed execute burned', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview()).mockResolvedValueOnce(preview({ previewId: 'p2' }));
    mocks.writeExecute.mockRejectedValueOnce(
      Object.assign(new Error('创建文件夹失败：权限不足'), {
        code: 'WRITE_FAILED',
        hint: '请检查目标目录权限或磁盘状态后重试。',
      }),
    );
    mocks.writeExecute.mockResolvedValueOnce({ auditId: 10, path: '/data/reports' });
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    // 后端已消费掉失败的预览：不能再提交同一个 id。
    expect(mocks.writeExecute).toHaveBeenCalledTimes(1);
    expect(mocks.writeExecute).toHaveBeenCalledWith('p1');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    expect(wrapper.text()).toContain('创建文件夹失败：权限不足');
    expect(wrapper.find('[data-action="preview"]').text()).toContain('重新预览');

    await wrapper.get('[data-action="preview"]').trigger('click');
    await flushPromises();
    expect(mocks.writePreview).toHaveBeenCalledTimes(2);
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();
    expect(mocks.writeExecute).toHaveBeenLastCalledWith('p2');
    expect(wrapper.emitted('created')?.[0]).toEqual(['reports']);
    wrapper.unmount();
  });

  it('flags a write that may have completed without an audit id', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockRejectedValueOnce(
      Object.assign(
        new Error(
          '创建文件夹，但审计记录写入失败：磁盘已满；本次操作可能已完成，且没有审计编号，无法在审计中回退。',
        ),
        {
          code: 'WRITE_FAILED',
          hint: '操作可能已完成，但审计记录写入失败，没有审计编号；请在文件列表中刷新核对，避免重复执行。',
        },
      ),
    );
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('可能已完成，无法回退');
    expect(wrapper.text()).toContain('没有审计编号');
    // 没有审计编号就没有回退入口，也不再提供「确认创建」。
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    expect(wrapper.get('[data-action="view-audit"]').text()).toContain('查看审计');
    wrapper.unmount();
  });

  it('refreshes the permit when it expired between preview and execute', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockRejectedValueOnce(
      Object.assign(new Error('当前服务器未解锁。'), {
        code: 'PERMIT_REQUIRED',
        hint: '先解锁当前服务器再执行写操作。',
      }),
    );
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    expect(mocks.permitStatus).toHaveBeenCalledWith('srv_1');
    expect(wrapper.text()).toContain('先解锁当前服务器再执行写操作。');
    await wrapper.get('[data-action="need-permit"]').trigger('click');
    expect(wrapper.emitted('needPermit')).toHaveLength(1);
    wrapper.unmount();
  });

  it('refreshes the permit and explains a busy server', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockRejectedValueOnce(
      Object.assign(new Error('写操作正在进行。'), {
        code: 'WRITE_BUSY',
        hint: '等待当前操作结束后重试。',
      }),
    );
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    expect(mocks.permitStatus).toHaveBeenCalledWith('srv_1');
    expect(wrapper.text()).toContain('写操作正在进行。');
    expect(wrapper.text()).toContain('等待当前操作结束后重试。');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    expect(wrapper.find('[data-action="preview"]').text()).toContain('重新预览');
    wrapper.unmount();
  });

  it('turns an expired preview into a re-preview action', async () => {
    mocks.writePreview
      .mockResolvedValueOnce(preview({ expiresAtMs: Date.now() - 1000 }))
      .mockResolvedValueOnce(preview({ previewId: 'p2' }));
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    const again = wrapper.get('[data-action="repreview"]');
    expect(again.text()).toContain('重新预览');

    await again.trigger('click');
    await flushPromises();
    expect(mocks.writePreview).toHaveBeenCalledTimes(2);
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(true);
    wrapper.unmount();
  });

  it('confirms with Enter once the impact preview is shown', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockResolvedValueOnce({ auditId: 11, path: '/data/reports' });
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await flushPromises();
    expect(mocks.writeExecute).toHaveBeenCalledWith('p1');
    wrapper.unmount();
  });

  it('closes on Escape and drops the pending preview', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    expect(guardState.preview).not.toBeNull();

    document.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }),
    );
    await flushPromises();
    expect(wrapper.emitted('close')).toHaveLength(1);
    expect(guardState.preview).toBeNull();
    wrapper.unmount();
  });

  it('names the host on the confirmation but calls the store with the server id', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockResolvedValueOnce({ auditId: 12, path: '/data/reports' });
    const wrapper = mountDialog('srv_1', 'web-01');

    expect(wrapper.get('.new-folder__target').text()).toContain('web-01');
    await previewName(wrapper, 'reports');
    // 弹层浮在整个桌面上：目标一节必须写清是哪台主机，路径本身不足以定位。
    expect(wrapper.findAll('.new-folder__section dd')[1].text()).toBe('web-01 · /data/reports');

    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    // 展示名只用于展示：发给后端的始终是 server id。
    expect(mocks.writePreview).toHaveBeenCalledWith({
      serverId: 'srv_1',
      dir: '/data',
      name: 'reports',
    });
    expect(mocks.writeExecute).toHaveBeenCalledWith('p1');
    const calls = [
      ...mocks.writePreview.mock.calls,
      ...mocks.writeExecute.mock.calls,
      ...mocks.permitStatus.mock.calls,
    ];
    expect(JSON.stringify(calls)).not.toContain('web-01');
    wrapper.unmount();
  });

  it('turns a PREVIEW_EXPIRED execute failure into a re-preview action', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockRejectedValueOnce(
      Object.assign(new Error('预览已过期，请重新预览。'), {
        code: 'PREVIEW_EXPIRED',
        hint: '重新预览后再确认。',
      }),
    );
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('预览已过期');
    expect(wrapper.text()).toContain('预览已过期，请重新预览。');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    expect(wrapper.get('[data-action="preview"]').text()).toContain('重新预览');
    // 什么都没执行，没有可查看的审计。
    expect(wrapper.find('[data-action="view-audit"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it('rechecks the clock on confirm so a stale countdown cannot execute', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(true);

    // 倒计时每秒才跳一次：把系统时间推过有效期，本地 nowMs 仍是旧值。
    vi.setSystemTime(Date.now() + 60_500);
    await wrapper.get('[data-action="confirm"]').trigger('click');
    await flushPromises();

    expect(mocks.writeExecute).not.toHaveBeenCalled();
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    expect(wrapper.get('[data-action="repreview"]').text()).toContain('重新预览');
    wrapper.unmount();
  });

  it('executes exactly once when confirm is pressed twice', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    let finish: ((value: unknown) => void) | undefined;
    mocks.writeExecute.mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    await wrapper.get('[data-action="confirm"]').trigger('click');
    // 执行中再点一次、再按一次 Enter，都只能落到同一条预览上。
    await wrapper.get('[data-action="confirm"]').trigger('click');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await flushPromises();
    expect(mocks.writeExecute).toHaveBeenCalledTimes(1);

    finish?.({ auditId: 13, path: '/data/reports' });
    await flushPromises();
    expect(wrapper.emitted('created')).toHaveLength(1);
    wrapper.unmount();
  });

  it('drops the preview when the target directory changes', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    const wrapper = mountDialog();

    await previewName(wrapper, 'reports');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(true);

    await wrapper.setProps({ dir: '/data/other' });
    await flushPromises();
    expect(guardState.preview).toBeNull();
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it('never confirms a preview for a different target', () => {
    guardState.preview = preview({ target: '/data/other' }) as never;
    guardState.previewServerId = 'srv_1';
    const wrapper = mountDialog('srv_1');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    wrapper.unmount();
  });

  it('never confirms a preview that belongs to another server', () => {
    guardState.preview = preview() as never;
    guardState.previewServerId = 'srv_other';
    const wrapper = mountDialog('srv_1');
    expect(wrapper.find('[data-action="confirm"]').exists()).toBe(false);
    wrapper.unmount();
  });
});
