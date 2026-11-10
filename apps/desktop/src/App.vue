<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, watch, watchEffect } from 'vue';
import { usePreferredDark } from '@vueuse/core';
import {
  LgConfigProvider,
  LgIconSprite,
  LgRenderProvider,
  type ThemeId,
  type ThemeScope,
} from '@lightguard/lightui';
import AddServerWizard from './product/AddServerWizard.vue';
import AlertsPage from './product/alerts/AlertsPage.vue';
import ConnectionsPage from './product/ConnectionsPage.vue';
import ConsoleShell from './product/ConsoleShell.vue';
import OverviewPage from './product/OverviewPage.vue';
import SettingsPage from './product/SettingsPage.vue';
import UiHost from './product/UiHost.vue';
import ServerDesktop from './product/desktop/ServerDesktop.vue';
import { syncAutostart } from './product/desktop/autostart';
import WindowFrame from './WindowFrame.vue';
import ThemeSwitchOverlay from './ThemeSwitchOverlay.vue';
import { backendAvailable, findServer, restartLiveUpdates, state, stopLiveUpdates } from './product/store';
import { navigateTo, route, routeTitle, startRouter } from './product/router';
import { startUpdateWatcher, updateController } from './product/updater';
import { syncAppIcons } from './product/osIcon';
import { isTauriRuntime } from './product/ssh/tauri';
import { renderSettings } from './render/settings';

startRouter();

const scope = computed<ThemeScope>(() => (route.value.name === 'desktop' ? 'desktop' : 'console'));
const theme = computed<ThemeId>(() =>
  scope.value === 'desktop' ? state.themes.desktop : state.themes.console,
);

// 窗口 / 任务栏图标跟随控制台主题；托盘由 syncAppIcons 按系统外观同步。
const prefersDark = usePreferredDark();
const framed = isTauriRuntime();

watchEffect(() => {
  if (!framed) return;
  void syncAppIcons(state.themes.console, prefersDark.value);
});

const desktopServer = computed(() => {
  if (route.value.name !== 'desktop' || !route.value.id) return null;
  return findServer(route.value.id) ?? null;
});

const page = computed(() => {
  const name = route.value.name === 'desktop' ? 'overview' : route.value.name;
  if (name === 'alerts' && !backendAvailable) return 'overview';
  return name;
});

watchEffect(() => {
  if (route.value.name === 'desktop' && !desktopServer.value) {
    navigateTo('overview', true);
  }
});

watchEffect(() => {
  if (route.value.name === 'alerts' && !backendAvailable) navigateTo('overview', true);
});

watchEffect(() => {
  if (desktopServer.value) {
    document.title = `${desktopServer.value.name} · LightGuard`;
    return;
  }
  document.title = `${routeTitle(page.value)} · LightGuard`;
});

let stopUpdateWatcher: (() => void) | undefined;

onMounted(() => {
  restartLiveUpdates();
  if (!backendAvailable) return;
  void syncAutostart();
  stopUpdateWatcher = startUpdateWatcher(updateController);
});

onBeforeUnmount(() => {
  stopLiveUpdates();
  stopUpdateWatcher?.();
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
          <AlertsPage v-else-if="page === 'alerts'" />
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
