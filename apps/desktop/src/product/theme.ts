import type { ThemeId } from '@lightguard/lightui';
import type { ThemeChoices } from './types';
import { setTheme } from './store';

type ViewTransitionDocument = Document & {
  startViewTransition?: (callback: () => void) => {
    finished: Promise<unknown>;
    ready: Promise<unknown>;
  };
};

function reflow() {
  // 在回调内同步改动后，Vue 的 DOM 更新落在微任务里；等待一帧让新主题先落盘再结束过渡。
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => resolve());
  });
}

export function switchScopeTheme(scope: keyof ThemeChoices, id: ThemeId) {
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
