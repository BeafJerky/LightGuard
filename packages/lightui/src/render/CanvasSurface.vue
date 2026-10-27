<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { usePreferredReducedMotion } from '@vueuse/core';
import { LG_SCOPE_KEY } from '../theme/context';
import type { CanvasFrame } from './types';

const props = withDefaults(
  defineProps<{
    draw: (ctx: CanvasRenderingContext2D, frame: CanvasFrame) => void;
    width?: number;
    height?: number;
    maxDpr?: number;
    animated?: boolean;
    paused?: boolean;
    redrawKey?: unknown;
    ariaLabel?: string;
    role?: string;
  }>(),
  { maxDpr: 2, animated: false, paused: false },
);

const host = ref<HTMLDivElement | null>(null);
const canvasEl = ref<HTMLCanvasElement | null>(null);
const failed = ref(false);
const windowFocused = ref(true);
const scope = inject(LG_SCOPE_KEY, null);
const reduced = usePreferredReducedMotion();

let ctx: CanvasRenderingContext2D | null = null;
let observer: ResizeObserver | null = null;
let frame = 0;
let cssWidth = 0;
let cssHeight = 0;
let dpr = 1;
let disposed = false;

const hostStyle = computed(() => {
  const style: Record<string, string> = {};
  if (props.width !== undefined) style.width = `${props.width}px`;
  if (props.height !== undefined) style.height = `${props.height}px`;
  return style;
});

const canvasRole = computed(() => props.role ?? (props.ariaLabel ? 'img' : undefined));

const shouldAnimate = computed(
  () =>
    props.animated &&
    !props.paused &&
    !failed.value &&
    windowFocused.value &&
    reduced.value !== 'reduce' &&
    (typeof document === 'undefined' || !document.hidden),
);

function render(time: number) {
  if (!ctx || disposed) return;
  const theme = scope?.value.theme;
  if (!theme) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, cssWidth, cssHeight);
  props.draw(ctx, { width: cssWidth, height: cssHeight, dpr, time, theme });
}

function resize() {
  const canvas = canvasEl.value;
  const hostEl = host.value;
  if (!canvas || !hostEl) return;

  const rect = hostEl.getBoundingClientRect();
  cssWidth = Math.max(1, Math.round(props.width ?? rect.width));
  cssHeight = Math.max(1, Math.round(props.height ?? rect.height));
  dpr = Math.min(window.devicePixelRatio || 1, props.maxDpr);

  const pixelWidth = Math.max(1, Math.round(cssWidth * dpr));
  const pixelHeight = Math.max(1, Math.round(cssHeight * dpr));
  if (canvas.width !== pixelWidth) canvas.width = pixelWidth;
  if (canvas.height !== pixelHeight) canvas.height = pixelHeight;

  try {
    ctx = canvas.getContext('2d');
  } catch {
    ctx = null;
  }
  failed.value = !ctx;
  if (!ctx) return;

  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  if (!shouldAnimate.value) render(0);
}

function tick(now: number) {
  if (!shouldAnimate.value) {
    frame = 0;
    return;
  }
  render(now);
  frame = requestAnimationFrame(tick);
}

function syncLoop() {
  if (frame) {
    cancelAnimationFrame(frame);
    frame = 0;
  }
  if (shouldAnimate.value) {
    frame = requestAnimationFrame(tick);
  } else {
    render(0);
  }
}

function onVisibilityChange() {
  syncLoop();
}

function onWindowBlur() {
  windowFocused.value = false;
}

function onWindowFocus() {
  windowFocused.value = true;
}

watch(shouldAnimate, syncLoop);
watch(() => props.paused, syncLoop);
watch(() => [props.width, props.height, props.maxDpr], resize);
watch(
  () => props.redrawKey,
  () => {
    if (!shouldAnimate.value) render(0);
  },
  { deep: true },
);
watch(
  () => scope?.value.theme.id,
  () => {
    if (!shouldAnimate.value) render(0);
  },
);

onMounted(() => {
  resize();
  if (typeof ResizeObserver !== 'undefined' && host.value) {
    observer = new ResizeObserver(resize);
    observer.observe(host.value);
  }
  document.addEventListener('visibilitychange', onVisibilityChange);
  window.addEventListener('blur', onWindowBlur);
  window.addEventListener('focus', onWindowFocus);
  syncLoop();
});

onBeforeUnmount(() => {
  disposed = true;
  if (frame) cancelAnimationFrame(frame);
  observer?.disconnect();
  observer = null;
  document.removeEventListener('visibilitychange', onVisibilityChange);
  window.removeEventListener('blur', onWindowBlur);
  window.removeEventListener('focus', onWindowFocus);
});

defineExpose({
  canvas: canvasEl,
  redraw: () => render(0),
  resize,
});
</script>

<template>
  <div ref="host" class="lg-canvas-surface" :style="hostStyle" data-lg-render="canvas">
    <canvas
      ref="canvasEl"
      class="lg-canvas-surface__canvas"
      :class="{ 'is-failed': failed }"
      :role="canvasRole"
      :aria-label="ariaLabel"
    />
    <slot v-if="failed" name="fallback" />
  </div>
</template>
