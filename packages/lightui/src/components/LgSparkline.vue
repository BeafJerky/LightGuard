<script setup lang="ts">
import { computed, useId } from 'vue';
import LgCanvasSurface from '../render/CanvasSurface.vue';
import { useRenderBackend } from '../render/useRenderBackend';
import { withAlpha } from '../render/paint/colors';
import type { CanvasFrame } from '../render/types';
import type { ThemeTokens as ThemeTokenSource } from '../theme/types';

const props = withDefaults(
  defineProps<{
    values: number[];
    height?: number;
    max?: number;
    color?: string;
    label?: string;
  }>(),
  { height: 56 },
);

const uid = useId();
const width = 240;
const padY = 3;
// 面积图必须铺满整个宽度，否则右端会留下一条竖直的渐变边，看起来像滚动条。
const padX = 0.5;

const backend = useRenderBackend('console.charts', () => ({ count: props.values.length }));
const canvas = computed(() => backend.value === 'canvas');
const ariaLabel = computed(() => props.label ?? '趋势图');

function resolveStroke(color: string | undefined, theme: ThemeTokenSource) {
  if (!color) return theme.color.primary;
  const match = /^var\(--lg-([a-z0-9-]+)\)$/.exec(color.trim());
  if (!match) return color;
  const key = match[1].replace(/-([a-z])/g, (_, char: string) => char.toUpperCase());
  return (theme.color as unknown as Record<string, string>)[key] ?? theme.color.primary;
}

function draw(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const values = props.values;
  if (values.length < 2 || frame.width < 2) return;

  const { width: frameWidth, height: frameHeight, theme } = frame;
  const dataMax = Math.max(...values, 0);
  const ceiling = props.max ?? (dataMax > 0 ? dataMax * 1.12 : 1);
  const step = (frameWidth - padX * 2) / (values.length - 1);
  const points = values.map((value, index) => ({
    x: padX + index * step,
    y: frameHeight - padY - (Math.max(0, value) / ceiling) * (frameHeight - padY * 2),
  }));

  const stroke = resolveStroke(props.color, theme);
  const gradient = ctx.createLinearGradient(0, 0, 0, frameHeight);
  gradient.addColorStop(0, withAlpha(stroke, 0.3));
  gradient.addColorStop(1, withAlpha(stroke, 0));

  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (const point of points.slice(1)) ctx.lineTo(point.x, point.y);
  ctx.lineTo(points[points.length - 1].x, frameHeight - padY);
  ctx.lineTo(points[0].x, frameHeight - padY);
  ctx.closePath();
  ctx.fillStyle = gradient;
  ctx.fill();

  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (const point of points.slice(1)) ctx.lineTo(point.x, point.y);
  ctx.strokeStyle = stroke;
  ctx.lineWidth = 1.8;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  ctx.stroke();
}

const ceiling = computed(() => {
  const dataMax = Math.max(...props.values, 0);
  return props.max ?? (dataMax > 0 ? dataMax * 1.12 : 1);
});

const points = computed(() => {
  const values = props.values;
  if (values.length < 2) return [];
  const step = (width - padX * 2) / (values.length - 1);
  return values.map((value, index) => ({
    x: padX + index * step,
    y: props.height - padY - (Math.max(0, value) / ceiling.value) * (props.height - padY * 2),
  }));
});

const linePath = computed(() =>
  points.value.map((point, index) => `${index === 0 ? 'M' : 'L'}${point.x.toFixed(2)} ${point.y.toFixed(2)}`).join(' '),
);

const areaPath = computed(() => {
  if (!points.value.length) return '';
  const first = points.value[0];
  const last = points.value[points.value.length - 1];
  return `${linePath.value} L${last.x.toFixed(2)} ${props.height - padY} L${first.x.toFixed(2)} ${props.height - padY} Z`;
});

const stroke = computed(() => props.color ?? 'var(--lg-primary)');
</script>

<template>
  <LgCanvasSurface
    v-if="canvas"
    class="lg-sparkline lg-sparkline--canvas"
    :draw="draw"
    :height="height"
    :redraw-key="[values, color, height]"
    :aria-label="ariaLabel"
    role="img"
  />
  <svg
    v-else
    class="lg-sparkline"
    :style="{ height: `${height}px` }"
    :viewBox="`0 0 ${width} ${height}`"
    preserveAspectRatio="none"
    role="img"
    :aria-label="ariaLabel"
  >
    <defs>
      <linearGradient :id="`lg-spark-${uid}`" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" :stop-color="stroke" stop-opacity="0.3" />
        <stop offset="100%" :stop-color="stroke" stop-opacity="0" />
      </linearGradient>
    </defs>
    <path :d="areaPath" :fill="`url(#lg-spark-${uid})`" />
    <path :d="linePath" fill="none" :stroke="stroke" stroke-width="1.8" stroke-linejoin="round" stroke-linecap="round" />
  </svg>
</template>
