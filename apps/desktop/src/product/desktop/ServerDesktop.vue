<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useElementSize, usePreferredDark, usePreferredReducedMotion } from '@vueuse/core';
import {
  LgButton,
  LgDesktopEffects,
  LgIcon,
  LgInput,
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
  statusLabel,
  statusTone,
  type DesktopAppKey,
} from './apps';
import { useDesktopWindows } from './useDesktopWindows';
import PermitControl from '../guard/PermitControl.vue';
import {
  closePermitPanel,
  lockPermit,
  openPermitPanel,
  permitOf,
  refreshPermit,
  subscribeGuard,
  unlockPermit,
} from '../guard/store';
import { permitVisual } from '../guard/view';
import { addLog, backendAvailable, disconnectServer, retryServer, state } from '../store';
import { navigateTo } from '../router';
import { switchScopeTheme } from '../theme';
import { confirm, toast } from '../ui';
import { renderSettings } from '../../render/settings';
import { isFullscreen, toggleFullscreen } from '../../windowState';
import type { Server } from '../types';
import './apps.css';

const props = defineProps<{ server: Server }>();

const reduced = usePreferredReducedMotion();
const prefersDark = usePreferredDark();

const shell = ref<HTMLElement | null>(null);
const { width: shellWidth, height: shellHeight } = useElementSize(shell);

const FULL_BAR_HEIGHT = 46;
const fullBar = computed(() => state.settings.desktopBar === 'full');
const barInset = computed(() => (fullBar.value ? FULL_BAR_HEIGHT : 0));
// 通栏时 Dock 占布局，窗口要让出 54px；悬浮时 Dock 覆盖在窗口之上，只留 6px 边距。
const DOCK_FULL_HEIGHT = 54;
const DOCK_FLOATING_INSET = 6;
const dockFloating = computed(() => state.settings.desktopDock === 'floating');
const dockInset = computed(() => (dockFloating.value ? DOCK_FLOATING_INSET : DOCK_FULL_HEIGHT));
const iconRows = computed(() => {
  const height = shellHeight.value || (typeof window === 'undefined' ? 900 : window.innerHeight);
  return Math.max(1, Math.floor((height - 138) / 92));
});

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
  resize,
  fitToBounds,
} = useDesktopWindows(
  bounds,
  () => barInset.value,
  () => dockInset.value,
);

watch([shellWidth, shellHeight, barInset, dockInset], () => fitToBounds());

const selectedIcon = ref(-1);
const startOpen = ref(false);
const trayOpen = ref(false);
const startQuery = ref('');

const visibleApps = computed(() =>
  DESKTOP_APPS.filter((app) => app.key !== 'files' || backendAvailable),
);
const iconItems = computed(() =>
  visibleApps.value.map((app) => ({ name: app.icon, label: app.title, tile: app.tile })),
);

const startApps = computed(() => {
  const keyword = startQuery.value.trim().toLowerCase();
  if (!keyword) return visibleApps.value;
  return visibleApps.value.filter((app) =>
    `${app.title} ${app.description}`.toLowerCase().includes(keyword),
  );
});

const session = computed(() => props.server.sessionId || '未连接');
const startedAt = computed(() => {
  const date = new Date(props.server.startedAt);
  if (!props.server.startedAt || !Number.isFinite(date.getTime())) return '—';
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
});

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

const permit = computed(() => permitOf(props.server.id));
const permitNow = ref(Date.now());
// 上锁播报要区分来源：倒计时/到期算“自动”，用户点“立即上锁”不能也说自动。
const lockNotice = ref<'' | 'auto' | 'manual'>('');
const permitState = computed(() => permitVisual(permit.value, permitNow.value));
let refreshingPermit = false;
let stopGuard: (() => void) | null = null;
let disposed = false;
let lockNoticeTimer: number | null = null;
// 手动上锁在 locked 翻转前立旗，锁成功由 watcher 读取并复位，失败则由请求方撤销。
let manualLockPending = false;
// 许可倒计时完全在本地走：锁定或到期后立刻停表，不再每秒唤醒后端。
let permitTimer: number | null = null;

function stopPermitTimer() {
  if (permitTimer === null) return;
  window.clearInterval(permitTimer);
  permitTimer = null;
}

function startPermitTimer() {
  if (permitTimer !== null) return;
  permitTimer = window.setInterval(() => {
    permitNow.value = Date.now();
    if (permit.value.expiresAtMs > permitNow.value) return;
    stopPermitTimer();
    void refreshPermitOnce();
  }, 1000);
}

function syncPermitTimer() {
  // 用真实时钟判断到期：permitNow 靠每秒 tick 推进，延长许可后它可能还是旧值。
  const current = Date.now();
  if (permit.value.locked || permit.value.expiresAtMs <= current) {
    stopPermitTimer();
    return;
  }
  permitNow.value = current;
  startPermitTimer();
}

function announceLock(kind: 'auto' | 'manual') {
  lockNotice.value = kind;
  if (lockNoticeTimer !== null) window.clearTimeout(lockNoticeTimer);
  lockNoticeTimer = window.setTimeout(() => {
    lockNotice.value = '';
    lockNoticeTimer = null;
  }, 4000);
}

async function refreshPermitOnce() {
  if (refreshingPermit) return;
  refreshingPermit = true;
  try {
    await refreshPermit(props.server.id);
  } finally {
    refreshingPermit = false;
    // 卸载后返回的许可请求不能再拉起计时器。
    if (disposed) return;
    permitNow.value = Date.now();
    syncPermitTimer();
  }
}

async function onUnlockPermit(minutes: number) {
  if (await unlockPermit(props.server.id, minutes)) {
    // 到期停表后再延长时 locked 没变，只有这里和到期时间 watcher 能重新拉起计时器。
    permitNow.value = Date.now();
    syncPermitTimer();
    closePermitPanel();
    toast({
      title: '写操作已解锁',
      message: `${props.server.name} 允许写操作 ${minutes} 分钟`,
      tone: 'success',
      duration: 2400,
    });
  }
}

async function onLockPermit() {
  manualLockPending = true;
  await lockPermit(props.server.id);
  // 失败时 locked 仍为 false：与解锁一致，保留面板让错误段落可见，也不算一次上锁播报。
  if (!permit.value.locked) {
    manualLockPending = false;
    return;
  }
  closePermitPanel();
}

watch(
  () => permit.value.locked,
  (locked, previous) => {
    if (!locked) {
      lockNotice.value = '';
      manualLockPending = false;
      permitNow.value = Date.now();
      syncPermitTimer();
      return;
    }
    stopPermitTimer();
    if (previous === false) {
      announceLock(manualLockPending ? 'manual' : 'auto');
      manualLockPending = false;
    }
  },
);

// 到期时间变化（延长、整条刷新、后端事件）都要重算计时器：
// 零秒刷新失败后许可可能停在“未锁定但已过期”，此时延长不改变 locked，
// 只挂在 locked 上的 watcher 不会触发，倒计时/任务栏/危险色就会一直冻在 00:00。
watch(
  () => permit.value.expiresAtMs,
  () => {
    if (disposed) return;
    syncPermitTimer();
  },
);

onMounted(() => {
  void refreshPermitOnce();
  stopGuard = subscribeGuard();
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
  disposed = true;
  stopGuard?.();
  stopGuard = null;
  stopPermitTimer();
  if (lockNoticeTimer !== null) window.clearTimeout(lockNoticeTimer);
  lockNoticeTimer = null;
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
  startQuery.value = '';
  trayOpen.value = false;
}

function isOpen(key: DesktopAppKey) {
  return windows.value.some((win) => win.app === key);
}

function isMinimized(key: DesktopAppKey) {
  return windows.value.find((win) => win.app === key)?.minimized ?? false;
}

function openSearch() {
  startOpen.value = true;
  trayOpen.value = false;
}

function onIconSelect(index: number) {
  selectedIcon.value = index;
}

function onIconActivate(index: number) {
  const app = visibleApps.value[index];
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
  if (!startOpen.value) startQuery.value = '';
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
  navigateTo('overview');
  toast({ title: '已返回控制台', message: `${props.server.name} 的会话保持连接，后台继续只读采样`, tone: 'info', duration: 2400 });
}

async function onDisconnect() {
  const ok = await confirm({
    title: '断开 SSH 会话',
    message: `将结束与 ${props.server.name}（${props.server.ip}:${props.server.port}）的桌面会话，并关闭所有窗口。控制台中的服务器配置不受影响。`,
    confirmLabel: '断开',
    tone: 'danger',
  });
  if (!ok) return;
  await disconnectServer(props.server.id);
  addLog({
    serverId: props.server.id,
    action: '会话断开',
    status: 'success',
    duration: '0.00s',
    source: '桌面客户端',
    session: session.value,
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
  <div
    ref="shell"
    class="serverdesk"
    :class="{
      'is-bar-full': fullBar,
      'is-dock-floating': dockFloating,
      'is-permit-unlocked': !permit.locked,
    }"
    :style="{ '--desk-bar-inset': `${barInset}px`, '--desk-dock-bottom': `${dockInset}px` }"
  >
    <div class="desk-wallpaper" aria-hidden="true" @click="onWallpaperClick" />

    <header class="conn-bar" aria-label="SSH 会话状态">
      <div class="conn-bar__identity">
        <button type="button" class="conn-bar__back" aria-label="返回控制台" @click="goConsole">
          <LgIcon name="chevron-left" :size="13" />
        </button>
        <span class="conn-bar__dot" :data-status="server.status" aria-hidden="true" />
        <span class="conn-bar__mode">SSH</span>
        <strong class="conn-bar__name">{{ server.name }}</strong>
        <span class="conn-bar__addr">{{ server.user }}@{{ server.ip }}:{{ server.port }}</span>
      </div>
      <div class="conn-bar__stats">
        <span class="conn-bar__meta">
          <LgIcon name="wifi" :size="11" />
          {{ server.latency }} ms
        </span>
        <span v-if="server.encryption" class="conn-bar__meta">
          <LgIcon name="lock" :size="11" />
          已加密
        </span>
        <span class="conn-bar__meta conn-bar__uptime">
          <LgIcon name="clock" :size="11" />
          {{ server.uptime }}
        </span>
      </div>
      <PermitControl
        class="conn-bar__permit"
        :server-id="server.id"
        :status="permit"
        :default-minutes="state.settings.permit.defaultMinutes"
        :write-active="permit.writeActive"
        :now-ms="permitNow"
        :lock-notice="lockNotice"
        @unlock="onUnlockPermit"
        @lock="onLockPermit"
      />
      <button type="button" class="conn-bar__power" @click="onDisconnect">断开</button>
    </header>

    <LgIcon name="shield" :size="300" class="desk-watermark" aria-hidden="true" />
    <span class="desk-signature" aria-hidden="true">LightGuard Server Desktop</span>

    <DesktopIconLayer
      class="desk-icons"
      :items="iconItems"
      :selected-index="selectedIcon"
      :rows="iconRows"
      aria-label="服务器桌面图标"
      @select="onIconSelect"
      @activate="onIconActivate"
    />

    <div class="desk-hint">
      <LgIcon name="info" :size="14" />
      <span>双击图标打开 · 拖动标题栏移动窗口</span>
    </div>

    <DesktopWindow
      v-for="win in windows"
      :key="win.app"
      :win="win"
      :active="active === win.app"
      :server="server"
      :session="session"
      :started-at="startedAt"
      :top-inset="barInset"
      :bottom-inset="dockInset"
      @focus="focus(win.app)"
      @minimize="minimize(win.app)"
      @maximize="toggleMaximize(win.app)"
      @close="close(win.app)"
      @move="(x, y) => move(win.app, x, y)"
      @resize="(x, y, w, h) => resize(win.app, x, y, w, h)"
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

      <div class="desk-start__search">
        <LgInput v-model:value="startQuery" size="small" placeholder="搜索应用" clearable>
          <template #prefix><LgIcon name="search" :size="13" /></template>
        </LgInput>
      </div>

      <div class="desk-start__section">
        <h3>应用</h3>
        <div v-if="startApps.length" class="desk-start__apps">
          <button
            v-for="app in startApps"
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
        <p v-else class="desk-start__empty">没有匹配的应用</p>
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
        <span>{{ server.encryption ? `会话加密 · ${server.encryption.cipher}` : 'SSH 直连 · 只读采集' }}</span>
      </footer>
    </section>

    <div v-if="trayOpen" class="desk-scrim" @click="trayOpen = false" />
    <section v-if="trayOpen" class="desk-tray" role="dialog" aria-label="系统托盘">
      <header class="desk-tray__head">
        <span class="desk-tray__mark"><LgIcon name="shield" :size="15" /></span>
        <div>
          <strong>LightGuard</strong>
          <small>{{ server.encryption ? 'SSH 直连 · 会话加密' : 'SSH 直连 · 只读采集' }}</small>
        </div>
      </header>
      <div class="desk-tray__row"><span>会话</span><strong class="desk-tray__mono">{{ session }}</strong></div>
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
      <div class="desk-taskbar__left">
        <button
          type="button"
          class="desk-taskbar__start"
          :class="{ 'is-open': startOpen }"
          :aria-expanded="startOpen"
          aria-label="开始菜单"
          @click="toggleStart"
        >
          <LgIcon name="grid" :size="19" />
        </button>
        <button type="button" class="desk-taskbar__search" @click="openSearch">
          <LgIcon name="search" :size="13" />
          搜索应用
        </button>
      </div>

      <div class="desk-taskbar__center" aria-label="固定应用">
        <button
          v-for="app in visibleApps"
          :key="app.key"
          type="button"
          class="desk-taskbar__app"
          :class="{
            'is-open': isOpen(app.key),
            'is-active': active === app.key && !isMinimized(app.key),
            'is-min': isMinimized(app.key),
          }"
          :aria-label="`${app.title}${isOpen(app.key) ? (isMinimized(app.key) ? '（已最小化）' : '（运行中）') : ''}`"
          :title="app.title"
          @click="toggleTaskbar(app.key)"
        >
          <span class="desk-taskbar__tile" :class="app.tile"><LgIcon :name="app.icon" :size="15" /></span>
        </button>
      </div>

      <div class="desk-taskbar__right">
        <button
          v-if="isFullscreen"
          type="button"
          class="desk-taskbar__icon"
          aria-label="退出全屏"
          title="退出全屏 (F11)"
          @click="toggleFullscreen"
        >
          <LgIcon name="monitor" :size="14" />
        </button>
        <button type="button" class="desk-taskbar__icon" aria-label="显示桌面" title="显示桌面" @click="onShowDesktop">
          <LgIcon name="monitor" :size="14" />
        </button>
        <!--
          许可入口必须排在窄屏要隐藏的显示器图标之后：窄屏规则靠 :first-child 选中
          那个图标，插到最前面会让它被顶到第二位而不是消失。
        -->
        <button
          v-if="!permit.locked"
          type="button"
          class="desk-taskbar__icon desk-taskbar__permit-btn"
          :class="{ 'is-danger': permitState.state === 'expiring' }"
          :aria-label="`写操作已解锁，剩余 ${permitState.remaining}`"
          :title="`写操作已解锁 · ${permitState.remaining}`"
          @click="openPermitPanel"
        >
          <LgIcon name="unlock" :size="14" />
          <span class="desk-taskbar__permit">{{ permitState.remaining }}</span>
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
  --desk-bar-space: 46px;
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

.conn-bar {
  position: absolute;
  top: 12px;
  left: 50%;
  z-index: 220;
  display: flex;
  align-items: center;
  gap: 7px;
  height: 34px;
  max-width: calc(100% - 24px);
  padding: 0 6px;
  transform: translateX(-50%);
  border: var(--lg-border-width) solid var(--lg-border-strong);
  border-radius: var(--lg-radius-full);
  background: color-mix(in srgb, var(--lg-surface) 78%, transparent);
  box-shadow: var(--lg-shadow-sm);
  backdrop-filter: blur(var(--lg-panel-blur));
  color: var(--lg-text-secondary);
  font-size: 10.5px;
  white-space: nowrap;
}

.conn-bar__identity,
.conn-bar__stats {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
}

.conn-bar__back {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 0;
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.conn-bar__back:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.conn-bar__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--lg-success);
  box-shadow: 0 0 0 3px var(--lg-success-soft);
}

.conn-bar__dot[data-status='warning'] {
  background: var(--lg-warning);
  box-shadow: 0 0 0 3px var(--lg-warning-soft);
}

.conn-bar__dot[data-status='error'] {
  background: var(--lg-danger);
  box-shadow: 0 0 0 3px var(--lg-danger-soft);
}

.conn-bar__dot[data-status='offline'] {
  background: var(--lg-text-muted);
  box-shadow: 0 0 0 3px var(--lg-hover);
}

.conn-bar__dot[data-status='connecting'] {
  background: var(--lg-info);
  box-shadow: 0 0 0 3px var(--lg-info-soft);
}

.conn-bar__mode {
  padding: 1px 6px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 8.5px;
  letter-spacing: 0.08em;
}

.conn-bar__name {
  overflow: hidden;
  max-width: 180px;
  color: var(--lg-text);
  font-size: 11.5px;
  font-weight: 650;
  text-overflow: ellipsis;
}

.conn-bar__addr {
  overflow: hidden;
  max-width: 190px;
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 9.5px;
  text-overflow: ellipsis;
}

.conn-bar__meta {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 9.5px;
}

.conn-bar__power {
  height: 24px;
  padding: 0 9px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface);
  color: var(--lg-text-muted);
  font-size: 10px;
  cursor: pointer;
}

.conn-bar__power:hover {
  border-color: var(--lg-danger);
  color: var(--lg-danger);
}

.serverdesk.is-bar-full .conn-bar {
  top: 0;
  right: 0;
  left: 0;
  gap: 16px;
  height: var(--desk-bar-space);
  max-width: none;
  padding: 0 18px;
  transform: none;
  border-width: 0 0 var(--lg-border-width);
  border-radius: 0;
  background: color-mix(in srgb, var(--lg-surface) 88%, transparent);
}

.serverdesk.is-bar-full .conn-bar__identity {
  gap: 10px;
}

.serverdesk.is-bar-full .conn-bar__back {
  width: 28px;
  height: 28px;
}

.serverdesk.is-bar-full .conn-bar__mode {
  padding: 2px 7px;
  font-size: 9px;
}

.serverdesk.is-bar-full .conn-bar__name {
  min-width: 0;
  max-width: 220px;
  font-size: 12px;
}

.serverdesk.is-bar-full .conn-bar__addr {
  min-width: 0;
  max-width: 240px;
  padding-left: 10px;
  border-left: var(--lg-border-width) solid var(--lg-border);
  font-size: 10px;
}

.serverdesk.is-bar-full .conn-bar__stats {
  gap: 0;
  margin-left: auto;
}

.serverdesk.is-bar-full .conn-bar__stats > * + * {
  margin-left: 16px;
  padding-left: 16px;
  border-left: var(--lg-border-width) solid var(--lg-border);
}

.serverdesk.is-bar-full .conn-bar__meta {
  font-size: 10px;
}

.serverdesk.is-bar-full .conn-bar__power {
  height: 28px;
  padding: 0 12px;
  font-size: 10.5px;
}

.desk-watermark {
  position: absolute;
  right: 4.5%;
  bottom: 15%;
  z-index: 1;
  color: var(--lg-desktop-text, #e8eff9);
  opacity: 0.055;
  pointer-events: none;
}

.desk-signature {
  position: absolute;
  right: 26px;
  bottom: 76px;
  z-index: 2;
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: 9.5px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  opacity: 0.65;
  pointer-events: none;
}

.desk-icons {
  position: absolute;
  top: calc(var(--desk-bar-space) + 16px);
  left: 14px;
  z-index: 3;
}

.desk-hint {
  position: absolute;
  left: 18px;
  bottom: 68px;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 5px 9px;
  border-radius: var(--lg-radius-sm);
  background: color-mix(in srgb, var(--lg-surface) 42%, transparent);
  backdrop-filter: blur(var(--lg-panel-blur));
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: 10px;
  pointer-events: none;
  animation: desk-hint-fade 9s var(--lg-ease) forwards;
}

@keyframes desk-hint-fade {
  0%,
  72% {
    opacity: 1;
  }
  100% {
    opacity: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .desk-hint {
    animation: none;
  }
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

.desk-start__search {
  padding-top: 10px;
}

.desk-start__empty {
  margin: 4px 0;
  color: var(--lg-text-muted);
  font-size: 11px;
  text-align: center;
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

.desk-tray__mono {
  font-family: var(--lg-font-mono);
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
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  height: 54px;
  margin-top: auto;
  padding: 0 10px;
  border-top: var(--lg-border-width) solid var(--lg-border);
  background: color-mix(in srgb, var(--lg-surface) 82%, transparent);
  backdrop-filter: blur(var(--lg-panel-blur));
}

.desk-taskbar__left,
.desk-taskbar__center,
.desk-taskbar__right {
  display: flex;
  align-items: center;
  gap: 3px;
  min-width: 0;
}

/* 悬浮 Dock：底部居中胶囊，脱离布局流，窗口可以延伸到它后方（macOS 式覆盖）。 */
.serverdesk.is-dock-floating .desk-taskbar {
  position: absolute;
  right: auto;
  bottom: 8px;
  left: 50%;
  display: flex;
  justify-content: center;
  gap: 10px;
  width: auto;
  max-width: calc(100% - 24px);
  height: 46px;
  margin: 0;
  padding: 0 8px;
  transform: translateX(-50%);
  border: var(--lg-border-width) solid var(--lg-border-strong);
  border-radius: var(--lg-radius-full);
  background: color-mix(in srgb, var(--lg-surface) 86%, transparent);
  box-shadow: 0 12px 30px rgb(0 0 0 / 22%), var(--lg-shadow-sm);
}

.desk-taskbar__center {
  justify-content: center;
}

.desk-taskbar__right {
  justify-content: flex-end;
}

.desk-taskbar__start {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 38px;
  height: 38px;
  padding: 0;
  border: 0;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-secondary);
  cursor: pointer;
}

.desk-taskbar__start:hover,
.desk-taskbar__start.is-open {
  background: var(--lg-surface-2);
  color: var(--lg-primary);
}

.desk-taskbar__search {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 7px;
  height: 34px;
  padding: 0 12px 0 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface-2);
  color: var(--lg-text-muted);
  font-size: 11px;
  white-space: nowrap;
  cursor: pointer;
}

.desk-taskbar__search:hover {
  border-color: var(--lg-border-strong);
  color: var(--lg-text-secondary);
}

.desk-taskbar__app {
  position: relative;
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 38px;
  height: 38px;
  padding: 0;
  border: 0;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  cursor: pointer;
}

.desk-taskbar__app:hover {
  background: var(--lg-surface-2);
}

.desk-taskbar__app.is-active {
  background: var(--lg-surface-2);
}

.desk-taskbar__app.is-min {
  opacity: 0.72;
}

.desk-taskbar__app.is-open::after {
  content: '';
  position: absolute;
  bottom: 1px;
  left: 50%;
  width: 14px;
  height: 2px;
  transform: translateX(-50%);
  border-radius: 2px;
  background: var(--lg-primary);
}

.desk-taskbar__app.is-min::after {
  width: 6px;
  opacity: 0.55;
}

.desk-taskbar__icon:not(.desk-taskbar__permit-btn) {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border: 0;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.desk-taskbar__icon:not(.desk-taskbar__permit-btn):hover {
  background: var(--lg-surface-2);
  color: var(--lg-text);
}

.desk-taskbar__tile {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: var(--lg-radius-sm);
}

.desk-taskbar__label,
.desk-taskbar__empty {
  display: none;
}

.desk-clock {
  display: flex;
  flex: 0 0 auto;
  flex-direction: column;
  align-items: flex-end;
  padding: 0 4px 0 8px;
  color: var(--lg-text-secondary);
  font-family: var(--lg-font-mono);
  line-height: 1.2;
  white-space: nowrap;
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

.conn-bar__permit {
  padding-left: 7px;
  border-left: var(--lg-border-width) solid var(--lg-border);
}

.serverdesk.is-permit-unlocked::after {
  content: '';
  position: absolute;
  inset: 0;
  z-index: 400;
  border: 2px solid var(--lg-warning);
  pointer-events: none;
}

.desk-taskbar__permit-btn {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  padding: 0 6px;
  border: var(--lg-border-width) solid transparent;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-secondary);
  cursor: pointer;
}

.desk-taskbar__permit-btn:hover {
  border-color: var(--lg-border);
}

.desk-taskbar__permit {
  color: var(--lg-warning);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
}

.desk-taskbar__permit-btn.is-danger .desk-taskbar__permit {
  color: var(--lg-danger);
}

@media (max-width: 900px) {
  .conn-bar__addr {
    display: none;
  }
}

@media (max-width: 719px) {
  .serverdesk {
    --desk-bar-space: 40px;
  }

  .serverdesk.is-bar-full {
    --desk-bar-space: 46px;
  }

  .conn-bar {
    top: 8px;
    height: 32px;
    gap: 5px;
  }

  .serverdesk.is-bar-full .conn-bar {
    top: 0;
    height: var(--desk-bar-space);
  }

  .conn-bar__mode,
  .conn-bar__uptime,
  .conn-bar__meta {
    display: none;
  }

  .conn-bar__name {
    max-width: 110px;
  }

  .desk-icons {
    top: calc(var(--desk-bar-space) + 10px);
    left: 6px;
  }

  .desk-taskbar {
    height: 50px;
    gap: 4px;
    padding: 0 6px;
  }

  .serverdesk.is-dock-floating .desk-taskbar {
    max-width: calc(100% - 12px);
    padding: 0 6px;
    gap: 4px;
  }

  .desk-taskbar__search {
    display: none;
  }

  .desk-taskbar__center {
    gap: 0;
  }

  .desk-taskbar__app {
    width: 32px;
    height: 32px;
  }

  .desk-taskbar__tile {
    width: 24px;
    height: 24px;
  }

  /* 窄屏只留一个显示器图标（全屏时是“退出全屏”，否则是“显示桌面”）：
     这条规则依赖它们排在__right 的第一位，许可入口必须放在它之后。 */
  .desk-taskbar__right .desk-taskbar__icon:first-child {
    display: none;
  }

  .desk-clock small {
    display: none;
  }

  .desk-watermark {
    right: -2%;
    bottom: 12%;
    width: 40vw;
    height: auto;
  }

  .desk-signature {
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

  /* 两个入口都要让位：子组件的 display 规则与父级同权重，所以这里加一层类名。 */
  .conn-bar .conn-bar__permit,
  .desk-taskbar__right .desk-taskbar__permit-btn {
    display: none;
  }
}
</style>
