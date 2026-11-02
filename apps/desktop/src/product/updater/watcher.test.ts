import { afterEach, describe, expect, it, vi } from 'vitest';
import { UPDATE_INTERVAL_MS, UPDATE_STARTUP_DELAY_MS, startUpdateWatcher } from './watcher';

afterEach(() => {
  vi.useRealTimers();
});

describe('update watcher', () => {
  it('waits for the startup delay, then repeats on the 24 hour interval', () => {
    vi.useFakeTimers();
    const check = vi.fn(async () => {});
    const stop = startUpdateWatcher({ check });

    expect(check).not.toHaveBeenCalled();
    vi.advanceTimersByTime(UPDATE_STARTUP_DELAY_MS - 1);
    expect(check).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    expect(check).toHaveBeenCalledWith({ manual: false });

    vi.advanceTimersByTime(UPDATE_INTERVAL_MS);
    expect(check).toHaveBeenCalledTimes(2);
    stop();
  });

  it('stops both timers', () => {
    vi.useFakeTimers();
    const check = vi.fn(async () => {});
    const stop = startUpdateWatcher({ check }, { delayMs: 10, intervalMs: 20 });
    stop();
    vi.advanceTimersByTime(100);
    expect(check).not.toHaveBeenCalled();
  });
});
