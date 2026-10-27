<script setup lang="ts">
import { computed, h, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgAppBackground, LgButton, LgDropdown, LgIcon, themeOptions } from '@lightguard/lightui';
import type { ThemeId } from '@lightguard/lightui';
import { APP_VERSION } from './data';
import { route } from './router';
import { backendAvailable, fleet, setTheme, state, tickAll, unreadEvents } from './store';
import { openWizard, toast } from './ui';
import { isFullscreen, toggleFullscreen } from '../windowState';

const navItems = [
  { name: 'overview' as const, label: '服务器总览', icon: 'dashboard', href: '#/overview' },
  { name: 'connections' as const, label: '连接记录', icon: 'activity', href: '#/connections' },
  { name: 'settings' as const, label: '设置', icon: 'sliders', href: '#/settings' },
];

const refreshing = ref(false);
const sideOpen = ref(false);

watch(
  () => route.value.name,
  () => {
    sideOpen.value = false;
  },
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

const currentTheme = computed(
  () => themeOptions.find((option) => option.id === state.themes.console) ?? themeOptions[0],
);

const themeTrigger = ref<HTMLElement | null>(null);
const themeMenuWidth = ref(208);
let themeObserver: ResizeObserver | null = null;

onMounted(() => {
  const trigger = themeTrigger.value;
  if (!trigger) return;
  const sync = () => {
    themeMenuWidth.value = trigger.offsetWidth;
  };
  sync();
  if (typeof ResizeObserver !== 'undefined') {
    themeObserver = new ResizeObserver(sync);
    themeObserver.observe(trigger);
  }
});

onBeforeUnmount(() => {
  themeObserver?.disconnect();
  themeObserver = null;
});

const themeMenuOptions = computed(() =>
  themeOptions.map((option) => ({ key: option.id, label: option.label })),
);

function renderThemeLabel(option: { key: string | number; label: string }) {
  const active = option.key === state.themes.console;
  const rowWidth = Math.max(140, themeMenuWidth.value - 28);
  return h(
    'div',
    {
      style: {
        display: 'flex',
        alignItems: 'center',
        gap: '9px',
        width: `${rowWidth}px`,
        height: '34px',
        color: active ? 'var(--lg-primary)' : 'var(--lg-text)',
        fontSize: '12.5px',
        fontWeight: active ? '600' : '500',
      },
    },
    [
      h(LgIcon, {
        name: themeGlyphs[String(option.key)] ?? 'moon',
        size: 15,
        style: {
          flex: '0 0 auto',
          color: active ? 'var(--lg-primary)' : 'var(--lg-text-muted)',
        },
      }),
      h(
        'span',
        {
          style: {
            flex: '1 1 auto',
            minWidth: '0',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          },
        },
        option.label,
      ),
      h(LgIcon, {
        name: 'check',
        size: 14,
        style: {
          flex: '0 0 auto',
          color: 'var(--lg-primary)',
          visibility: active ? 'visible' : 'hidden',
        },
      }),
    ],
  );
}

const pageMeta = computed(() => {
  if (route.value.name === 'connections') {
    return {
      eyebrow: 'WORKSPACE / AUDIT LOG',
      title: '连接记录',
      subtitle: `${state.logs.length} 条审计记录 · 仅元数据，不含口令`,
    };
  }
  if (route.value.name === 'settings') {
    return {
      eyebrow: 'WORKSPACE / PREFERENCES',
      title: '设置',
      subtitle: '外观、渲染、安全与本地备份',
    };
  }
  return {
    eyebrow: 'WORKSPACE / FLEET HEALTH',
    title: '服务器总览',
    subtitle: `共 ${fleet.value.total} 台主机，${
      fleet.value.online + fleet.value.warning
    } 台可连接，打开即知是否健康。`,
  };
});

async function refreshFleet() {
  if (refreshing.value) return;
  if (!backendAvailable) {
    toast({ title: '真实 SSH 仅桌面版可用', message: '请在桌面客户端中连接服务器。', tone: 'info' });
    return;
  }
  refreshing.value = true;
  try {
    await tickAll();
    toast({
      title: '实时数据已刷新',
      message: `${fleet.value.sampled} 台在线主机已更新`,
      tone: 'success',
      duration: 2200,
    });
  } finally {
    refreshing.value = false;
  }
}

function onThemeSelect(key: string) {
  setTheme('console', key as ThemeId);
}
</script>

<template>
  <div class="console-app">
    <LgAppBackground />
    <a class="console-skip" href="#lg-main">跳到主要内容</a>

    <button
      v-if="sideOpen"
      type="button"
      class="console-scrim"
      aria-label="关闭导航"
      @click="sideOpen = false"
    />

    <aside class="console-side" :class="{ 'is-open': sideOpen }">
      <a class="console-brand" href="#/overview" aria-label="LightGuard 总览" @click="sideOpen = false">
        <span class="console-brand__mark" aria-hidden="true">
          <LgIcon name="shield" :size="21" />
        </span>
        <span class="console-brand__text">
          <strong>LightGuard</strong>
          <small>轻守卫 · 服务器守卫</small>
        </span>
      </a>

      <nav class="console-nav" aria-label="主导航">
        <a
          v-for="item in navItems"
          :key="item.name"
          class="console-nav__item"
          :class="{ 'is-active': route.name === item.name }"
          :href="item.href"
          :aria-current="route.name === item.name ? 'page' : undefined"
          @click="sideOpen = false"
        >
          <LgIcon :name="item.icon" :size="17" />
          <span>{{ item.label }}</span>
          <span
            v-if="item.name === 'overview' && fleet.attention > 0"
            class="console-nav__badge is-warning"
            :aria-label="`${fleet.attention} 台主机需要关注`"
          >
            {{ fleet.attention > 99 ? '99+' : fleet.attention }}
          </span>
          <span
            v-else-if="item.name === 'connections' && unreadEvents > 0"
            class="console-nav__badge"
            :aria-label="`${unreadEvents} 条异常记录`"
          >
            {{ unreadEvents > 99 ? '99+' : unreadEvents }}
          </span>
        </a>
      </nav>

      <div class="console-side__foot">
        <div class="console-workspace">
          <span class="console-avatar" aria-hidden="true">LG</span>
          <div>
            <strong>本地工作区</strong>
            <small>{{ backendAvailable ? 'SSH 直连 · 只读采集' : '浏览器预览' }} · v{{ APP_VERSION }}</small>
          </div>
          <span class="console-mode__dot" :class="{ 'is-off': !backendAvailable }" aria-hidden="true" />
        </div>

        <LgDropdown
          trigger="click"
          placement="top-start"
          :options="themeMenuOptions"
          :render-label="renderThemeLabel"
          @select="onThemeSelect"
        >
          <button
            ref="themeTrigger"
            type="button"
            class="console-theme"
            :aria-label="`控制台主题，当前 ${currentTheme.label}`"
          >
            <LgIcon :name="themeGlyphs[state.themes.console] ?? 'moon'" :size="16" />
            <span>
              <small>控制台主题</small>
              <strong>{{ currentTheme.label }}</strong>
            </span>
            <LgIcon name="chevron-down" :size="13" />
          </button>
        </LgDropdown>
      </div>
    </aside>

    <div class="console-main">
      <header class="console-topbar">
        <button
          type="button"
          class="console-menu"
          aria-label="打开导航"
          :aria-expanded="sideOpen"
          @click="sideOpen = !sideOpen"
        >
          <LgIcon name="menu" :size="17" />
        </button>
        <div class="console-crumbs">
          <LgIcon name="grid" :size="13" />
          <span>工作区</span>
          <LgIcon name="chevron-right" :size="12" />
          <strong>{{ pageMeta.title }}</strong>
        </div>
        <div class="console-topbar__right">
          <button
            v-if="isFullscreen"
            type="button"
            class="console-fullscreen"
            title="退出全屏 (F11)"
            @click="toggleFullscreen"
          >
            <LgIcon name="monitor" :size="13" />
            退出全屏
          </button>
          <span v-if="fleet.attention > 0" class="console-attention">
            <LgIcon name="alert" :size="12" />
            {{ fleet.attention }} 台需关注
          </span>
          <span class="console-mode-chip">
            <span class="console-mode__dot" :class="{ 'is-off': !backendAvailable }" aria-hidden="true" />
            {{ backendAvailable ? 'SSH 直连' : '预览模式' }}
          </span>
          <span class="console-avatar" aria-label="本地工作区">LG</span>
        </div>
      </header>

      <header class="console-head">
        <div class="console-head__meta">
          <span class="console-eyebrow">{{ pageMeta.eyebrow }}</span>
          <h1>{{ pageMeta.title }}</h1>
          <p>{{ pageMeta.subtitle }}</p>
        </div>
        <div class="console-head__actions">
          <LgButton
            v-if="route.name === 'overview'"
            size="small"
            :loading="refreshing"
            @click="refreshFleet"
          >
            <template #icon><LgIcon name="refresh" :size="14" /></template>
            刷新状态
          </LgButton>
          <LgButton type="primary" size="small" @click="openWizard">
            <template #icon><LgIcon name="plus" :size="15" /></template>
            添加服务器
          </LgButton>
        </div>
      </header>

      <main id="lg-main" class="console-body" tabindex="-1">
        <slot />
      </main>
    </div>
  </div>
</template>

<style scoped>
.console-app {
  position: relative;
  display: grid;
  grid-template-columns: 236px minmax(0, 1fr);
  min-height: 100vh;
  background-color: var(--lg-bg);
  background-image: var(--lg-app-bg-image), var(--lg-app-grid-image);
  background-size: auto, var(--lg-app-grid-size);
  color: var(--lg-text);
}

.console-skip {
  position: fixed;
  top: -100px;
  left: 16px;
  z-index: 90;
  padding: 8px 12px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-primary);
  color: var(--lg-on-primary);
  font-size: 12px;
  text-decoration: none;
}

.console-skip:focus {
  top: 12px;
}

.console-side {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  flex-direction: column;
  gap: 22px;
  height: 100vh;
  padding: 22px 14px 18px;
  border-right: 1px solid var(--lg-border);
  background: color-mix(in srgb, var(--lg-bg-soft) 82%, transparent);
  backdrop-filter: blur(var(--lg-panel-blur));
}

.console-brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 8px;
  color: inherit;
  text-decoration: none;
}

.console-brand__mark {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 36px;
  height: 36px;
  border: 1px solid var(--lg-border-strong);
  border-radius: 12px;
  background: linear-gradient(155deg, var(--lg-primary-soft), var(--lg-accent-soft));
  color: var(--lg-primary);
}

.console-brand__text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.console-brand__text strong {
  font-size: 15px;
  font-weight: 650;
  letter-spacing: 0.01em;
}

.console-brand__text small {
  color: var(--lg-text-muted);
  font-size: 11px;
}

.console-nav {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.console-nav__item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 11px;
  border: 1px solid transparent;
  border-radius: var(--lg-radius-sm);
  color: var(--lg-text-secondary);
  font-size: 13px;
  text-decoration: none;
  transition:
    background-color var(--lg-dur-fast) var(--lg-ease),
    color var(--lg-dur-fast) var(--lg-ease);
}

.console-nav__item:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.console-nav__item.is-active {
  border-color: color-mix(in srgb, var(--lg-primary) 24%, transparent);
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
  font-weight: 600;
}

.console-nav__item.is-active::before {
  content: '';
  position: absolute;
  top: 8px;
  bottom: 8px;
  left: 0;
  width: 2px;
  border-radius: 2px;
  background: var(--lg-primary);
}

.console-nav__badge {
  margin-left: auto;
  padding: 1px 6px;
  border-radius: var(--lg-radius-full);
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
  font-size: 10px;
  font-weight: 700;
}

.console-nav__badge.is-warning {
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.console-side__foot {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: auto;
}

.console-workspace {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 9px 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.console-workspace > div {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.console-workspace strong {
  font-size: 11.5px;
  font-weight: 600;
}

.console-workspace small {
  overflow: hidden;
  color: var(--lg-text-muted);
  font-size: 9.5px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.console-avatar {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 28px;
  height: 28px;
  border: 1px solid color-mix(in srgb, var(--lg-primary) 30%, var(--lg-border));
  border-radius: var(--lg-radius-sm);
  background: linear-gradient(145deg, var(--lg-primary-soft), var(--lg-accent-soft));
  color: var(--lg-primary);
  font-family: var(--lg-font-mono);
  font-size: 10px;
  font-weight: 700;
}

.console-mode__dot {
  flex: 0 0 auto;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--lg-success);
  box-shadow: 0 0 0 4px var(--lg-success-soft);
}

.console-mode__dot.is-off {
  background: var(--lg-warning);
  box-shadow: 0 0 0 4px var(--lg-warning-soft);
}

.console-mode div {
  display: flex;
  flex-direction: column;
}

.console-mode strong {
  font-size: 11px;
  font-weight: 600;
}

.console-mode small {
  color: var(--lg-text-muted);
  font-size: 10px;
}

.console-theme {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 9px 11px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: transparent;
  color: var(--lg-text-secondary);
  text-align: left;
  cursor: pointer;
}

.console-theme:hover {
  border-color: var(--lg-border-strong);
  background: var(--lg-hover);
}

.console-theme > span {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.console-theme small {
  color: var(--lg-text-muted);
  font-size: 10px;
}

.console-theme strong {
  color: var(--lg-text);
  font-size: 12px;
  font-weight: 600;
}

.console-menu {
  display: none;
}

.console-scrim {
  position: fixed;
  inset: 0;
  z-index: 55;
  padding: 0;
  border: 0;
  background: color-mix(in srgb, #000 42%, transparent);
}

.console-main {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.console-topbar {
  position: sticky;
  top: 0;
  z-index: 30;
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  height: 46px;
  padding: 0 clamp(18px, 3vw, 38px);
  border-bottom: var(--lg-border-width) solid var(--lg-border);
  background: color-mix(in srgb, var(--lg-bg-soft) 78%, transparent);
  backdrop-filter: blur(var(--lg-panel-blur));
}

.console-crumbs {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  color: var(--lg-text-muted);
  font-size: 11.5px;
  white-space: nowrap;
}

.console-crumbs strong {
  overflow: hidden;
  color: var(--lg-text);
  font-weight: 600;
  text-overflow: ellipsis;
}

.console-topbar__right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.console-fullscreen {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 9px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: 10.5px;
  white-space: nowrap;
  cursor: pointer;
}

.console-fullscreen:hover {
  border-color: var(--lg-border-strong);
  color: var(--lg-text);
}

.console-attention {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border: var(--lg-border-width) solid color-mix(in srgb, var(--lg-warning) 28%, var(--lg-border));
  border-radius: var(--lg-radius-full);
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
  font-size: 10.5px;
  white-space: nowrap;
}

.console-mode-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 8px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  color: var(--lg-text-muted);
  font-size: 10.5px;
  white-space: nowrap;
}

.console-mode-chip .console-mode__dot {
  width: 5px;
  height: 5px;
  box-shadow: 0 0 0 3px var(--lg-success-soft);
}

.console-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 18px;
  padding: 24px clamp(18px, 3vw, 38px) 0;
  flex-wrap: wrap;
}

.console-eyebrow {
  display: flex;
  align-items: center;
  gap: 7px;
  margin-bottom: 8px;
  color: var(--lg-primary);
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.13em;
  text-transform: uppercase;
}

.console-eyebrow::before {
  content: '';
  width: 18px;
  height: 1px;
  background: var(--lg-primary);
  opacity: 0.7;
}

.console-head__meta h1 {
  margin: 0;
  font-family: var(--lg-font-display, var(--lg-font-sans));
  font-size: clamp(23px, 2.4vw, 28px);
  font-weight: 700;
  letter-spacing: -0.025em;
  line-height: 1.2;
}

.console-head__meta p {
  margin: 7px 0 0;
  color: var(--lg-text-secondary);
  font-size: 12.5px;
}

.console-head__actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.console-demo-tag {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 8px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  color: var(--lg-text-muted);
  font-size: 11px;
  white-space: nowrap;
}

.console-body {
  flex: 1;
  width: 100%;
  max-width: 1560px;
  margin: 0 auto;
  padding: 20px clamp(18px, 3vw, 38px) 46px;
  outline: none;
}

@media (max-width: 640px) {
  .console-attention,
  .console-mode-chip {
    display: none;
  }

  .console-head {
    padding-top: 18px;
  }

  .console-topbar {
    padding: 0 14px;
  }
}

@media (max-width: 980px) {
  .console-app {
    grid-template-columns: minmax(0, 1fr);
  }

  .console-side {
    position: fixed;
    top: 0;
    left: 0;
    z-index: 60;
    width: min(280px, 82vw);
    height: 100dvh;
    padding: 18px 14px;
    transform: translateX(-100%);
    border-right: var(--lg-border-width) solid var(--lg-border);
    background: var(--lg-bg-soft);
    transition: transform var(--lg-dur-base) var(--lg-ease);
  }

  .console-side.is-open {
    transform: none;
    box-shadow: var(--lg-shadow-lg);
  }

  .console-menu {
    display: grid;
    flex: 0 0 auto;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: var(--lg-border-width) solid var(--lg-border);
    border-radius: var(--lg-radius-sm);
    background: var(--lg-surface);
    color: var(--lg-text-secondary);
    cursor: pointer;
  }

  .console-menu:hover {
    border-color: var(--lg-border-strong);
    color: var(--lg-text);
  }

  .console-side__foot {
    flex-direction: column;
    align-items: stretch;
    margin-top: auto;
  }

  .console-topbar {
    padding: 0 14px;
  }

  .console-theme {
    width: 100%;
    white-space: nowrap;
  }

  .console-head {
    padding-top: 18px;
  }
}

@media (max-width: 620px) {
  .console-head {
    align-items: flex-start;
    flex-direction: column;
    gap: 10px;
  }

  .console-head__actions {
    width: 100%;
    justify-content: space-between;
  }

  .console-side__foot {
    align-items: stretch;
    flex-direction: column;
  }

  .console-theme {
    width: 100%;
  }

  .console-body {
    padding-bottom: 32px;
  }
}
</style>
