<script setup lang="ts">
import { computed, provide, ref } from 'vue';
import { usePreferredDark } from '@vueuse/core';
import {
  NConfigProvider,
  NDialogProvider,
  NMessageProvider,
  NNotificationProvider,
  darkTheme,
  dateZhCN,
  zhCN,
} from 'naive-ui';
import { LG_SCOPE_KEY } from '../theme/context';
import { resolveTheme } from '../theme/tokens';
import { toCssVars } from '../theme/css';
import { toNaiveOverrides } from '../theme/naive';
import type { ThemeId, ThemeScope, ThemeScopeContext } from '../theme/types';

const props = withDefaults(
  defineProps<{
    theme?: ThemeId;
    scope?: ThemeScope;
  }>(),
  {
    theme: 'system',
    scope: 'console',
  },
);

const prefersDark = usePreferredDark();
const resolved = computed(() => resolveTheme(props.theme, prefersDark.value));
const themeOverrides = computed(() => toNaiveOverrides(resolved.value));
const cssVars = computed(() => ({
  ...toCssVars(resolved.value),
  'color-scheme': resolved.value.mode,
}));
const naiveTheme = computed(() => (resolved.value.mode === 'dark' ? darkTheme : null));

const portalEl = ref<HTMLElement | null>(null);

const scopeContext = computed<ThemeScopeContext>(() => ({
  theme: resolved.value,
  scope: props.scope,
  portalEl: portalEl.value,
}));

provide(LG_SCOPE_KEY, scopeContext);
</script>

<template>
  <div
    class="lg-scope"
    :data-lg-theme="resolved.id"
    :data-lg-pack="resolved.pack"
    :data-lg-mode="resolved.mode"
    :data-lg-scope="scope"
    :style="cssVars"
  >
    <div ref="portalEl" class="lg-portal" />
    <NConfigProvider
      :theme="naiveTheme"
      :theme-overrides="themeOverrides"
      :locale="zhCN"
      :date-locale="dateZhCN"
    >
      <NMessageProvider :to="portalEl ?? undefined">
        <NDialogProvider :to="portalEl ?? undefined">
          <NNotificationProvider :to="portalEl ?? undefined">
            <slot />
          </NNotificationProvider>
        </NDialogProvider>
      </NMessageProvider>
    </NConfigProvider>
  </div>
</template>
