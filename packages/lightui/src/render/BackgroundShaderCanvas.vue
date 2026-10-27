<script setup lang="ts">
import { inject, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LG_SCOPE_KEY } from '../theme/context';
import { createBackgroundShader, type BackgroundShaderHandle } from './paint/backgroundShader';

const emit = defineEmits<{ fail: [] }>();

const canvasEl = ref<HTMLCanvasElement | null>(null);
const scope = inject(LG_SCOPE_KEY, null);

let handle: BackgroundShaderHandle | null = null;
let observer: ResizeObserver | null = null;

function paint() {
  const theme = scope?.value.theme;
  if (handle && theme) handle.draw(theme);
}

function resize() {
  if (!handle || !canvasEl.value) return;
  const rect = canvasEl.value.getBoundingClientRect();
  handle.resize(rect.width, rect.height, Math.min(window.devicePixelRatio || 1, 2));
  paint();
}

onMounted(() => {
  if (!canvasEl.value) return;
  handle = createBackgroundShader(canvasEl.value);
  if (!handle) {
    emit('fail');
    return;
  }
  resize();
  if (typeof ResizeObserver !== 'undefined' && canvasEl.value) {
    observer = new ResizeObserver(resize);
    observer.observe(canvasEl.value);
  }
});

watch(() => scope?.value.theme.id, resize);

onBeforeUnmount(() => {
  observer?.disconnect();
  observer = null;
  handle?.dispose();
  handle = null;
});
</script>

<template>
  <div class="lg-shader-canvas" data-lg-render="webgl">
    <canvas ref="canvasEl" class="lg-shader-canvas__canvas" />
  </div>
</template>

<style scoped>
.lg-shader-canvas {
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.lg-shader-canvas__canvas {
  display: block;
  width: 100%;
  height: 100%;
}
</style>
