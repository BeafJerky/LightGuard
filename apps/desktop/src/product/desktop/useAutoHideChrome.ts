import { onScopeDispose, ref, watch, type Ref } from 'vue';

export const AUTO_HIDE_DELAY = 1500;

export interface AutoHideChrome {
  hidden: Ref<boolean>;
  /** 指针进入呼出区或元素本身：立即显示并取消计时。 */
  onEnter: () => void;
  /** 指针离开：开始计时，到点后隐藏。 */
  onLeave: () => void;
}

interface AutoHideChromeOptions {
  /** 自动隐藏是否处于生效状态，例如“悬浮样式 + 开关开启 + 存在最大化窗口”。 */
  active: () => boolean;
  /** 指针是否仍停在元素上；为真时顺延计时，避免界面在鼠标底下被抽走。 */
  isPointerInside?: () => boolean;
  /** 浮层（开始菜单 / 托盘 / 许可面板）打开期间不隐藏。 */
  isSuppressed?: () => boolean;
  delay?: number;
}

export function useAutoHideChrome(options: AutoHideChromeOptions): AutoHideChrome {
  const { active, isPointerInside, isSuppressed, delay = AUTO_HIDE_DELAY } = options;
  const hidden = ref(false);
  let timer: number | undefined;

  function clearTimer() {
    if (timer === undefined) return;
    window.clearTimeout(timer);
    timer = undefined;
  }

  function scheduleHide() {
    clearTimer();
    if (!active()) {
      hidden.value = false;
      return;
    }
    timer = window.setTimeout(() => {
      timer = undefined;
      if (!active()) {
        hidden.value = false;
        return;
      }
      // 浮层打开或指针还停在元素上时顺延，等条件消失后再隐藏。
      if (isSuppressed?.() || isPointerInside?.()) {
        scheduleHide();
        return;
      }
      hidden.value = true;
    }, delay);
  }

  function onEnter() {
    clearTimer();
    hidden.value = false;
  }

  watch(
    active,
    (value) => {
      if (value) scheduleHide();
      else {
        clearTimer();
        hidden.value = false;
      }
    },
    { immediate: true },
  );

  onScopeDispose(clearTimer);

  return { hidden, onEnter, onLeave: scheduleHide };
}
