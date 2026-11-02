import { beforeEach, describe, expect, it, vi } from 'vitest';
import { setAutostart, syncAutostart, type AutostartBridge } from './autostart';
import { updateSettings, state } from '../store';
import { toasts } from '../ui';

function bridge(initial: boolean): AutostartBridge {
  let enabled = initial;
  return {
    isEnabled: vi.fn(async () => enabled),
    enable: vi.fn(async () => {
      enabled = true;
    }),
    disable: vi.fn(async () => {
      enabled = false;
    }),
  };
}

beforeEach(() => {
  updateSettings({ autostart: false });
  toasts.splice(0, toasts.length);
});

describe('autostart controller', () => {
  it('enables the OS entry before persisting the setting', async () => {
    const api = bridge(false);
    await expect(setAutostart(true, api)).resolves.toBe(true);
    expect(api.enable).toHaveBeenCalledOnce();
    expect(state.settings.autostart).toBe(true);
  });

  it('rolls the setting back when the OS call fails', async () => {
    const api = bridge(false);
    api.enable = vi.fn(async () => {
      throw new Error('access denied');
    });
    await expect(setAutostart(true, api)).resolves.toBe(false);
    expect(state.settings.autostart).toBe(false);
  });

  it('mirrors an existing registry entry into settings', async () => {
    const api = bridge(true);
    await expect(syncAutostart(api)).resolves.toBe(true);
    expect(state.settings.autostart).toBe(true);
  });

  it('warns once and keeps the local value when the OS state cannot be read', async () => {
    updateSettings({ autostart: true });
    const api = bridge(false);
    api.isEnabled = vi.fn(async () => {
      throw new Error('access denied');
    });

    await expect(syncAutostart(api)).resolves.toBe(true);
    expect(state.settings.autostart).toBe(true);
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toMatchObject({ tone: 'warning' });
  });
});
