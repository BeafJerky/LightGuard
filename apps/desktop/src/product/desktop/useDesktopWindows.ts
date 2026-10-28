import { ref } from 'vue';
import { appMeta, type DesktopAppKey } from './apps';

export interface DesktopWindowState {
  app: DesktopAppKey;
  x: number;
  y: number;
  w: number;
  h: number;
  z: number;
  minimized: boolean;
  maximized: boolean;
  restore: { x: number; y: number; w: number; h: number } | null;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

const MIN_W = 320;
const MIN_H = 240;
const TASKBAR_HEIGHT = 54;

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function fitRect(rect: Rect, width: number, height: number, topInset: number) {
  const usableH = Math.max(120, height - TASKBAR_HEIGHT);
  const maxW = Math.max(160, width);
  const maxH = Math.max(120, usableH - topInset);
  rect.w = clamp(rect.w, Math.min(MIN_W, maxW), maxW);
  rect.h = clamp(rect.h, Math.min(MIN_H, maxH), maxH);
  rect.x = clamp(rect.x, 0, Math.max(0, width - rect.w));
  rect.y = clamp(rect.y, topInset, Math.max(topInset, usableH - rect.h));
}

export function useDesktopWindows(
  getBounds: () => { width: number; height: number },
  getTopInset: () => number = () => 0,
) {
  const windows = ref<DesktopWindowState[]>([]);
  const active = ref<DesktopAppKey | null>(null);
  let z = 30;

  function open(key: DesktopAppKey) {
    const existing = windows.value.find((win) => win.app === key);
    if (existing) {
      existing.minimized = false;
      focus(key);
      return;
    }
    const meta = appMeta(key);
    const { width: shellWidth, height: shellHeight } = getBounds();
    const W = Math.max(360, shellWidth || window.innerWidth);
    const H = Math.max(320, shellHeight || window.innerHeight);
    const w = clamp(meta.width, 320, Math.max(320, W - 40));
    const h = clamp(meta.height, 260, Math.max(260, H - 110));
    const topInset = Math.max(0, getTopInset());
    const offset = (windows.value.length % 5) * 26;
    const x = clamp(Math.round((W - w) / 2) - 50 + offset, 10, Math.max(10, W - w - 10));
    const y = clamp(64 + offset, topInset + 10, Math.max(topInset + 10, H - h - 66));
    windows.value.push({
      app: key,
      x,
      y,
      w,
      h,
      z: ++z,
      minimized: false,
      maximized: false,
      restore: null,
    });
    active.value = key;
  }

  function focus(key: DesktopAppKey) {
    const win = windows.value.find((entry) => entry.app === key);
    if (!win) return;
    win.minimized = false;
    win.z = ++z;
    active.value = key;
  }

  function minimize(key: DesktopAppKey) {
    const win = windows.value.find((entry) => entry.app === key);
    if (!win) return;
    win.minimized = true;
    if (active.value === key) active.value = null;
  }

  function toggleMaximize(key: DesktopAppKey) {
    const win = windows.value.find((entry) => entry.app === key);
    if (!win) return;
    if (win.maximized) {
      win.maximized = false;
      if (win.restore) {
        win.x = win.restore.x;
        win.y = win.restore.y;
        win.w = win.restore.w;
        win.h = win.restore.h;
      }
    } else {
      win.restore = { x: win.x, y: win.y, w: win.w, h: win.h };
      win.maximized = true;
    }
    focus(key);
  }

  function close(key: DesktopAppKey) {
    windows.value = windows.value.filter((entry) => entry.app !== key);
    if (active.value === key) active.value = null;
  }

  function toggleTaskbar(key: DesktopAppKey) {
    const win = windows.value.find((entry) => entry.app === key);
    if (!win) {
      open(key);
      return;
    }
    if (win.minimized) {
      focus(key);
      return;
    }
    if (active.value === key) minimize(key);
    else focus(key);
  }

  function showDesktop() {
    for (const win of windows.value) win.minimized = true;
    active.value = null;
  }

  function fitToBounds() {
    const { width, height } = getBounds();
    if (!Number.isFinite(width) || !Number.isFinite(height)) return;
    // 窗口隐藏或全屏切换的瞬间会量到接近 0 的尺寸，此时收敛会把布局压坏
    if (width < 200 || height < 160) return;
    const topInset = Math.max(0, getTopInset());
    for (const win of windows.value) {
      fitRect(win, width, height, topInset);
      if (win.restore) fitRect(win.restore, width, height, topInset);
    }
  }

  function move(key: DesktopAppKey, x: number, y: number) {
    const win = windows.value.find((entry) => entry.app === key);
    if (!win) return;
    win.x = x;
    win.y = y;
  }

  function resize(key: DesktopAppKey, x: number, y: number, w: number, h: number) {
    const win = windows.value.find((entry) => entry.app === key);
    if (!win) return;
    win.x = x;
    win.y = y;
    win.w = w;
    win.h = h;
    win.maximized = false;
    win.restore = null;
  }

  return {
    windows,
    active,
    open,
    focus,
    minimize,
    toggleMaximize,
    close,
    toggleTaskbar,
    showDesktop,
    move,
    resize,
    fitToBounds,
  };
}
