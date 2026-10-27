<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { usePreferredDark, usePreferredReducedMotion } from '@vueuse/core';
import {
  LgButton,
  LgDesktopEffects,
  LgIcon,
  LgStatusPill,
  resolveTheme,
  themeOptions,
} from '@lightguard/lightui';
import type { ThemeId } from '@lightguard/lightui';
import DesktopIconLayer from '../../desktop/DesktopIconLayer.vue';
import DesktopWindow from './DesktopWindow.vue';
import {
  DESKTOP_APPS,
  appMeta,
  desktopIconItems,
  statusLabel,
  statusTone,
  type DesktopAppKey,
} from './apps';
import { useDesktopWindows } from './useDesktopWindows';
import { addLog, retryServer, state } from '../store';
import { navigateTo } from '../router';
import { switchScopeTheme } from '../theme';
import { confirm, toast } from '../ui';
import { renderSettings } from '../../render/settings';
import type { Server } from '../types';
import './apps.css';

const props = defineProps<{ server: Server }>();

const reduced = usePreferredReducedMotion();
const prefersDark = usePreferredDark();

const shell = ref<HTMLElement | null>(null);

function bounds() {
  const rect = shell.value?.getBoundingClientRect();
  return { width: rect?.width ?? window.innerWidth, height: rect?.height ?? window.innerHeight };
}

const {
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
} = useDesktopWindows(bounds);

const selectedIcon = ref(-1);
const startOpen = ref(false);
const trayOpen = ref(false);

const session = `lg_${Math.random().toString(36).slice(2, 12)}`;
const startedAt = (() => {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())} ${pad(now.getHours())}:${pad(now.getMinutes())}`;
})();

const booting = ref(true);
const bootStage = ref(0);
const bootStages = ['解析主机地址', '建立 TCP 通道', 'SSH 密钥交换', '校验主机指纹', '加载系统信息'];

const now = ref(new Date());
const clockTime = computed(() => {
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${pad(now.value.getHours())}:${pad(now.value.getMinutes())}`;
});
const clockDate = computed(() => {
  const weekdays = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];
  return `${now.value.getMonth() + 1}/${now.value.getDate()} ${weekdays[now.value.getDay()]}`;
});

const currentTheme = computed(
  () => themeOptions.find((option) => option.id === state.themes.desktop) ?? themeOptions[0],
);

const themeGlyphs: Record<string, string> = {
  system: 'monitor',
  dark: 'moon',
  light: 'sun',
  claude: 'book',
  pixel: 'grid',
  nes: 'cartridge',
  tech: 'spark',
};

const themeSwatches = computed(() =>
  themeOptions.map((option) => ({
    ...option,
    swatch: resolveTheme(option.id, prefersDark.value).color.primary,
  })),
);

const retrying = ref(false);
const needsReconnect = computed(
  () => props.server.status === 'error' || props.server.status === 'offline' || props.server.status === 'connecting',
);
const connecting = computed(() => props.server.status === 'connecting');

let timers: number[] = [];

onMounted(() => {
  document.addEventListener('keydown', onGlobalKey, true);
  timers.push(window.setInterval(() => (now.value = new Date()), 20000));
  if (reduced.value === 'reduce') {
    booting.value = false;
    open('about');
    return;
  }
  bootStages.forEach((_, index) => {
    timers.push(
      window.setTimeout(() => {
        bootStage.value = index;
      }, 190 + index * 190),
    );
  });
  timers.push(
    window.setTimeout(() => {
      booting.value = false;
      open('about');
    }, 1280),
  );
});

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onGlobalKey, true);
  for (const timer of timers) {
    window.clearTimeout(timer);
    window.clearInterval(timer);
  }
  timers = [];
});

function onGlobalKey(event: KeyboardEvent) {
  if (event.key !== 'Escape') return;
  if (startOpen.value) startOpen.value = false;
  else if (trayOpen.value) trayOpen.value = false;
}

function openApp(key: DesktopAppKey) {
  open(key);
  startOpen.value = false;
  trayOpen.value = false;
}

function onIconSelect(index: number) {
  selectedIcon.value = index;
  const app = DESKTOP_APPS[index];
  if (app) open(app.key);
}

function onIconActivate(index: number) {
  const app = DESKTOP_APPS[index];
  if (app) open(app.key);
}

function onWallpaperClick() {
  selectedIcon.value = -1;
  startOpen.value = false;
  trayOpen.value = false;
}

function pickTheme(id: ThemeId) {
  switchScopeTheme('desktop', id);
}

function toggleStart() {
  trayOpen.value = false;
  startOpen.value = !startOpen.value;
}

function toggleTray() {
  startOpen.value = false;
  trayOpen.value = !trayOpen.value;
}

function onShowDesktop() {
  showDesktop();
  startOpen.value = false;
  trayOpen.value = false;
}

function goConsole() {
  startOpen.value = false;
  trayOpen.value = false;
  addLog({
    serverId: props.server.id,
    action: '会话断开',
    status: 'success',
    duration: '0.00s',
    source: '桌面客户端',
    session,
  });
  navigateTo('overview');
  toast({ title: '已返回控制台', message: `${props.server.name} 的桌面会话已关闭`, tone: 'info', duration: 2400 });
}

async function onDisconnect() {
  const ok = await confirm({
    title: '断开 SSH 会话',
    message: `将结束与 ${props.server.name}（${props.server.ip}:${props.server.port}）的桌面会话，并关闭所有窗口。控制台中的服务器配置不受影响。`,
    confirmLabel: '断开',
    tone: 'danger',
  });
  if (!ok) return;
  addLog({
    serverId: props.server.id,
    action: '会话断开',
    status: 'success',
    duration: '0.00s',
    source: '桌面客户端',
    session,
  });
  navigateTo('overview');
  toast({ title: '会话已断开', message: `${props.server.name} 已退出服务器桌面`, tone: 'success' });
}

async function onRetry() {
  if (retrying.value) return;
  retrying.value = true;
  const updated = await retryServer(props.server.id);
  retrying.value = false;
  if (updated && (updated.status === 'online' || updated.status === 'warning')) {
    toast({ title: '重新连接成功', message: `${updated.name} 已恢复实时数据`, tone: 'success' });
  } else {
    toast({
      title: '连接仍然失败',
      message: updated?.error?.message ?? '请检查网络与凭据后重试。',
      tone: 'error',
    });
  }
}
</script>

<template>
  <div ref="shell" class="serverdesk">
    <div class="desk-wallpaper" aria-hidden="true" @click="onWallpaperClick" />

    <header class="desk-bar">
      <button type="button" class="desk-bar__back" @click="goConsole">
        <LgIcon name="chevron-left" :size="14" />
        控制台
      </button>
      <div class="desk-bar__id">
        <span class="desk-bar__mark"><LgIcon name="shield" :size="16" /></span>
        <div>
          <strong>{{ server.name }}</strong>
          <small>{{ server.user }}@{{ server.ip }}:{{ server.port }}</small>
        </div>
      </div>
      <div class="desk-bar__meta">
        <LgStatusPill :label="statusLabel(server.status)" :tone="statusTone(server.status)" />
        <span class="desk-bar__mono">{{ server.latency }} ms</span>
        <span class="desk-bar__mono desk-bar__session">{{ session }}</span>
        <button type="button" class="desk-bar__power" aria-label="断开连接" @click="onDisconnect">
          <LgIcon name="power" :size="14" />
        </button>
      </div>
    </header>

    <DesktopIconLayer
      class="desk-icons"
      :items="desktopIconItems"
      :selected-index="selectedIcon"
      aria-label="服务器桌面图标"
      @select="onIconSelect"
      @activate="onIconActivate"
    />

    <div v-if="windows.length === 0" class="desk-hint">
      <LgIcon name="grid" :size="18" />
      <span>单击或双击桌面图标打开应用，也可以从「开始」菜单启动。</span>
    </div>

    <DesktopWindow
      v-for="win in windows"
      :key="win.app"
      :win="win"
      :active="active === win.app"
      :server="server"
      :session="session"
      :started-at="startedAt"
      @focus="focus(win.app)"
      @minimize="minimize(win.app)"
      @maximize="toggleMaximize(win.app)"
      @close="close(win.app)"
      @move="(x, y) => move(win.app, x, y)"
      @disconnect="onDisconnect"
    />

    <div v-if="startOpen" class="desk-scrim" @click="startOpen = false" />
    <section v-if="startOpen" class="desk-start" role="dialog" aria-label="开始菜单">
      <header class="desk-start__head">
        <span class="desk-start__avatar"><LgIcon name="server" :size="20" /></span>
        <div>
          <strong>{{ server.name }}</strong>
          <small>{{ server.role }} · {{ server.os }}</small>
        </div>
        <LgStatusPill :label="statusLabel(server.status)" :tone="statusTone(server.status)" />
      </header>

      <div class="desk-start__section">
        <h3>应用</h3>
        <div class="desk-start__apps">
          <button
            v-for="app in DESKTOP_APPS"
            :key="app.key"
            type="button"
            class="desk-start__app"
            @click="openApp(app.key)"
          >
            <span class="desk-start__tile" :class="app.tile"><LgIcon :name="app.icon" :size="16" /></span>
            <span>
              <strong>{{ app.title }}</strong>
              <small>{{ app.description }}</small>
            </span>
          </button>
        </div>
      </div>

      <div class="desk-start__section">
        <h3>桌面主题</h3>
        <div class="desk-start__themes">
          <button
            v-for="option in themeSwatches"
            :key="option.id"
            type="button"
            class="desk-theme"
            :class="{ 'is-active': state.themes.desktop === option.id }"
            :aria-pressed="state.themes.desktop === option.id"
            :title="option.description"
            @click="pickTheme(option.id)"
          >
            <span class="desk-theme__dot" :style="{ background: option.swatch }" />
            {{ option.label }}
          </button>
        </div>
      </div>

      <footer class="desk-start__foot">
        <button type="button" @click="goConsole">
          <LgIcon name="chevron-left" :size="13" />
          返回控制台
        </button>
        <button type="button" class="is-danger" @click="onDisconnect">
          <LgIcon name="power" :size="13" />
          断开连接
        </button>
        <span>本地演示 · 不发起真实网络请求</span>
      </footer>
    </section>

    <div v-if="trayOpen" class="desk-scrim" @click="trayOpen = false" />
    <section v-if="trayOpen" class="desk-tray" role="dialog" aria-label="系统托盘">
      <header class="desk-tray__head">
        <span class="desk-tray__mark"><LgIcon name="shield" :size="15" /></span>
        <div>
          <strong>LightGuard</strong>
          <small>本地演示 · 会话加密</small>
        </div>
      </header>
      <div class="desk-tray__row"><span>会话</span><strong class="desk-bar__mono">{{ session }}</strong></div>
      <div class="desk-tray__row"><span>主题</span><strong>{{ currentTheme.label }}</strong></div>
      <div class="desk-tray__swatches" role="group" aria-label="快速切换桌面主题">
        <button
          v-for="option in themeSwatches"
          :key="option.id"
          type="button"
          class="desk-tray__swatch"
          :class="{ 'is-active': state.themes.desktop === option.id }"
          :style="{ background: option.swatch }"
          :aria-label="option.label"
          :title="option.label"
          @click="pickTheme(option.id)"
        />
      </div>
      <div class="desk-tray__actions">
        <button type="button" @click="goConsole">返回控制台</button>
        <button type="button" class="is-danger" @click="onDisconnect">断开</button>
      </div>
    </section>

    <footer class="desk-taskbar">
      <button
        type="button"
        class="desk-taskbar__start"
        :class="{ 'is-open': startOpen }"
        :aria-expanded="startOpen"
        @click="toggleStart"
      >
        <LgIcon name="grid" :size="15" />
        开始
      </button>

      <div class="desk-taskbar__apps">
        <button
          v-for="win in windows"
          :key="win.app"
          type="button"
          class="desk-taskbar__app"
          :class="{ 'is-active': active === win.app && !win.minimized, 'is-min': win.minimized }"
          :aria-label="`${appMeta(win.app).title}${win.minimized ? '（已最小化）' : ''}`"
          @click="toggleTaskbar(win.app)"
        >
          <span class="desk-taskbar__tile" :class="appMeta(win.app).tile">
            <LgIcon :name="appMeta(win.app).icon" :size="12" />
          </span>
          <span class="desk-taskbar__label">{{ appMeta(win.app).title }}</span>
        </button>
        <span v-if="windows.length === 0" class="desk-taskbar__empty">双击桌面图标打开应用</span>
      </div>

      <div class="desk-taskbar__right">
        <button type="button" class="desk-taskbar__icon" aria-label="显示桌面" title="显示桌面" @click="onShowDesktop">
          <LgIcon name="monitor" :size="14" />
        </button>
        <button
          type="button"
          class="desk-taskbar__icon"
          :class="{ 'is-open': trayOpen }"
          aria-label="系统托盘"
          title="系统托盘"
          @click="toggleTray"
        >
          <LgIcon :name="themeGlyphs[state.themes.desktop] ?? 'moon'" :size="14" />
        </button>
        <div class="desk-clock">
          <strong>{{ clockTime }}</strong>
          <span>{{ clockDate }}</span>
        </div>
      </div>
    </footer>

    <div v-if="!booting && needsReconnect" class="desk-offline" role="alertdialog" aria-label="连接状态">
      <div class="desk-offline__card">
        <span class="desk-offline__icon" :class="{ 'is-spinning': connecting }">
          <LgIcon :name="connecting ? 'refresh' : 'alert'" :size="22" />
        </span>
        <h2>{{ connecting ? '正在重新建立会话…' : '会话已中断' }}</h2>
        <p v-if="connecting">正在与 {{ server.ip }}:{{ server.port }} 重新协商 SSH 通道。</p>
        <template v-else>
          <p>{{ server.error?.message ?? '无法连接目标主机' }}</p>
          <p class="desk-offline__hint">{{ server.error?.hint ?? '请检查网络与凭据后重试。' }}</p>
        </template>
        <div class="desk-offline__actions">
          <LgButton size="small" :disabled="connecting || retrying" :loading="retrying" @click="onRetry">
            重新连接
          </LgButton>
          <LgButton size="small" secondary @click="goConsole">返回控制台</LgButton>
        </div>
      </div>
    </div>

    <div v-if="booting" class="desk-boot" role="status" aria-live="polite">
      <div class="desk-boot__card">
        <span class="desk-boot__mark"><LgIcon name="shield" :size="26" /></span>
        <h1>LightGuard</h1>
        <p>{{ server.name }} · {{ server.ip }}:{{ server.port }}</p>
        <ul class="desk-boot__steps">
          <li
            v-for="(stage, index) in bootStages"
            :key="stage"
            :class="{ 'is-done': index < bootStage, 'is-active': index === bootStage }"
          >
            <span class="desk-boot__dot" aria-hidden="true" />
            {{ stage }}
          </li>
        </ul>
        <div class="desk-boot__progress">
          <i :style="{ width: `${((bootStage + 1) / bootStages.length) * 100}%` }" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.serverdesk {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100dvh;
  overflow: hidden;
  background-color: var(--lg-bg);
  background-image: var(--lg-desktop-wallpaper);
  background-size: cover;
  background-position: center;
  color: var(--lg-desktop-text, var(--lg-text));
}

.desk-wallpaper {
  position: absolute;
  inset: 0;
  z-index: 0;
  background-image: var(--lg-desktop-vignette);
  background-size: cover;
}

.desk-bar {
  position: relative;
  z-index: 220;
  display: flex;
  align-items: center;
  gap: 10px;
  height: 42px;
  padding: 0 12px;
  border-bottom: 1px solid var(--lg-border);
  background: color-mix(in srgb, var(--lg-surface) 76%, transparent);
  backdrop-filter: blur(14px);
}

.desk-bar__back {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 4px 8px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: 10.5px;
  cursor: pointer;
}

.desk-bar__back:hover {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.desk-bar__id {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.desk-bar__mark {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.desk-bar__id div {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.desk-bar__id strong {
  overflow: hidden;
  font-size: 11.5px;
  font-weight: 650;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-bar__id small {
  overflow: hidden;
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-family: var(--lg-font-mono);
  font-size: 9.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-bar__meta {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-left: auto;
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: 10px;
}

.desk-bar__mono {
  font-family: var(--lg-font-mono);
}

.desk-bar__session {
  max-width: 150px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-bar__power {
  display: grid;
  place-items: center;
  width: 26px;
  height: 24px;
  padding: 0;
  border: 1px solid var(--lg-border);
  border-radius: 6px;
  background: var(--lg-surface);
  color: var(--lg-text-muted);
  cursor: pointer;
}

.desk-bar__power:hover {
  border-color: var(--lg-danger);
  color: var(--lg-danger);
}

.desk-icons {
  position: absolute;
  top: 56px;
  left: 14px;
  z-index: 3;
}

.desk-hint {
  position: absolute;
  right: 16px;
  bottom: 62px;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 8px 11px;
  border: 1px dashed color-mix(in srgb, var(--lg-desktop-text) 24%, transparent);
  border-radius: var(--lg-radius-sm);
  background: color-mix(in srgb, var(--lg-surface) 58%, transparent);
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: 10.5px;
  pointer-events: none;
}

.desk-scrim {
  position: absolute;
  inset: 0;
  z-index: 240;
}

.desk-start {
  position: absolute;
  bottom: 54px;
  left: 10px;
  z-index: 250;
  display: flex;
  flex-direction: column;
  width: min(560px, calc(100vw - 20px));
  max-height: calc(100dvh - 74px);
  padding: 12px;
  overflow: auto;
  border: 1px solid var(--lg-border-strong);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-lg);
}

.desk-start__head {
  display: flex;
  align-items: center;
  gap: 9px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--lg-border);
}

.desk-start__head > div {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.desk-start__head strong {
  font-size: 12.5px;
}

.desk-start__head small {
  overflow: hidden;
  color: var(--lg-text-muted);
  font-size: 10px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-start__avatar {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.desk-start__section {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding-top: 10px;
}

.desk-start__section h3 {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.desk-start__apps {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 5px;
}

.desk-start__app {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 8px;
  border: 1px solid transparent;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text);
  text-align: left;
  cursor: pointer;
}

.desk-start__app:hover,
.desk-start__app:focus-visible {
  border-color: var(--lg-border);
  background: var(--lg-hover);
  outline: none;
}

.desk-start__app > span:last-child {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.desk-start__app strong {
  font-size: 11px;
  font-weight: 600;
}

.desk-start__app small {
  overflow: hidden;
  color: var(--lg-text-muted);
  font-size: 9.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-start__tile {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
}

.desk-start__themes {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
}

.desk-theme {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 8px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-bg-soft);
  color: var(--lg-text-muted);
  font-size: 10.5px;
  cursor: pointer;
}

.desk-theme:hover {
  color: var(--lg-text);
}

.desk-theme.is-active {
  border-color: var(--lg-primary);
  background: var(--lg-primary-soft);
  color: var(--lg-text);
  font-weight: 600;
}

.desk-theme__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}

.desk-start__foot {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 12px;
  padding-top: 9px;
  border-top: 1px solid var(--lg-border);
}

.desk-start__foot button {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 8px;
  border: 1px solid var(--lg-border);
  border-radius: 6px;
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: 10px;
  cursor: pointer;
}

.desk-start__foot button:hover {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.desk-start__foot button.is-danger:hover {
  border-color: var(--lg-danger);
  color: var(--lg-danger);
}

.desk-start__foot span {
  margin-left: auto;
  color: var(--lg-text-muted);
  font-size: 9.5px;
}

.desk-tray {
  position: absolute;
  right: 10px;
  bottom: 54px;
  z-index: 250;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 250px;
  padding: 10px;
  border: 1px solid var(--lg-border-strong);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-lg);
}

.desk-tray__head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--lg-border);
}

.desk-tray__head div {
  display: flex;
  flex-direction: column;
}

.desk-tray__head strong {
  font-size: 11.5px;
}

.desk-tray__head small {
  color: var(--lg-text-muted);
  font-size: 9.5px;
}

.desk-tray__mark {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: 8px;
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.desk-tray__row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  color: var(--lg-text-muted);
  font-size: 10px;
}

.desk-tray__row strong {
  overflow: hidden;
  color: var(--lg-text-secondary);
  font-size: 10px;
  font-weight: 500;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-tray__swatches {
  display: flex;
  gap: 5px;
}

.desk-tray__swatch {
  width: 18px;
  height: 18px;
  padding: 0;
  border: 2px solid transparent;
  border-radius: 50%;
  cursor: pointer;
}

.desk-tray__swatch.is-active {
  border-color: var(--lg-text);
  box-shadow: 0 0 0 2px var(--lg-surface);
}

.desk-tray__actions {
  display: flex;
  gap: 5px;
  padding-top: 6px;
  border-top: 1px solid var(--lg-border);
}

.desk-tray__actions button {
  flex: 1;
  padding: 4px 6px;
  border: 1px solid var(--lg-border);
  border-radius: 6px;
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: 10px;
  cursor: pointer;
}

.desk-tray__actions button:hover {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.desk-tray__actions button.is-danger:hover {
  border-color: var(--lg-danger);
  color: var(--lg-danger);
}

.desk-taskbar {
  position: relative;
  z-index: 260;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 46px;
  margin-top: auto;
  padding: 0 8px;
  border-top: 1px solid var(--lg-border);
  background: color-mix(in srgb, var(--lg-surface) 82%, transparent);
  backdrop-filter: blur(16px);
}

.desk-taskbar__start {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 5px;
  height: 32px;
  padding: 0 11px;
  border: 1px solid transparent;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: 11px;
  cursor: pointer;
}

.desk-taskbar__start:hover,
.desk-taskbar__start.is-open {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-primary);
}

.desk-taskbar__apps {
  display: flex;
  flex: 1;
  gap: 3px;
  min-width: 0;
  overflow: hidden;
}

.desk-taskbar__app {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 170px;
  height: 32px;
  padding: 0 9px;
  border: 1px solid transparent;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-muted);
  font-size: 10.5px;
  cursor: pointer;
}

.desk-taskbar__app:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.desk-taskbar__app.is-active {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-text);
  box-shadow: inset 0 -2px 0 var(--lg-primary);
}

.desk-taskbar__app.is-min {
  opacity: 0.6;
}

.desk-taskbar__tile {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 20px;
  height: 20px;
  border-radius: 5px;
}

.desk-taskbar__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.desk-taskbar__empty {
  align-self: center;
  padding-left: 6px;
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: 10px;
}

.desk-taskbar__right {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 3px;
}

.desk-taskbar__icon {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.desk-taskbar__icon:hover,
.desk-taskbar__icon.is-open {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-primary);
}

.desk-clock {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  padding: 0 4px 0 8px;
  color: var(--lg-text-secondary);
  font-family: var(--lg-font-mono);
  line-height: 1.2;
}

.desk-clock strong {
  font-size: 11px;
  font-weight: 600;
}

.desk-clock span {
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: 9px;
}

.desk-offline {
  position: absolute;
  inset: 0;
  z-index: 280;
  display: grid;
  place-items: center;
  padding: 20px;
  background: color-mix(in srgb, var(--lg-bg) 62%, transparent);
  backdrop-filter: blur(6px);
}

.desk-offline__card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  max-width: 380px;
  padding: 22px;
  border: 1px solid var(--lg-border-strong);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-lg);
  text-align: center;
}

.desk-offline__icon {
  display: grid;
  place-items: center;
  width: 42px;
  height: 42px;
  border-radius: 12px;
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.desk-offline__icon.is-spinning :deep(svg) {
  animation: desk-spin 1.1s linear infinite;
}

.desk-offline h2 {
  margin: 2px 0 0;
  font-size: 14px;
}

.desk-offline p {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 11px;
  line-height: 1.6;
}

.desk-offline__hint {
  color: var(--lg-text-secondary) !important;
}

.desk-offline__actions {
  display: flex;
  gap: 7px;
  margin-top: 6px;
}

.desk-boot {
  position: absolute;
  inset: 0;
  z-index: 400;
  display: grid;
  place-items: center;
  padding: 20px;
  background: color-mix(in srgb, var(--lg-bg) 88%, transparent);
  backdrop-filter: blur(10px);
}

.desk-boot__card {
  display: flex;
  flex-direction: column;
  align-items: center;
  width: min(360px, 100%);
  padding: 26px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-lg);
  text-align: center;
}

.desk-boot__mark {
  display: grid;
  place-items: center;
  width: 52px;
  height: 52px;
  border-radius: 15px;
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
  animation: desk-pulse 1.6s var(--lg-ease) infinite;
}

.desk-boot h1 {
  margin: 12px 0 0;
  font-size: 17px;
  letter-spacing: -0.01em;
}

.desk-boot p {
  margin: 3px 0 0;
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 10px;
}

.desk-boot__steps {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
  margin: 16px 0 0;
  padding: 0;
  list-style: none;
  text-align: left;
}

.desk-boot__steps li {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.desk-boot__steps li.is-active {
  color: var(--lg-primary);
  font-weight: 600;
}

.desk-boot__steps li.is-done {
  color: var(--lg-text-secondary);
}

.desk-boot__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--lg-border-strong);
}

.desk-boot__steps li.is-active .desk-boot__dot {
  background: var(--lg-primary);
  box-shadow: 0 0 0 3px var(--lg-primary-soft);
}

.desk-boot__steps li.is-done .desk-boot__dot {
  background: var(--lg-success);
}

.desk-boot__progress {
  width: 100%;
  height: 3px;
  margin-top: 14px;
  overflow: hidden;
  border-radius: var(--lg-radius-full);
  background: var(--lg-bg-soft);
}

.desk-boot__progress i {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--lg-primary);
  transition: width 180ms var(--lg-ease);
}

@keyframes desk-pulse {
  0%,
  100% {
    transform: scale(1);
  }
  50% {
    transform: scale(1.07);
  }
}

@keyframes desk-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 900px) {
  .desk-bar__session {
    display: none;
  }
}

@media (max-width: 719px) {
  .desk-bar {
    height: 38px;
    padding: 0 8px;
  }

  .desk-bar__id small,
  .desk-bar__meta .lg-pill,
  .desk-bar__mono {
    display: none;
  }

  .desk-icons {
    top: 48px;
    left: 6px;
    transform: scale(0.92);
    transform-origin: top left;
  }

  .desk-taskbar__label {
    display: none;
  }

  .desk-taskbar__app {
    padding: 0 7px;
  }

  .desk-taskbar__empty {
    display: none;
  }

  .desk-hint {
    display: none;
  }

  .desk-start {
    left: 6px;
    width: calc(100vw - 12px);
  }

  .desk-start__apps {
    grid-template-columns: minmax(0, 1fr);
  }

  .desk-start__foot span {
    display: none;
  }

  .desk-tray {
    right: 6px;
    width: min(250px, calc(100vw - 12px));
  }
}
</style>
