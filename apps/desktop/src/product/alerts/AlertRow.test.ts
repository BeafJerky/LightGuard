import { mount } from '@vue/test-utils';
import { describe, expect, it } from 'vitest';
import type { AlertItem } from '../ssh';
import AlertRow from './AlertRow.vue';

function alert(patch: Partial<AlertItem> = {}): AlertItem {
  return {
    id: 7,
    serverId: 'srv_1',
    serverName: 'web-01',
    ruleId: 'disk',
    severity: 'warning',
    status: 'active',
    title: '磁盘 /data 使用率 92%',
    detail: '阈值 90%，已持续 6 分钟',
    firstSeenAt: 1_000_000,
    lastSeenAt: 1_360_000,
    resolvedAt: null,
    ignoreReason: '',
    ignoredUntil: null,
    ...patch,
  };
}

const nowMs = 1_360_000;

describe('alert row', () => {
  it('expresses severity and status with words', () => {
    const wrapper = mount(AlertRow, { props: { alert: alert(), nowMs } });
    expect(wrapper.text()).toContain('警告');
    expect(wrapper.text()).toContain('磁盘 /data 使用率 92%');
    expect(wrapper.text()).toContain('活动中');
    expect(wrapper.text()).toContain('6 分钟前');
  });

  it('requires an ignore reason before emitting', async () => {
    const wrapper = mount(AlertRow, { props: { alert: alert(), nowMs } });
    await wrapper.get('[data-action="ignore"]').trigger('click');
    await wrapper.get('[data-action="confirm-ignore"]').trigger('click');
    expect(wrapper.emitted('ignore')).toBeUndefined();
    expect(wrapper.text()).toContain('请填写忽略原因');

    await wrapper.get('input[aria-label="忽略原因"]').setValue('计划维护');
    await wrapper.get('[data-action="confirm-ignore"]').trigger('click');
    const payload = wrapper.emitted('ignore')?.[0]?.[0] as { reason: string; untilMs: number };
    expect(payload.reason).toBe('计划维护');
    expect(payload.untilMs).toBeGreaterThan(Date.now());
  });

  it('mutes a server or a single rule', async () => {
    const wrapper = mount(AlertRow, { props: { alert: alert(), nowMs } });
    await wrapper.get('[data-action="mute"]').trigger('click');
    await wrapper.get('[data-action="confirm-mute"]').trigger('click');
    const payload = wrapper.emitted('mute')?.[0]?.[0] as {
      serverId: string;
      ruleId: string | null;
      untilMs: number;
    };
    expect(payload.serverId).toBe('srv_1');
    expect(payload.ruleId).toBe('disk');
    expect(payload.untilMs).toBeGreaterThan(Date.now());
  });

  it('hides write actions once handled and shows the result', () => {
    const wrapper = mount(AlertRow, {
      props: { alert: alert({ status: 'resolved', resolvedAt: nowMs }), nowMs },
    });
    expect(wrapper.text()).toContain('已恢复');
    expect(wrapper.find('[data-action="ignore"]').exists()).toBe(false);
    expect(wrapper.find('[data-action="mute"]').exists()).toBe(false);
    expect(wrapper.find('[data-action="view-server"]').exists()).toBe(true);
  });
});

describe('alert row accessibility', () => {
  it('spells severity out for every level instead of relying on color', () => {
    const critical = mount(AlertRow, { props: { alert: alert({ severity: 'critical' }), nowMs } });
    const info = mount(AlertRow, { props: { alert: alert({ severity: 'info' }), nowMs } });
    expect(critical.text()).toContain('紧急');
    expect(info.text()).toContain('提示');
  });

  it('names every action and reports which panel is open', async () => {
    const wrapper = mount(AlertRow, { props: { alert: alert(), nowMs } });
    expect(wrapper.get('[data-action="view-server"]').attributes('aria-label')).toContain('web-01');

    const mute = wrapper.get('[data-action="mute"]');
    expect(mute.attributes('aria-expanded')).toBe('false');
    await mute.trigger('click');
    expect(wrapper.get('[data-action="mute"]').attributes('aria-expanded')).toBe('true');
    expect(wrapper.get('[data-action="ignore"]').attributes('aria-expanded')).toBe('false');

    await wrapper.get('[data-action="ignore"]').trigger('click');
    expect(wrapper.get('[data-action="ignore"]').attributes('aria-expanded')).toBe('true');
    expect(wrapper.get('[data-action="mute"]').attributes('aria-expanded')).toBe('false');
  });

  it('closes the open panel on Escape without emitting', async () => {
    const wrapper = mount(AlertRow, { props: { alert: alert(), nowMs } });
    await wrapper.get('[data-action="mute"]').trigger('click');
    await wrapper.get('[data-action="confirm-mute"]').trigger('keydown', { key: 'Escape' });
    expect(wrapper.find('[data-action="confirm-mute"]').exists()).toBe(false);
    expect(wrapper.get('[data-action="mute"]').attributes('aria-expanded')).toBe('false');
    expect(wrapper.emitted('mute')).toBeUndefined();
  });

  it('refuses an over-long reason with the shared validator message', async () => {
    const wrapper = mount(AlertRow, { props: { alert: alert(), nowMs } });
    await wrapper.get('[data-action="ignore"]').trigger('click');
    await wrapper.get('input[aria-label="忽略原因"]').setValue('x'.repeat(201));
    await wrapper.get('[data-action="confirm-ignore"]').trigger('click');
    expect(wrapper.emitted('ignore')).toBeUndefined();
    expect(wrapper.get('.alert-row__error').text()).toBe('忽略原因不能超过 200 个字符');
  });

  it('shows the recorded reason on an ignored row', () => {
    const wrapper = mount(AlertRow, {
      props: { alert: alert({ status: 'ignored', ignoreReason: '计划维护' }), nowMs },
    });
    expect(wrapper.text()).toContain('已忽略');
    expect(wrapper.text()).toContain('忽略原因：计划维护');
  });
});
