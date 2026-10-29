import { enableAutoUnmount, mount, type VueWrapper } from '@vue/test-utils';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ConnectResult, PermitStatus, ServerSnapshot } from '../ssh';
import { newServer } from '../ssh/adapter';
import type { Server } from '../types';
import ServerDesktop from '../desktop/ServerDesktop.vue';
import PermitControl from './PermitControl.vue';
import { applyPermit, closePermitPanel, resetGuard } from './store';

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
  lockNotice: '' as '' | 'auto' | 'manual',
};

function mountControl(patch: Partial<PermitStatus> = {}, extra: Record<string, unknown> = {}) {
  // jsdom 只给挂在文档上的元素设置 activeElement，Escape 用例要断言焦点回位。
  return mount(PermitControl, {
    attachTo: document.body,
    props: { ...base, status: status(patch), ...extra },
  });
}

/**
 * 倒计时用例开着假时钟，`flushPromises` 依赖 setImmediate（同样被假时钟接管）会挂住，
 * 这里只推进微任务队列：够 Vue 刷新 watcher，也够失败请求的 rejection 走到 finally。
 */
async function settle(wrapper: VueWrapper, rounds = 6) {
  for (let index = 0; index < rounds; index += 1) {
    await Promise.resolve();
    await wrapper.vm.$nextTick();
  }
}

function snapshot(): ServerSnapshot {
  return {
    hostname: 'demo-01',
    os: 'Debian GNU/Linux 12',
    kernel: '6.1.0-18-amd64',
    arch: 'x86_64',
    timezone: 'CST',
    uptimeSeconds: 90061,
    bootTime: '2026-10-24 09:00',
    cpuModel: 'Intel Xeon E5-2680 v4',
    cpuCores: 4,
    cpuFreq: '2400 MHz',
    cpuPercent: 37.4,
    cpuCoresLoad: [31.2, 42.7, 22.1, 53.5],
    load: [0.42, 0.5, 0.61],
    tempC: 52.6,
    memTotalGb: 15.6,
    memAvailableGb: 6.2,
    memPercent: 60.3,
    swapTotalGb: 2,
    swapUsedGb: 0.12,
    diskTotalGb: 240,
    diskPercent: 71.2,
    disks: [
      { mount: '/', fs: '/dev/sda1', kind: 'ext4', totalGb: 240, usedGb: 170.9, inodePercent: 8 },
    ],
    downMbps: 1.234,
    upMbps: 0.456,
    interfaces: [
      {
        name: 'eth0',
        ip: '192.0.2.10',
        mac: 'aa:bb:cc:dd:ee:ff',
        speed: '1 Gbps',
        rxTotalGb: 24.5,
        txTotalGb: 8.25,
        downMbps: 1.2,
        upMbps: 0.4,
      },
    ],
    gateway: '192.0.2.1',
    dns: '223.5.5.5',
    publicIp: '—',
    mtu: '1500',
    processes: [{ pid: 742, name: 'sshd', user: 'root', cpu: 0.4, mem: 0.9, state: 'S' }],
    processCount: 128,
    sampledAt: '2026-10-27T15:30:00+08:00',
  };
}

// 许可见解独立于服务器视图，但桌面会画“系统信息”窗口，所以这里给一份完整的服务器。
function demoServer(): Server {
  const result: ConnectResult = {
    sessionId: 'sess_1',
    startedAt: '2026-10-27T15:00:00+08:00',
    fingerprint: 'SHA256:abc',
    hostKeyState: 'known',
    authLabel: '密码',
    latencyMs: 12,
    snapshot: snapshot(),
  };
  return newServer(
    {
      serverId: 'srv_1',
      auth: 'password',
      draft: {
        name: '演示机',
        ip: '192.0.2.10',
        port: 22,
        user: 'admin',
        auth: 'password',
        group: '测试',
        tags: [],
        role: '通用服务器',
        note: '',
      },
    },
    result,
  );
}

beforeEach(() => {
  resetGuard();
});

afterEach(() => {
  vi.useRealTimers();
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
    const wrapper = mountControl({ locked: false, expiresAtMs: 42_000 }, { lockNotice: 'auto' });
    expect(wrapper.get('.permit-control').classes()).toContain('is-expiring');
    expect(wrapper.get('[role="status"]').text()).toBe('已自动上锁');
    expect(wrapper.get('.permit-control__trigger').attributes('aria-label')).toContain('即将到期');
  });

  it('does not claim a manual lock was automatic', () => {
    const wrapper = mountControl({}, { lockNotice: 'manual' });
    expect(wrapper.get('[role="status"]').text()).toBe('已上锁');
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

  it('returns focus to the trigger when the panel is closed programmatically', async () => {
    const wrapper = mountControl();
    const trigger = wrapper.get('.permit-control__trigger');
    await trigger.trigger('click');

    const inside = wrapper.get('[data-minutes="30"]');
    (inside.element as HTMLElement).focus();
    expect(document.activeElement).toBe(inside.element);

    // 父级在解锁/上锁成功后就是这样关面板的：没有 Escape，焦点元素随面板一起卸载。
    closePermitPanel();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[role="dialog"]').exists()).toBe(false);
    expect(document.activeElement).toBe(trigger.element);
  });

  it('does not steal focus on mount', () => {
    mountControl();
    expect(document.activeElement).toBe(document.body);
  });
});

describe('permit countdown on the server desktop', () => {
  it('restarts the countdown after extending an expired-unlocked permit', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00.000Z'));
    const wrapper = mount(ServerDesktop, { props: { server: demoServer() } });
    await settle(wrapper);

    const connBar = () => wrapper.get('.conn-bar__permit .permit-control__label').text();

    // 初始锁定：计时器不跑。
    expect(connBar()).toBe('已锁定');
    expect(wrapper.find('.desk-taskbar__permit').exists()).toBe(false);

    // 首次解锁生效（store 应用后端结果）：倒计时启动。
    applyPermit(status({ locked: false, expiresAtMs: Date.now() + 5000 }));
    await settle(wrapper);
    expect(connBar()).toBe('00:05');

    vi.advanceTimersByTime(1000);
    await settle(wrapper);
    expect(connBar()).toBe('00:04');
    expect(wrapper.get('.desk-taskbar__permit').text()).toBe('00:04');

    // 到期：本地停表并刷新；没有后端时刷新失败，许可停在“未锁定但已过期”。
    vi.advanceTimersByTime(5000);
    await settle(wrapper);
    expect(connBar()).toBe('00:00');
    vi.advanceTimersByTime(3000);
    await settle(wrapper);
    expect(connBar()).toBe('00:00');

    // 从过期状态延长：locked 始终是 false，只有到期时间变了，计时器必须重新跑起来。
    applyPermit(status({ locked: false, expiresAtMs: Date.now() + 12_000 }));
    await settle(wrapper);
    expect(connBar()).toBe('00:12');

    vi.advanceTimersByTime(1000);
    await settle(wrapper);
    expect(connBar()).toBe('00:11');
    expect(wrapper.get('.desk-taskbar__permit').text()).toBe('00:11');

    // 上锁仍然要停表。
    applyPermit(status({ locked: true, expiresAtMs: 0 }));
    await settle(wrapper);
    expect(connBar()).toBe('已锁定');
    vi.advanceTimersByTime(2000);
    await settle(wrapper);
    expect(connBar()).toBe('已锁定');
  });

  it('keeps the panel open on a failed lock and still calls a later expiry lock automatic', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T00:00:00.000Z'));
    const wrapper = mount(ServerDesktop, { props: { server: demoServer() } });
    await settle(wrapper);

    applyPermit(status({ locked: false, expiresAtMs: Date.now() + 60_000 }));
    await settle(wrapper);

    await wrapper.get('.conn-bar__permit .permit-control__trigger').trigger('click');
    await settle(wrapper);
    await wrapper.get('.permit-control__secondary').trigger('click');
    await settle(wrapper);

    // 无后端时上锁必然失败：面板保留，错误段落可见，而不是悄悄关掉。
    expect(wrapper.find('[role="dialog"]').exists()).toBe(true);
    expect(wrapper.get('.permit-control__error').text()).not.toBe('');

    // 失败的手动上锁不能把之后的到期上锁算成手动。
    applyPermit(status({ locked: true, expiresAtMs: 0 }));
    await settle(wrapper);
    expect(wrapper.get('.permit-control__sr').text()).toBe('已自动上锁');
  });
});
