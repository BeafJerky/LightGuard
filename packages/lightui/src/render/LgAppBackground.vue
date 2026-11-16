<script setup lang="ts">
import { computed, ref } from 'vue';
import CanvasSurface from './CanvasSurface.vue';
import BackgroundShaderCanvas from './BackgroundShaderCanvas.vue';
import BackgroundShaderWebgpu from './BackgroundShaderWebgpu.vue';
import { useRenderBackend } from './useRenderBackend';
import { drawAppBackground } from './paint/background';

withDefaults(defineProps<{ paused?: boolean }>(), { paused: false });

const backend = useRenderBackend('app.background');
const webgpuFailed = ref(false);
const shaderFailed = ref(false);
const active = computed(() => backend.value !== 'dom');
const paint = computed(() => {
  if (backend.value === 'webgpu' && !webgpuFailed.value) return 'webgpu';
  if ((backend.value === 'webgpu' || backend.value === 'webgl') && !shaderFailed.value) {
    return 'webgl';
  }
  return 'canvas';
});
</script>

<template>
  <div v-if="active" class="lg-app-background" aria-hidden="true" :data-lg-backend="paint">
    <BackgroundShaderWebgpu v-if="paint === 'webgpu'" @fail="webgpuFailed = true" />
    <BackgroundShaderCanvas v-else-if="paint === 'webgl'" @fail="shaderFailed = true" />
    <CanvasSurface v-else :draw="drawAppBackground" :paused="paused" />
  </div>
</template>

<style scoped>
.lg-app-background {
  position: absolute;
  inset: 0;
  z-index: 0;
  overflow: hidden;
  border-radius: inherit;
  pointer-events: none;
}
</style>
