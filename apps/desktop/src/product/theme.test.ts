// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const store = vi.hoisted(() => ({
  state: { themes: { console: 'nes', desktop: 'nes' } as Record<string, string> },
  setTheme: vi.fn(),
}));

vi.mock('./store', () => ({
  state: store.state,
  setTheme: store.setTheme,
}));

import { switchScopeTheme } from './theme';

function stubMatchMedia() {
  vi.stubGlobal(
    'matchMedia',
    vi.fn(() => ({ matches: false, addEventListener() {}, removeEventListener() {} })),
  );
}

describe('switchScopeTheme', () => {
  beforeEach(() => {
    store.state.themes.console = 'nes';
    store.state.themes.desktop = 'nes';
    store.setTheme.mockReset();
    store.setTheme.mockImplementation((scope: string, id: string) => {
      store.state.themes[scope] = id;
    });
    stubMatchMedia();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    delete (document as { startViewTransition?: unknown }).startViewTransition;
  });

  it('resolves the view transition update without waiting for an animation frame', async () => {
    // 回归：回调里等 rAF 会让 View Transition 死锁——浏览器在回调 resolve 前不派发帧。
    vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1));
    let updateSettled = false;
    (document as { startViewTransition?: unknown }).startViewTransition = (
      callback: () => unknown,
    ) => {
      Promise.resolve(callback()).then(() => {
        updateSettled = true;
      });
      return { ready: Promise.resolve(), finished: Promise.resolve() };
    };

    switchScopeTheme('console', 'dark');
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(updateSettled).toBe(true);
    expect(store.setTheme).toHaveBeenCalledWith('console', 'dark');
  });

  it('applies the theme directly when the browser has no view transitions', () => {
    switchScopeTheme('console', 'light');
    expect(store.setTheme).toHaveBeenCalledWith('console', 'light');
  });

  it('skips the switch when the theme is already active', () => {
    switchScopeTheme('console', 'nes');
    expect(store.setTheme).not.toHaveBeenCalled();
  });
});
