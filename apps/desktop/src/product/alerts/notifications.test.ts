import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  onAction: vi.fn(),
  show: vi.fn(async () => {}),
  unminimize: vi.fn(async () => {}),
  setFocus: vi.fn(async () => {}),
}));

vi.mock('@tauri-apps/plugin-notification', () => ({ onAction: mocks.onAction }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    show: mocks.show,
    unminimize: mocks.unminimize,
    setFocus: mocks.setFocus,
  }),
}));
// 产品路由同时导出 navigate 与 navigateTo；通知跳转只用按名字跳转的后者。
vi.mock('../router', () => ({ navigateTo: vi.fn() }));
vi.mock('../ssh/tauri', () => ({ isTauriRuntime: () => true }));
vi.mock('./store', () => ({
  alertsState: { focusAlertId: null as number | null },
  focusAlert: vi.fn(async () => {}),
}));

import { navigateTo } from '../router';
import { focusAlert } from './store';
import { startNotificationActions } from './notifications';

function handler() {
  return mocks.onAction.mock.calls[0][0] as (event: {
    actionId: string;
    notification: { id: number; extra: Record<string, unknown> } | null;
  }) => Promise<void>;
}

describe('notification actions', () => {
  beforeEach(() => {
    // 全部 spy 一起清：navigateTo/focusAlert 与窗口 spy 都按调用次数断言。
    vi.clearAllMocks();
    mocks.onAction.mockResolvedValue(() => {});
  });

  it('routes a tap to the alert center with the alert id', async () => {
    await startNotificationActions();
    await handler()({
      actionId: 'tap',
      notification: { id: 7, extra: { alertId: 42 } },
    });
    expect(navigateTo).toHaveBeenCalledWith('alerts');
    expect(focusAlert).toHaveBeenCalledWith(42);
  });

  it('restores the window before navigating from a notification tap', async () => {
    await startNotificationActions();
    await handler()({ actionId: 'tap', notification: { id: 8, extra: { alertId: 3 } } });
    expect(mocks.show).toHaveBeenCalledTimes(1);
    expect(mocks.unminimize).toHaveBeenCalledTimes(1);
    expect(mocks.setFocus).toHaveBeenCalledTimes(1);
    expect(mocks.show.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.unminimize.mock.invocationCallOrder[0],
    );
    expect(mocks.unminimize.mock.invocationCallOrder[0]).toBeLessThan(
      mocks.setFocus.mock.invocationCallOrder[0],
    );
  });

  it('ignores actions other than tap', async () => {
    await startNotificationActions();
    await handler()({ actionId: 'dismiss', notification: { id: 9, extra: { alertId: 42 } } });
    expect(navigateTo).not.toHaveBeenCalled();
    expect(focusAlert).not.toHaveBeenCalled();
    expect(mocks.show).not.toHaveBeenCalled();
  });

  it('coerces an alert id delivered as a numeric string', async () => {
    await startNotificationActions();
    await handler()({ actionId: 'tap', notification: { id: 10, extra: { alertId: '42' } } });
    expect(focusAlert).toHaveBeenCalledWith(42);
  });

  it('navigates without focusing when the notification carries no id', async () => {
    await startNotificationActions();
    await handler()({ actionId: '', notification: { id: 0, extra: {} } });
    expect(navigateTo).toHaveBeenCalledWith('alerts');
    expect(focusAlert).not.toHaveBeenCalled();
  });
});
