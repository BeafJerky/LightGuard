import { mount, enableAutoUnmount } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { PermitStatus } from '../ssh';
import PermitControl from './PermitControl.vue';
import { resetGuard } from './store';

// 面板开关是全局状态，多个未卸载的控件会同时响应 Escape 并各自抢焦点。
enableAutoUnmount(afterEach);

function status(patch: Partial<PermitStatus> = {}): PermitStatus {
  return {
    serverId: 'srv_1',
    locked: true,
    minutes: 15,
    expiresAtMs: 0,
    writeActive: false,
    ...patch,
  };
}

const base = {
  serverId: 'srv_1',
  defaultMinutes: 15 as const,
  writeActive: false,
  nowMs: 0,
  autoLocked: false,
};

function mountControl(patch: Partial<PermitStatus> = {}, extra: Record<string, unknown> = {}) {
  // jsdom 只给挂在文档上的元素设置 activeElement，Escape 用例要断言焦点回位。
  return mount(PermitControl, {
    attachTo: document.body,
    props: { ...base, status: status(patch), ...extra },
  });
}

beforeEach(() => {
  resetGuard();
});

describe('permit control', () => {
  it('opens the panel on the default duration while locked', async () => {
    const wrapper = mountControl();
    expect(wrapper.get('.permit-control__trigger').text()).toContain('已锁定');
    await wrapper.get('.permit-control__trigger').trigger('click');
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
    expect(wrapper.get('[data-minutes="15"]').attributes('aria-pressed')).toBe('true');
  });

  it('emits the chosen unlock duration', async () => {
    const wrapper = mountControl();
    await wrapper.get('.permit-control__trigger').trigger('click');
    await wrapper.get('[data-minutes="30"]').trigger('click');
    await wrapper.get('.permit-control__primary').trigger('click');
    expect(wrapper.emitted('unlock')?.[0]).toEqual([30]);
  });

  it('shows the local countdown and can lock again', async () => {
    const wrapper = mountControl({ locked: false, expiresAtMs: 90_000 });
    expect(wrapper.get('.permit-control__trigger').text()).toContain('01:30');
    await wrapper.get('.permit-control__trigger').trigger('click');
    await wrapper.get('.permit-control__secondary').trigger('click');
    expect(wrapper.emitted('lock')).toHaveLength(1);
  });

  it('marks the expiring window and announces auto lock once', () => {
    const wrapper = mountControl({ locked: false, expiresAtMs: 42_000 }, { autoLocked: true });
    expect(wrapper.get('.permit-control').classes()).toContain('is-expiring');
    expect(wrapper.get('[role="status"]').text()).toBe('已自动上锁');
    expect(wrapper.get('.permit-control__trigger').attributes('aria-label')).toContain('即将到期');
  });

  it('closes on Escape and returns focus to the trigger', async () => {
    const wrapper = mountControl();
    const trigger = wrapper.get('.permit-control__trigger');
    await trigger.trigger('click');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    await wrapper.vm.$nextTick();
    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(document.activeElement).toBe(trigger.element);
  });
});
