<script setup lang="ts">
import { computed } from 'vue';
import {
  LgCanvasSurface,
  LgIcon,
  LgTooltip,
  useRenderBackend,
  withAlpha,
} from '@lightguard/lightui';
import type { CanvasFrame } from '@lightguard/lightui';
import { drawIconGlyph } from './iconGlyphs';
import { iconCells, iconLayerSize, type IconLayerVariant } from './iconLayout';

interface DesktopIconItem {
  name: string;
  label: string;
  tile?: string;
}

const props = withDefaults(
  defineProps<{
    items: DesktopIconItem[];
    variant?: IconLayerVariant;
    ariaLabel?: string;
    selectedIndex?: number;
    rows?: number;
  }>(),
  { variant: 'grid', ariaLabel: '桌面图标', selectedIndex: -1, rows: 3 },
);

const emit = defineEmits<{
  select: [index: number];
  activate: [index: number];
}>();

const backend = useRenderBackend('desktop.icons');
const canvas = computed(() => backend.value === 'canvas');
const cells = computed(() => iconCells(props.items.length, props.variant, props.rows));
const size = computed(() => iconLayerSize(props.items.length, props.variant, props.rows));

function px(value: string, fallback: number) {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  if (typeof ctx.roundRect === 'function') ctx.roundRect(x, y, width, height, radius);
  else ctx.rect(x, y, width, height);
}

function draw(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const { theme } = frame;
  const dock = props.variant === 'dock';
  const tile = dock ? 34 : 46;
  const tileColors: Record<string, string> = {
    'tile-about': theme.color.primary,
    'tile-performance': theme.color.accent,
    'tile-disks': theme.color.info,
    'tile-network': theme.color.warning,
    'tile-processes': theme.color.danger,
    'tile-connection': theme.color.success,
    'tile-terminal': theme.color.textSecondary,
    'tile-docker': theme.color.info,
  };

  cells.value.forEach((cell, index) => {
    const item = props.items[index];
    if (!item) return;
    const x = cell.x + (cell.width - tile) / 2;
    const y = cell.y + (dock ? (cell.height - tile) / 2 : 4);
    const radius = dock ? px(theme.radius.sm, 5) : 12;
    const tint = item.tile ? tileColors[item.tile] : null;

    ctx.beginPath();
    roundRect(ctx, x, y, tile, tile, radius);
    if (tint) {
      ctx.fillStyle = theme.color.surface2;
      ctx.fill();
      ctx.fillStyle = withAlpha(tint, 0.34);
      ctx.fill();
      ctx.strokeStyle = withAlpha(tint, 0.42);
    } else {
      ctx.fillStyle = withAlpha(theme.color.surface, dock ? 0.82 : 0.55);
      ctx.fill();
      ctx.strokeStyle = theme.color.border;
    }
    ctx.lineWidth = px(theme.color.borderWidth, 1);
    ctx.stroke();

    ctx.strokeStyle = tint ?? theme.color.primary;
    ctx.lineWidth = px(theme.icon.stroke, 1.7);
    ctx.lineCap = theme.icon.linecap as CanvasLineCap;
    ctx.lineJoin = theme.icon.linejoin as CanvasLineJoin;
    drawIconGlyph(ctx, item.name, x + tile / 2, y + tile / 2, dock ? 18 : 24);

    if (!dock) {
      ctx.fillStyle = theme.desktop.text;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.font = `11px ${theme.font.sans}`;
      ctx.fillText(item.label, cell.x + cell.width / 2, y + tile + 13, cell.width);
    }
  });
}

function cellStyle(index: number) {
  const cell = cells.value[index];
  if (!cell) return {};
  return {
    left: `${cell.x}px`,
    top: `${cell.y}px`,
    width: `${cell.width}px`,
    height: `${cell.height}px`,
  };
}
</script>

<template>
  <div
    class="desktop-icon-layer"
    :class="[`desktop-icon-layer--${variant}`, { 'is-canvas': canvas }]"
    :style="{ width: `${size.width}px`, height: `${size.height}px` }"
    role="group"
    :aria-label="ariaLabel"
  >
    <LgCanvasSurface
      v-if="canvas"
      class="desktop-icon-layer__canvas"
      :draw="draw"
      :redraw-key="[items, variant]"
    />
    <div class="desktop-icon-layer__overlay">
      <template v-for="(item, index) in items" :key="item.name">
        <LgTooltip v-if="variant === 'dock'" placement="top">
          <template #trigger>
            <button
              type="button"
              class="desktop-icon-layer__btn desktop-icon-layer__btn--dock"
              :style="cellStyle(index)"
              :aria-label="item.label"
              @click="emit('select', index)"
            >
              <LgIcon v-if="!canvas" :name="item.name" :size="18" />
              <span v-else class="desktop-icon-layer__sr">{{ item.label }}</span>
            </button>
          </template>
          {{ item.label }}
        </LgTooltip>
        <button
          v-else
          type="button"
          class="desktop-icon-layer__btn desktop-icon-layer__btn--grid"
          :class="{ 'is-selected': selectedIndex === index }"
          :style="cellStyle(index)"
          :aria-label="item.label"
          :aria-pressed="selectedIndex === index"
          :title="item.label"
          @click="emit('select', index)"
          @dblclick="emit('activate', index)"
          @keydown.enter.prevent="emit('activate', index)"
          @keydown.space.prevent="emit('activate', index)"
        >
          <template v-if="!canvas">
            <span class="desktop-icon-layer__glyph" :class="item.tile"><LgIcon :name="item.name" :size="24" /></span>
            <span class="desktop-icon-layer__label">{{ item.label }}</span>
          </template>
          <span v-else class="desktop-icon-layer__sr">{{ item.label }}</span>
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped>
.desktop-icon-layer {
  position: relative;
}

.desktop-icon-layer__canvas {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.desktop-icon-layer__overlay {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.desktop-icon-layer__btn {
  position: absolute;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: flex-start;
  gap: 7px;
  padding: 6px 4px;
  border: 1px solid transparent;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-desktop-text, var(--lg-text));
  font: inherit;
  font-size: 11px;
  line-height: 1.3;
  cursor: pointer;
  pointer-events: auto;
  text-align: center;
  text-shadow: var(--lg-desktop-text-shadow, none);
}

.desktop-icon-layer:not(.is-canvas) .desktop-icon-layer__btn--grid:hover {
  border-color: var(--lg-border);
  background: color-mix(in srgb, var(--lg-surface) 45%, transparent);
}

.desktop-icon-layer:not(.is-canvas) .desktop-icon-layer__btn--grid:focus-visible,
.desktop-icon-layer__btn--grid.is-selected {
  border-color: color-mix(in srgb, var(--lg-primary) 42%, transparent);
  background: color-mix(in srgb, var(--lg-primary) 24%, transparent);
}

.desktop-icon-layer.is-canvas .desktop-icon-layer__btn--grid:hover {
  background: color-mix(in srgb, var(--lg-hover, var(--lg-surface-3)) 32%, transparent);
}

.desktop-icon-layer__btn--dock {
  align-items: center;
  justify-content: center;
  padding: 0;
  border: var(--lg-border-width) solid var(--lg-border);
  background: var(--lg-surface-2);
  color: var(--lg-text-secondary);
}

.desktop-icon-layer:not(.is-canvas) .desktop-icon-layer__btn--dock:hover {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.desktop-icon-layer.is-canvas .desktop-icon-layer__btn--dock {
  border-color: transparent;
  background: transparent;
}

.desktop-icon-layer.is-canvas .desktop-icon-layer__btn--dock:hover,
.desktop-icon-layer.is-canvas .desktop-icon-layer__btn--dock:focus-visible {
  box-shadow: inset 0 0 0 1px var(--lg-primary);
  color: var(--lg-primary);
}

.desktop-icon-layer__btn:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.desktop-icon-layer__glyph {
  display: grid;
  place-items: center;
  width: 46px;
  height: 46px;
  border: 0;
  border-radius: var(--lg-radius-md);
  background: color-mix(in srgb, var(--lg-surface) 60%, transparent);
  color: var(--lg-primary);
}

.desktop-icon-layer__label {
  display: -webkit-box;
  max-width: 82px;
  overflow: hidden;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  word-break: break-word;
}

.desktop-icon-layer__sr {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
