<script setup lang="ts">
import { computed } from 'vue';
import { LgAppBackground, LgButton, LgDropdown, LgIcon, themeOptions } from '@lightguard/lightui';
import type { ThemeId } from '@lightguard/lightui';
import { route } from './router';
import { fleet, setTheme, state, unreadEvents } from './store';
import { openWizard } from './ui';

const navItems = [
  { name: 'overview' as const, label: '总览', icon: 'dashboard', href: '#/overview' },
  { name: 'connections' as const, label: '连接记录', icon: 'activity', href: '#/connections' },
  { name: 'settings' as const, label: '设置', icon: 'sliders', href: '#/settings' },
];

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

const themeMenuOptions = computed(() =>
  themeOptions.map((option) => ({
    key: option.id,
    label:
      option.id === state.themes.console ? `✓  ${option.label}` : `　　${option.label}`,
  })),
);

const pageMeta = computed(() => {
  if (route.value.name === 'connections') {
    return {
      title: '连接记录',
      subtitle: `${state.logs.length} 条审计记录 · 仅元数据，不含口令`,
    };
  }
  if (route.value.name === 'settings') {
    return { title: '设置', subtitle: '外观、渲染、安全与本地备份' };
  }
  return {
    title: '服务器总览',
    subtitle: `共 ${fleet.value.total} 台主机 · ${fleet.value.online + fleet.value.warning} 台可连接 · 本地演示数据`,
  };
});

function onThemeSelect(key: string) {
  setTheme('console', key as ThemeId);
}
</script>

<template>
  <div class="console-app">
    <LgAppBackground />
    <a class="console-skip" href="#lg-main">跳到主要内容</a>

    <aside class="console-side">
      <a class="console-brand" href="#/overview" aria-label="LightGuard 总览">
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
        >
          <LgIcon :name="item.icon" :size="17" />
          <span>{{ item.label }}</span>
          <span
            v-if="item.name === 'connections' && unreadEvents > 0"
            class="console-nav__badge"
            :aria-label="`${unreadEvents} 条异常记录`"
          >
            {{ unreadEvents > 99 ? '99+' : unreadEvents }}
          </span>
        </a>
      </nav>

      <div class="console-side__foot">
        <div class="console-mode">
          <span class="console-mode__dot" aria-hidden="true" />
          <div>
            <strong>本地演示模式</strong>
            <small>不发起真实网络请求</small>
          </div>
        </div>

        <LgDropdown
          trigger="click"
          placement="top-start"
          :options="themeMenuOptions"
          @select="onThemeSelect"
        >
          <button
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
      <header class="console-head">
        <div class="console-head__meta">
          <h1>{{ pageMeta.title }}</h1>
          <p>{{ pageMeta.subtitle }}</p>
        </div>
        <div class="console-head__actions">
          <span class="console-demo-tag">
            <LgIcon name="lock" :size="12" />
            P0 演示
          </span>
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
  backdrop-filter: blur(18px);
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

.console-nav__badge {
  margin-left: auto;
  padding: 1px 6px;
  border-radius: var(--lg-radius-full);
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
  font-size: 10px;
  font-weight: 700;
}

.console-side__foot {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: auto;
}

.console-mode {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 10px 11px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.console-mode__dot {
  flex: 0 0 auto;
  width: 7px;
  height: 7px;
  border-radius: 50%;
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

.console-main {
  position: relative;
  z-index: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.console-head {
  position: sticky;
  top: 0;
  z-index: 20;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
  padding: 20px clamp(18px, 3vw, 38px) 14px;
  border-bottom: 1px solid transparent;
  background: linear-gradient(to bottom, var(--lg-bg), color-mix(in srgb, var(--lg-bg) 72%, transparent));
  backdrop-filter: blur(10px);
}

.console-head__meta h1 {
  margin: 0;
  font-size: clamp(19px, 2vw, 23px);
  font-weight: 650;
  letter-spacing: -0.01em;
}

.console-head__meta p {
  margin: 3px 0 0;
  color: var(--lg-text-muted);
  font-size: 12px;
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
  border: 1px solid var(--lg-border);
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
  padding: 18px clamp(18px, 3vw, 38px) 46px;
  outline: none;
}

@media (max-width: 980px) {
  .console-app {
    grid-template-columns: minmax(0, 1fr);
  }

  .console-side {
    position: static;
    height: auto;
    gap: 12px;
    padding: 14px 16px;
    border-right: 0;
    border-bottom: 1px solid var(--lg-border);
  }

  .console-nav {
    flex-direction: row;
    gap: 6px;
    overflow-x: auto;
  }

  .console-nav__item {
    padding: 7px 12px;
    white-space: nowrap;
  }

  .console-side__foot {
    flex-direction: row;
    align-items: center;
    margin-top: 0;
  }

  .console-mode {
    flex: 1;
  }

  .console-theme {
    width: auto;
    white-space: nowrap;
  }

  .console-head {
    position: static;
    padding-top: 16px;
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
