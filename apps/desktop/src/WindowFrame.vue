<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon, LG_SCOPE_KEY } from '@lightguard/lightui';
import { findServer, flushSettings, state, updateSettings } from './product/store';
import { route } from './product/router';
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
  if (route.value.name === 'desktop') {
    const server = route.value.id ? findServer(route.value.id) : null;
    return server ? `${server.name} · 服务器桌面` : '服务器桌面';
  }
  const titles: Record<string, string> = {
    overview: '服务器总览',
    connections: '连接记录',
    settings: '设置',
  };
  return titles[route.value.name] ?? 'LightGuard';
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
    <header v-if="!isFullscreen" class="window-bar" data-tauri-drag-region>
      <div class="window-bar__brand" data-tauri-drag-region>
        <span class="window-bar__mark" data-tauri-drag-region>
          <LgIcon name="shield" :size="13" />
        </span>
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
  flex: 0 0 38px;
  align-items: center;
  gap: 9px;
  padding-left: 12px;
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

.window-bar__mark {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 22px;
  height: 22px;
  border: var(--lg-border-width) solid color-mix(in srgb, var(--lg-primary) 32%, var(--lg-border));
  border-radius: var(--lg-radius-sm);
  background: linear-gradient(145deg, var(--lg-primary-soft), var(--lg-accent-soft));
  color: var(--lg-primary);
}

.window-bar__brand strong {
  font-size: 11.5px;
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
  font-size: 10.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.window-bar__spacer {
  flex: 1;
  align-self: stretch;
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
  background: transparent;
}

html.lg-tauri-framed .window-frame .window-body > .serverdesk,
html.lg-tauri-framed .window-frame .window-body > .console-app {
  flex: 1 1 auto;
  height: auto;
  min-height: 0;
}

html.lg-tauri-framed .window-frame .console-main {
  min-height: 0;
  overflow-y: auto;
}

@media (min-width: 981px) {
  html.lg-tauri-framed .window-frame .console-side {
    height: auto;
  }
}

@media (max-width: 980px) {
  html.lg-tauri-framed .window-frame .console-side {
    top: 38px;
    height: calc(100dvh - 38px);
  }
}
</style>
