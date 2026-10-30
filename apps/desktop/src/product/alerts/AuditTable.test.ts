import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  auditList: vi.fn(),
  rollbackAudit: vi.fn(),
  confirm: vi.fn(),
  toast: vi.fn(),
  guardState: { error: '', errorHint: '' },
}));

vi.mock('../ssh', () => ({
  ssh: { auditList: mocks.auditList },
  toSshError: (error: unknown) => error,
}));

vi.mock('../guard/store', () => ({
  guardState: mocks.guardState,
  rollbackAudit: mocks.rollbackAudit,
}));

vi.mock('../ui', () => ({
  confirm: mocks.confirm,
  toast: mocks.toast,
}));

import AuditTable from './AuditTable.vue';

function row() {
  return {
    id: 9,
    atMs: 1_700_000_000_000,
    serverId: 'srv_1',
    serverName: 'web-01',
    op: 'docker.container.stop',
    params: `{"action":"stop","containerId":"${'a'.repeat(64)}","name":"web（aaaaaaaaaaaa）"}`,
    result: 'success',
    durationMs: 632,
    permitSource: 'unlock:15m',
    rollbackOf: null,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.guardState.error = '';
  mocks.guardState.errorHint = '';
  mocks.auditList.mockResolvedValue([row()]);
  mocks.confirm.mockResolvedValue(true);
});

describe('audit table rollback', () => {
  it('shows the concrete backend error instead of the generic hint', async () => {
    // 实机 0.13.0 复现：回退启动被拒时只显示「请检查 Docker 状态后重试。」，
    // 把「容器已在运行」盖掉了。
    mocks.guardState.error = '容器已在运行';
    mocks.guardState.errorHint = '请检查 Docker 状态后重试。';
    mocks.rollbackAudit.mockResolvedValue(false);

    const wrapper = mount(AuditTable);
    await flushPromises();
    await wrapper.find('.audit__row').trigger('click');
    await wrapper.find('.audit__rollback').trigger('click');
    await flushPromises();

    expect(mocks.confirm).toHaveBeenCalled();
    expect(mocks.rollbackAudit).toHaveBeenCalledWith(9);
    expect(wrapper.text()).toContain('回退失败：容器已在运行');
    expect(wrapper.text()).not.toContain('回退失败：请检查 Docker 状态后重试。');
  });

  it('falls back to the hint when the backend reports no message', async () => {
    mocks.guardState.error = '';
    mocks.guardState.errorHint = '请检查 Docker 状态后重试。';
    mocks.rollbackAudit.mockResolvedValue(false);

    const wrapper = mount(AuditTable);
    await flushPromises();
    await wrapper.find('.audit__row').trigger('click');
    await wrapper.find('.audit__rollback').trigger('click');
    await flushPromises();

    expect(wrapper.text()).toContain('回退失败：请检查 Docker 状态后重试。');
  });
});
