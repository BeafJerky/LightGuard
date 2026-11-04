import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref, type Ref } from 'vue';
import { AUTO_HIDE_DELAY, useAutoHideChrome } from './useAutoHideChrome';

interface SetupOptions {
  pointerInside?: () => boolean;
  suppressed?: () => boolean;
}

function setup(active: Ref<boolean>, options: SetupOptions = {}) {
  const scope = effectScope();
  const chrome = scope.run(() =>
    useAutoHideChrome({
      active: () => active.value,
      isPointerInside: options.pointerInside,
      isSuppressed: options.suppressed,
    }),
  );
  if (!chrome) throw new Error('useAutoHideChrome 必须在 effect scope 内运行');
  return { chrome, scope };
}

describe('desktop chrome auto hide', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('生效时到点隐藏，失效时立刻恢复', async () => {
    const active = ref(true);
    const { chrome, scope } = setup(active);

    vi.advanceTimersByTime(AUTO_HIDE_DELAY);
    expect(chrome.hidden.value).toBe(true);

    active.value = false;
    await nextTick();
    expect(chrome.hidden.value).toBe(false);
    vi.advanceTimersByTime(AUTO_HIDE_DELAY * 3);
    expect(chrome.hidden.value).toBe(false);

    scope.stop();
  });

  it('未生效时不隐藏，生效后才开始计时', async () => {
    const active = ref(false);
    const { chrome, scope } = setup(active);

    vi.advanceTimersByTime(AUTO_HIDE_DELAY * 3);
    expect(chrome.hidden.value).toBe(false);

    active.value = true;
    await nextTick();
    vi.advanceTimersByTime(AUTO_HIDE_DELAY - 1);
    expect(chrome.hidden.value).toBe(false);
    vi.advanceTimersByTime(1);
    expect(chrome.hidden.value).toBe(true);

    scope.stop();
  });

  it('呼出后取消计时，离开后重新计时', () => {
    const { chrome, scope } = setup(ref(true));

    vi.advanceTimersByTime(AUTO_HIDE_DELAY);
    expect(chrome.hidden.value).toBe(true);

    chrome.onEnter();
    expect(chrome.hidden.value).toBe(false);
    vi.advanceTimersByTime(AUTO_HIDE_DELAY * 3);
    expect(chrome.hidden.value).toBe(false);

    chrome.onLeave();
    vi.advanceTimersByTime(AUTO_HIDE_DELAY);
    expect(chrome.hidden.value).toBe(true);

    scope.stop();
  });

  it('指针仍停在元素上时顺延隐藏', () => {
    let inside = true;
    const { chrome, scope } = setup(ref(true), { pointerInside: () => inside });

    vi.advanceTimersByTime(AUTO_HIDE_DELAY * 3);
    expect(chrome.hidden.value).toBe(false);

    inside = false;
    vi.advanceTimersByTime(AUTO_HIDE_DELAY);
    expect(chrome.hidden.value).toBe(true);

    scope.stop();
  });

  it('浮层打开期间不隐藏', () => {
    let suppressed = true;
    const { chrome, scope } = setup(ref(true), { suppressed: () => suppressed });

    vi.advanceTimersByTime(AUTO_HIDE_DELAY * 2);
    expect(chrome.hidden.value).toBe(false);

    suppressed = false;
    vi.advanceTimersByTime(AUTO_HIDE_DELAY);
    expect(chrome.hidden.value).toBe(true);

    scope.stop();
  });

  it('scope 销毁后不再触发隐藏', () => {
    const { chrome, scope } = setup(ref(true));
    scope.stop();

    vi.advanceTimersByTime(AUTO_HIDE_DELAY * 2);
    expect(chrome.hidden.value).toBe(false);
  });
});
