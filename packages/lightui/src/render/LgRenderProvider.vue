<script setup lang="ts">
import { computed, provide } from 'vue';
import { detectCapabilities, withCapabilities } from './capabilities';
import { LG_RENDER_KEY } from './context';
import { resolveRenderBackend } from './resolve';
import { DEFAULT_RENDER_SETTINGS } from './settings';
import type {
  RenderCapabilities,
  RenderScopeContext,
  RenderSettings,
} from './types';

const props = defineProps<{
  settings?: RenderSettings;
  capabilities?: Partial<RenderCapabilities>;
}>();

const detected = detectCapabilities();
const capabilities = computed(() => withCapabilities(detected, props.capabilities));
const settings = computed(() => props.settings ?? DEFAULT_RENDER_SETTINGS);

const context = computed<RenderScopeContext>(() => {
  const currentSettings = settings.value;
  const currentCapabilities = capabilities.value;
  return {
    get settings() {
      return currentSettings;
    },
    get capabilities() {
      return currentCapabilities;
    },
    resolve: (tier, hint) =>
      resolveRenderBackend({
        tier,
        preference: currentSettings.tiers[tier] ?? 'inherit',
        settings: currentSettings,
        capabilities: currentCapabilities,
        hint,
      }),
  };
});

provide(LG_RENDER_KEY, context);
</script>

<template>
  <slot />
</template>
