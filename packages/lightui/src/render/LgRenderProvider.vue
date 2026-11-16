<script setup lang="ts">
import { computed, onMounted, provide, ref } from 'vue';
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

const detected = ref(detectCapabilities());
const capabilities = computed(() => withCapabilities(detected.value, props.capabilities));

// `navigator.gpu` 存在不代表能用：WebView2 可能没有可用 adapter。探测成功后
// 立即销毁探测设备，真正绘制时再创建；失败则永久回退 WebGL/Canvas。
onMounted(async () => {
  if (!detected.value.webgpu || props.capabilities?.webgpu !== undefined) return;
  const gpu = (navigator as unknown as {
    gpu?: { requestAdapter: () => Promise<{ requestDevice: () => Promise<{ destroy?: () => void }> } | null> };
  }).gpu;
  if (!gpu) {
    detected.value = { ...detected.value, webgpu: false };
    return;
  }
  try {
    const adapter = await gpu.requestAdapter();
    const device = adapter ? await adapter.requestDevice() : null;
    if (!adapter || !device) {
      detected.value = { ...detected.value, webgpu: false };
      return;
    }
    device.destroy?.();
  } catch {
    detected.value = { ...detected.value, webgpu: false };
  }
});
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
