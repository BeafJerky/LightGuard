<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, watch, watchEffect } from 'vue';
import {
  LgConfigProvider,
  LgIconSprite,
  LgRenderProvider,
  type ThemeId,
  type ThemeScope,
} from '@lightguard/lightui';
import AddServerWizard from './product/AddServerWizard.vue';
import ConnectionsPage from './product/ConnectionsPage.vue';
import ConsoleShell from './product/ConsoleShell.vue';
import OverviewPage from './product/OverviewPage.vue';
import SettingsPage from './product/SettingsPage.vue';
import UiHost from './product/UiHost.vue';
import ServerDesktop from './product/desktop/ServerDesktop.vue';
import WindowFrame from './WindowFrame.vue';
import ThemeSwitchOverlay from './ThemeSwitchOverlay.vue';
import { findServer, restartLiveUpdates, state, stopLiveUpdates } from './product/store';
import { navigateTo, route, startRouter } from './product/router';
import { renderSettings } from './render/settings';

startRouter();

const scope = computed<ThemeScope>(() => (route.value.name === 'desktop' ? 'desktop' : 'console'));
const theme = computed<ThemeId>(() =>
  scope.value === 'desktop' ? state.themes.desktop : state.themes.console,
);

const desktopServer = computed(() => {
  if (route.value.name !== 'desktop' || !route.value.id) return null;
  return findServer(route.value.id) ?? null;
});

const page = computed(() => (route.value.name === 'desktop' ? 'overview' : route.value.name));

watchEffect(() => {
  if (route.value.name === 'desktop' && !desktopServer.value) {
    navigateTo('overview', true);
  }
});

watchEffect(() => {
  if (desktopServer.value) {
    document.title = `${desktopServer.value.name} · LightGuard`;
    return;
  }
  const titles: Record<string, string> = {
    overview: '服务器总览',
    connections: '连接记录',
    settings: '设置',
  };
  document.title = `${titles[page.value] ?? '服务器总览'} · LightGuard`;
});

onMounted(() => {
  restartLiveUpdates();
});

onBeforeUnmount(() => {
  stopLiveUpdates();
});

watch(
  () => [state.settings.autoRefresh, state.settings.interval],
  () => restartLiveUpdates(),
);
</script>

<template>
  <LgIconSprite />
  <LgRenderProvider :settings="renderSettings">
    <LgConfigProvider class="product-scope" :theme="theme" :scope="scope">
      <WindowFrame>
        <ServerDesktop v-if="desktopServer" :key="desktopServer.id" :server="desktopServer" />
        <ConsoleShell v-else>
          <OverviewPage v-if="page === 'overview'" />
          <ConnectionsPage v-else-if="page === 'connections'" />
          <SettingsPage v-else />
        </ConsoleShell>

        <ThemeSwitchOverlay />
        <UiHost />
        <AddServerWizard />
      </WindowFrame>
    </LgConfigProvider>
  </LgRenderProvider>
</template>
