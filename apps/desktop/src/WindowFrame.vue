<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon, LG_SCOPE_KEY, chromeOf } from '@lightguard/lightui';
import BrandMark from './product/BrandMark.vue';
import { findServer, flushSettings, state, updateSettings } from './product/store';
import { route, routeTitle } from './product/router';
import { promptClose } from './product/ui';
import { isTauriRuntime } from './product/ssh/tauri';
import { isFullscreen, registerFullscreenToggler } from './windowState';

interface AppWindow {
  minimize: () => Promise<void>;
  toggleMaximize: () => Promise<void>;
  hide: () => Promise<void>;
  isMaximized: () => Promise<boolean>;
  isFullscreen: () => Promise<boolean>;
  setFullscreen: (value: boolean) => Promise<void>;
  setBackgroundColor: (color: string) => Promise<void>;
  onResized: (handler: () => void) => Promise<() => void>;
}

const framed = isTauriRuntime();
const scope = inject(LG_SCOPE_KEY, null);
const maximized = ref(false);
let appWindow: AppWindow | null = null;
let unlisten: (() => void) | null = null;
let unlistenClose: (() => void) | null = null;
let closing = false;

const pageLabel = computed(() => {
  const name = route.value.name;
  if (name === 'desktop') {
    const server = route.value.id ? findServer(route.value.id) : null;
    return server ? `${server.name} · 服务器桌面` : '服务器桌面';
  }
  return routeTitle(name);
});

// 窗口框跟随当前作用域主题：macOS 主题用交通灯，其余主题保持 Windows 式控制。
const isMacChrome = computed(() => {
  const theme = scope?.value.theme;
  return theme ? chromeOf(theme).windowControls === 'macos' : false;
});

if (framed && typeof document !== 'undefined') {
  document.documentElement.classList.add('lg-tauri-framed');
}

async function loadWindow() {
  if (!framed) return null;
  if (!appWindow) {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    appWindow = getCurrentWindow() as unknown as AppWindow;
  }
  return appWindow;
}

async function syncState() {
  const win = await loadWindow();
  if (!win) return;
  try {
    maximized.value = await win.isMaximized();
    isFullscreen.value = await win.isFullscreen();
  } catch {
    /* window may be closing */
  }
}

async function minimize() {
  const win = await loadWindow();
  await win?.minimize().catch(() => undefined);
}

async function toggleMaximize() {
  const win = await loadWindow();
  if (!win) return;
  await win.toggleMaximize().catch(() => undefined);
  await syncState();
}

async function applyWindowBackground() {
  const win = await loadWindow();
  const color = scope?.value.theme.color.bg;
  if (!win || !color) return;
  await win.setBackgroundColor(color).catch(() => undefined);
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_window_border_color', { color });
  } catch {
    /* border color is best-effort */
  }
}

async function toggleFullscreen() {
  const win = await loadWindow();
  if (!win) return;
  const next = !isFullscreen.value;
  await win.setFullscreen(next).catch(() => undefined);
  await syncState();
}

async function hideToTray() {
  const win = await loadWindow();
  await win?.hide().catch(() => undefined);
}

async function quitApp() {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('app_quit').catch(() => undefined);
}

async function registerCloseBridge() {
  try {
    const { listen } = await import('@tauri-apps/api/event');
    unlistenClose = await listen('lightguard://close-requested', () => {
      void requestClose();
    });
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('app_close_ready');
  } catch (error) {
    console.warn('[lightguard] 关闭事件监听不可用，系统关闭将直接退出应用', error);
  }
}

async function requestClose() {
  if (!framed || closing) return;
  closing = true;
  try {
    const behavior = state.settings.closeBehavior;
    if (behavior === 'tray') {
      await hideToTray();
      return;
    }
    if (behavior === 'quit') {
      await quitApp();
      return;
    }
    const result = await promptClose();
    if (!result) return;
    if (result.remember) {
      updateSettings({ closeBehavior: result.action });
      if (result.action === 'quit') await flushSettings();
    }
    if (result.action === 'tray') await hideToTray();
    else await quitApp();
  } finally {
    closing = false;
  }
}

function onKeyDown(event: KeyboardEvent) {
  if (event.key !== 'F11') return;
  event.preventDefault();
  void toggleFullscreen();
}

watch(
  () => scope?.value.theme.id,
  () => {
    void applyWindowBackground();
  },
);

onMounted(async () => {
  if (!framed) return;
  void registerCloseBridge();
  await applyWindowBackground();
  await syncState();
  registerFullscreenToggler(() => {
    void toggleFullscreen();
  });
  window.addEventListener('keydown', onKeyDown, true);
  const win = await loadWindow();
  if (!win) return;
  unlisten = await win.onResized(() => {
    void syncState();
  });
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown, true);
  registerFullscreenToggler(null);
  unlisten?.();
  unlisten = null;
  unlistenClose?.();
  unlistenClose = null;
  if (framed) document.documentElement.classList.remove('lg-tauri-framed');
});
</script>

<template>
  <div v-if="framed" class="window-frame" :class="{ 'is-maximized': maximized, 'is-fullscreen': isFullscreen }">
    <header
      v-if="!isFullscreen"
      class="window-bar"
      :class="{ 'is-macos': isMacChrome }"
      data-tauri-drag-region
    >
      <template v-if="isMacChrome">
        <div class="window-bar__traffic" role="group" aria-label="窗口控制">
          <button type="button" class="traffic traffic--close" aria-label="关闭" title="关闭" @click="requestClose">
            <span class="traffic__dot"><span class="traffic__glyph" aria-hidden="true">×</span></span>
          </button>
          <button type="button" class="traffic traffic--min" aria-label="最小化" title="最小化" @click="minimize">
            <span class="traffic__dot"><span class="traffic__glyph" aria-hidden="true">−</span></span>
          </button>
          <button
            type="button"
            class="traffic traffic--max"
            :aria-label="maximized ? '还原' : '最大化'"
            :title="maximized ? '还原' : '最大化'"
            @click="toggleMaximize"
          >
            <span class="traffic__dot"><span class="traffic__glyph" aria-hidden="true">{{ maximized ? '−' : '+' }}</span></span>
          </button>
        </div>
        <div class="window-bar__title" data-tauri-drag-region>{{ pageLabel }}</div>
        <div class="window-bar__traffic-spacer" data-tauri-drag-region />
      </template>
      <template v-else>
        <div class="window-bar__brand" data-tauri-drag-region>
          <BrandMark class="window-bar__mark" :size="24" data-tauri-drag-region />
          <strong data-tauri-drag-region>LightGuard</strong>
          <span class="window-bar__page" data-tauri-drag-region>{{ pageLabel }}</span>
        </div>
        <div class="window-bar__spacer" data-tauri-drag-region />
        <div class="window-bar__controls">
          <button type="button" class="window-bar__control" aria-label="全屏" title="全屏 (F11)" @click="toggleFullscreen">
            <LgIcon name="monitor" :size="12" />
          </button>
          <button type="button" class="window-bar__control" aria-label="最小化" title="最小化" @click="minimize">
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8h10" /></svg>
          </button>
          <button
            type="button"
            class="window-bar__control"
            :aria-label="maximized ? '还原' : '最大化'"
            :title="maximized ? '还原' : '最大化'"
            @click="toggleMaximize"
          >
            <LgIcon :name="maximized ? 'copy' : 'maximize'" :size="12" />
          </button>
          <button
            type="button"
            class="window-bar__control is-close"
            aria-label="关闭"
            title="关闭"
            @click="requestClose"
          >
            <LgIcon name="x" :size="12" />
          </button>
        </div>
      </template>
    </header>

    <div class="window-body">
      <slot />
    </div>

  </div>
  <slot v-else />
</template>

<style scoped>
.window-frame {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100dvh;
  overflow: hidden;
  border: 0;
  border-radius: 0;
  background: var(--lg-bg);
  color: var(--lg-text);
}



.window-bar {
  position: relative;
  z-index: 400;
  display: flex;
  flex: 0 0 40px;
  align-items: center;
  gap: 9px;
  padding-left: 14px;
  border-bottom: var(--lg-border-width) solid var(--lg-border);
  background: color-mix(in srgb, var(--lg-surface) 74%, transparent);
  user-select: none;
}

.window-bar__brand {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.window-bar__brand strong {
  font-size: var(--lg-fs-sm);
  font-weight: 650;
  letter-spacing: 0.01em;
  white-space: nowrap;
}

.window-bar__page {
  overflow: hidden;
  max-width: 360px;
  padding-left: 9px;
  border-left: var(--lg-border-width) solid var(--lg-border);
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.window-bar__spacer {
  flex: 1;
  align-self: stretch;
}

/* macOS 主题：无边框标题栏 + 自绘交通灯，标题居中，去掉 Windows 式控制区。 */
.window-bar.is-macos {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  gap: 0;
  padding: 0 12px 0 10px;
  border-bottom: 1px solid color-mix(in srgb, var(--lg-border-strong) 60%, transparent);
  background: color-mix(in srgb, var(--lg-surface) 72%, transparent);
  backdrop-filter: blur(24px) saturate(1.5);
}

.window-bar__traffic {
  display: flex;
  align-items: center;
  justify-self: start;
}

.traffic {
  display: grid;
  place-items: center;
  width: 20px;
  height: 28px;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
}

.traffic:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: -3px;
}

.traffic__dot {
  display: grid;
  place-items: center;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 16%);
  transition: filter 120ms var(--lg-ease);
}

.traffic--close .traffic__dot {
  background: #ff5f57;
}

.traffic--min .traffic__dot {
  background: #febc2e;
}

.traffic--max .traffic__dot {
  background: #28c840;
}

.traffic:hover .traffic__dot {
  filter: brightness(1.08) saturate(1.12);
}

.traffic:active .traffic__dot {
  filter: brightness(0.9);
}

.traffic__glyph {
  color: rgb(0 0 0 / 55%);
  font: 700 9px/1 "Segoe UI", system-ui, sans-serif;
  opacity: 0;
  transition: opacity 100ms var(--lg-ease);
}

.traffic:hover .traffic__glyph,
.traffic:focus-visible .traffic__glyph {
  opacity: 1;
}

.window-bar__title {
  min-width: 0;
  max-width: min(52vw, 560px);
  overflow: hidden;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  font-weight: 600;
  text-align: center;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.window-bar__traffic-spacer {
  width: 60px;
  justify-self: end;
}

.window-bar__controls {
  display: flex;
  align-self: stretch;
}

.window-bar__control {
  display: grid;
  place-items: center;
  width: 46px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.window-bar__control:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.window-bar__control.is-close:hover {
  background: var(--lg-danger);
  color: #fff;
}

.window-bar__control svg {
  width: 12px;
  height: 12px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
}

.window-body {
  position: relative;
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}</style>

<style>
html.lg-tauri-framed,
html.lg-tauri-framed body,
html.lg-tauri-framed #app,
html.lg-tauri-framed .product-scope {
  height: 100%;
  overflow: hidden;
  overscroll-behavior: none;
  background: transparent;
}

/* 窗口内只允许内容区自身滚动：文档层不滚动，避免触控板在边界处拖动整页、露出窗口底色。
   窗口框按视口定位而不是 100dvh：WebView2 在缩放窗口后 dvh 可能滞后，100% 又会落到
   中间层 auto 高度的容器上，两种做法都可能让窗口框比可视区高出一截。 */
html.lg-tauri-framed .window-frame {
  position: fixed;
  inset: 0;
  height: auto;
}

html.lg-tauri-framed .window-frame .window-body > .serverdesk,
html.lg-tauri-framed .window-frame .window-body > .console-app {
  flex: 1 1 auto;
  height: auto;
  min-height: 0;
}

html.lg-tauri-framed .window-frame .console-main {
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
}

@media (min-width: 981px) {
  html.lg-tauri-framed .window-frame .console-side {
    height: auto;
  }
}

@media (max-width: 980px) {
  html.lg-tauri-framed .window-frame .console-side {
    top: 40px;
    bottom: 0;
    height: auto;
  }
}
</style>
