import { flushPromises, mount } from '@vue/test-utils';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  alertsList: vi.fn(),
  alertsIgnore: vi.fn(),
  alertsMute: vi.fn(),
  auditList: vi.fn(),
  onAlertsChanged: vi.fn(() => () => {}),
}));

vi.mock('../ssh', () => ({
  ssh: mocks,
  toSshError: (error: unknown) => (error && typeof error === 'object' ? error : new Error('失败')),
}));

import { route } from '../router';
import { resetAlerts } from './store';
import AlertsPage from './AlertsPage.vue';

function item(id: number) {
  return {
    id,
    serverId: 'srv_1',
    serverName: 'web-01',
    ruleId: 'cpu',
    severity: 'warning' as const,
    status: 'active' as const,
    title: `CPU 使用率 93% #${id}`,
    detail: '阈值 90%',
    firstSeenAt: Date.now() - 6 * 60_000,
    lastSeenAt: Date.now(),
    resolvedAt: null,
    ignoreReason: '',
    ignoredUntil: null,
  };
}

beforeEach(() => {
  resetAlerts();
  vi.clearAllMocks();
  route.value = { name: 'alerts', tab: 'alerts' };
  mocks.alertsList.mockResolvedValue({
    items: [item(1), item(2)],
    activeCount: 2,
    loginMonitor: [{ serverId: 'srv_2', available: false, reason: '无登录日志读取权限' }],
  });
  mocks.auditList.mockResolvedValue([]);
});

describe('alerts page', () => {
  it('loads open alerts and renders rows with the login monitor note', async () => {
    const wrapper = mount(AlertsPage);
    await flushPromises();
    expect(mocks.alertsList).toHaveBeenCalledWith('open', 200, 0);
    expect(wrapper.findAll('.alert-row')).toHaveLength(2);
    expect(wrapper.text()).toContain('无登录日志读取权限');
  });

  it('switches the handled filter through the store', async () => {
    const wrapper = mount(AlertsPage);
    await flushPromises();
    const handled = wrapper.findAll('.alerts-filters button').find((node) => node.text() === '已处理');
    await handled?.trigger('click');
    await flushPromises();
    expect(mocks.alertsList).toHaveBeenLastCalledWith('handled', 200, 0);
  });

  it('opens the audit tab from the route and loads audit rows', async () => {
    route.value = { name: 'alerts', tab: 'audit' };
    const wrapper = mount(AlertsPage);
    await flushPromises();
    expect(mocks.auditList).toHaveBeenCalledWith(200, 0);
    expect(wrapper.text()).toContain('操作审计');
    expect(wrapper.text()).toContain('暂无操作审计记录');
  });
});
