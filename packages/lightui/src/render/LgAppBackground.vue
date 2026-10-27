<script setup lang="ts">
import { computed, ref } from 'vue';
import CanvasSurface from './CanvasSurface.vue';
import BackgroundShaderCanvas from './BackgroundShaderCanvas.vue';
import { useRenderBackend } from './useRenderBackend';
import { drawAppBackground } from './paint/background';

withDefaults(defineProps<{ paused?: boolean }>(), { paused: false });

const backend = useRenderBackend('app.background');
const shaderFailed = ref(false);
const active = computed(() => backend.value !== 'dom');
const paint = computed(() =>
  backend.value === 'webgl' && !shaderFailed.value ? 'webgl' : 'canvas',
);
</script>

<template>
  <div v-if="active" class="lg-app-background" aria-hidden="true" :data-lg-backend="paint">
    <BackgroundShaderCanvas v-if="paint === 'webgl'" @fail="shaderFailed = true" />
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
