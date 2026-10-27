import { ref } from 'vue';
import type { ThemeId } from '@lightguard/lightui';
import type { ThemeChoices } from './types';
import { setTheme, state } from './store';

type ViewTransitionDocument = Document & {
  startViewTransition?: (callback: () => void) => {
    finished: Promise<unknown>;
    ready: Promise<unknown>;
  };
};

export const themeSwitching = ref(false);
let switching = false;

function reflow() {
  // 在回调内同步改动后，Vue 的 DOM 更新落在微任务里；等待一帧让新主题先落盘再结束过渡。
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve());
  });
}

function withMask(apply: () => void) {
  if (switching) return;
  switching = true;
  themeSwitching.value = true;
  // 先让遮罩完成一帧绘制，再切换主题，避免用户看到旧界面卡住。
  requestAnimationFrame(() => {
    apply();
    window.setTimeout(() => {
      themeSwitching.value = false;
      switching = false;
    }, 280);
  });
}

export function switchScopeTheme(scope: keyof ThemeChoices, id: ThemeId) {
  if (state.themes[scope] === id) return;
  if (scope === 'desktop') {
    // 桌面主题包含壁纸、特效与大量 canvas 重绘，跳过整页 View Transition，改用轻量遮罩。
    withMask(() => setTheme(scope, id));
    return;
  }
  const apply = () => {
    setTheme(scope, id);
  };
  const doc = document as ViewTransitionDocument;
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (reduced || !doc.startViewTransition) {
    apply();
    return;
  }
  const transition = doc.startViewTransition(async () => {
    apply();
    await reflow();
  });
  transition.finished.catch(() => {});
  transition.ready.catch(() => {});
}
