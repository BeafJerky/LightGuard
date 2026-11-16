<script setup lang="ts">
import { inject, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LG_SCOPE_KEY } from '../theme/context';
import {
  createBackgroundShaderWebgpu,
  type BackgroundShaderWebgpuHandle,
} from './paint/backgroundShaderWebgpu';

const emit = defineEmits<{ fail: [] }>();

const canvasEl = ref<HTMLCanvasElement | null>(null);
const scope = inject(LG_SCOPE_KEY, null);

let handle: BackgroundShaderWebgpuHandle | null = null;
let observer: ResizeObserver | null = null;
let disposed = false;

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

onMounted(async () => {
  if (!canvasEl.value) return;
  const created = await createBackgroundShaderWebgpu(canvasEl.value, () => {
    if (disposed) return;
    handle?.dispose();
    handle = null;
    emit('fail');
  });
  if (disposed) {
    created?.dispose();
    return;
  }
  handle = created;
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
  disposed = true;
  observer?.disconnect();
  observer = null;
  handle?.dispose();
  handle = null;
});
</script>

<template>
  <div class="lg-shader-canvas" data-lg-render="webgpu">
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