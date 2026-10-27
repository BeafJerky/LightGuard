<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import LgCanvasSurface from '../render/CanvasSurface.vue';
import { useRenderBackend } from '../render/useRenderBackend';
import { withAlpha } from '../render/paint/colors';
import type { CanvasFrame } from '../render/types';

interface VirtualTableColumn {
  key: string;
  title: string;
  width?: number;
  align?: 'left' | 'right';
}

type VirtualTableRow = Record<string, string | number>;

const props = withDefaults(
  defineProps<{
    columns: VirtualTableColumn[];
    rows: VirtualTableRow[];
    rowHeight?: number;
    headerHeight?: number;
    height?: number;
    ariaLabel?: string;
  }>(),
  { rowHeight: 30, headerHeight: 32, height: 320, ariaLabel: '数据表格' },
);

const emit = defineEmits<{ select: [index: number, row: VirtualTableRow] }>();

const backend = useRenderBackend('console.tables', () => ({ count: props.rows.length }));
const canvas = computed(() => backend.value === 'canvas');

const root = ref<HTMLDivElement | null>(null);
const scroller = ref<HTMLDivElement | null>(null);
const scrollTop = ref(0);
const selected = ref(0);

const redrawKey = computed(
  () => `${scrollTop.value}|${selected.value}|${props.rows.length}|${props.columns.length}`,
);

const totalContentHeight = computed(
  () => props.headerHeight + props.rows.length * props.rowHeight,
);

function columnStyle(column: VirtualTableColumn) {
  return { flex: `${Math.max(0.2, column.width ?? 1)} 1 0` };
}

function setSelected(index: number) {
  if (!props.rows.length) return;
  const next = Math.min(Math.max(0, index), props.rows.length - 1);
  selected.value = next;
  emit('select', next, props.rows[next]);
  ensureVisible();
}

function ensureVisible() {
  const el = scroller.value;
  if (!el) return;
  const viewHeight = Math.max(1, el.clientHeight - props.headerHeight);
  const top = selected.value * props.rowHeight;
  if (top < el.scrollTop) el.scrollTop = top;
  else if (top + props.rowHeight > el.scrollTop + viewHeight) {
    el.scrollTop = top + props.rowHeight - viewHeight;
  }
  scrollTop.value = el.scrollTop;
}

function onScroll() {
  scrollTop.value = scroller.value?.scrollTop ?? 0;
}

function onKeydown(event: KeyboardEvent) {
  const last = props.rows.length - 1;
  let next = selected.value;
  switch (event.key) {
    case 'ArrowDown':
      next += 1;
      break;
    case 'ArrowUp':
      next -= 1;
      break;
    case 'PageDown':
      next += 10;
      break;
    case 'PageUp':
      next -= 10;
      break;
    case 'Home':
      next = 0;
      break;
    case 'End':
      next = last;
      break;
    default:
      return;
  }
  event.preventDefault();
  setSelected(next);
}

function onClick(event: MouseEvent) {
  const el = scroller.value;
  if (!el) return;
  const y = event.clientY - el.getBoundingClientRect().top - props.headerHeight + el.scrollTop;
  if (y < 0) return;
  setSelected(Math.floor(y / props.rowHeight));
}

const visibleRange = computed(() => {
  const start = Math.max(0, Math.floor(scrollTop.value / props.rowHeight) - 1);
  const end = Math.min(
    props.rows.length,
    Math.ceil((scrollTop.value + props.height) / props.rowHeight) + 1,
  );
  return { start, end };
});

const visibleRows = computed(() =>
  props.rows
    .slice(visibleRange.value.start, visibleRange.value.end)
    .map((row, offset) => ({ row, index: visibleRange.value.start + offset })),
);

const selectionSummary = computed(() => {
  const row = props.rows[selected.value];
  if (!row) return '';
  const text = props.columns.map((column) => `${column.title} ${row[column.key]}`).join('，');
  return `已选择第 ${selected.value + 1} 行，共 ${props.rows.length} 行：${text}`;
});

function fitText(
  ctx: CanvasRenderingContext2D,
  text: string,
  maxWidth: number,
): string {
  if (maxWidth <= 0) return '';
  if (ctx.measureText(text).width <= maxWidth) return text;
  let low = 0;
  let high = text.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (ctx.measureText(`${text.slice(0, middle)}…`).width <= maxWidth) low = middle;
    else high = middle - 1;
  }
  return low > 0 ? `${text.slice(0, low)}…` : '';
}

function draw(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const { width, height, theme } = frame;
  const { rowHeight, headerHeight } = props;
  if (width < 2 || height < 2) return;

  const weights = props.columns.map((column) => Math.max(0.2, column.width ?? 1));
  const weightSum = weights.reduce((sum, weight) => sum + weight, 0) || 1;
  const columnWidths = weights.map((weight) => (width * weight) / weightSum);
  const columnX: number[] = [];
  let cursor = 0;
  for (const columnWidth of columnWidths) {
    columnX.push(cursor);
    cursor += columnWidth;
  }

  ctx.save();
  ctx.beginPath();
  ctx.rect(0, headerHeight, width, height - headerHeight);
  ctx.clip();

  const offset = scrollTop.value;
  const first = Math.max(0, Math.floor(offset / rowHeight));
  ctx.font = `12px ${theme.font.sans}`;
  ctx.textBaseline = 'middle';

  for (let index = first; index < props.rows.length; index += 1) {
    const y = headerHeight + index * rowHeight - offset;
    if (y >= height) break;
    const row = props.rows[index];
    if (!row) continue;

    if (index === selected.value) {
      ctx.fillStyle = withAlpha(theme.color.primary, 0.16);
      ctx.fillRect(0, y, width, rowHeight);
      ctx.fillStyle = theme.color.primary;
      ctx.fillRect(0, y, 3, rowHeight);
    } else if (index % 2 === 1) {
      ctx.fillStyle = withAlpha(theme.color.surface, 0.35);
      ctx.fillRect(0, y, width, rowHeight);
    }

    props.columns.forEach((column, columnIndex) => {
      const align = column.align ?? 'left';
      ctx.fillStyle = index === selected.value ? theme.color.text : theme.color.textSecondary;
      ctx.font = `${align === 'right' ? `12px ${theme.font.mono}` : `12px ${theme.font.sans}`}`;
      ctx.textAlign = align;
      const x = align === 'right'
        ? columnX[columnIndex] + columnWidths[columnIndex] - 12
        : columnX[columnIndex] + 12;
      const text = fitText(ctx, String(row[column.key] ?? ''), columnWidths[columnIndex] - 20);
      ctx.fillText(text, x, y + rowHeight / 2);
    });
  }
  ctx.restore();

  ctx.fillStyle = theme.color.surface2;
  ctx.fillRect(0, 0, width, headerHeight);
  ctx.strokeStyle = theme.color.border;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, headerHeight - 0.5);
  ctx.lineTo(width, headerHeight - 0.5);
  ctx.stroke();

  ctx.font = `11px ${theme.font.sans}`;
  ctx.fillStyle = theme.color.textMuted;
  ctx.textBaseline = 'middle';
  props.columns.forEach((column, columnIndex) => {
    const align = column.align ?? 'left';
    ctx.textAlign = align;
    const x = align === 'right'
      ? columnX[columnIndex] + columnWidths[columnIndex] - 12
      : columnX[columnIndex] + 12;
    const text = fitText(ctx, column.title, columnWidths[columnIndex] - 20);
    ctx.fillText(text, x, headerHeight / 2);
  });
}

watch(
  () => props.rows.length,
  () => {
    selected.value = 0;
    scrollTop.value = 0;
    if (scroller.value) scroller.value.scrollTop = 0;
  },
);

// 切换后端会重建滚动容器，旧 scrollTop 不再适用。
watch(canvas, async () => {
  scrollTop.value = 0;
  await nextTick();
  if (scroller.value) scroller.value.scrollTop = 0;
});
</script>

<template>
  <div
    ref="root"
    class="lg-virtual-table"
    :class="{ 'is-canvas': canvas }"
    :style="{ height: `${height}px` }"
    role="grid"
    tabindex="0"
    :aria-label="ariaLabel"
    :aria-rowcount="rows.length + 1"
    :aria-colcount="columns.length"
    @keydown="onKeydown"
  >
    <template v-if="canvas">
      <LgCanvasSurface
        class="lg-virtual-table__canvas"
        :draw="draw"
        :redraw-key="redrawKey"
        aria-hidden="true"
      />
      <div
        ref="scroller"
        class="lg-virtual-table__scroller"
        @scroll="onScroll"
        @click="onClick"
      >
        <div :style="{ height: `${totalContentHeight}px` }" />
      </div>
    </template>

    <div
      v-else
      ref="scroller"
      class="lg-virtual-table__scroller"
      @scroll="onScroll"
      @click="onClick"
    >
      <div class="lg-virtual-table__head" role="row">
        <span
          v-for="column in columns"
          :key="column.key"
          class="lg-virtual-table__cell"
          role="columnheader"
          :style="[columnStyle(column), { textAlign: column.align ?? 'left' }]"
        >{{ column.title }}</span>
      </div>
      <div class="lg-virtual-table__body" :style="{ height: `${rows.length * rowHeight}px` }">
        <div
          v-for="entry in visibleRows"
          :key="entry.index"
          class="lg-virtual-table__row"
          :class="{ 'is-selected': entry.index === selected }"
          role="row"
          :aria-selected="entry.index === selected"
          :aria-rowindex="entry.index + 2"
          :style="{ top: `${entry.index * rowHeight}px`, height: `${rowHeight}px` }"
        >
          <span
            v-for="column in columns"
            :key="column.key"
            class="lg-virtual-table__cell"
            role="gridcell"
            :style="[columnStyle(column), { textAlign: column.align ?? 'left' }]"
          >{{ entry.row[column.key] }}</span>
        </div>
      </div>
    </div>
    <p class="lg-virtual-table__sr" aria-live="polite">{{ selectionSummary }}</p>
  </div>
</template>

<style scoped>
.lg-virtual-table {
  position: relative;
  overflow: hidden;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  outline: none;
}

.lg-virtual-table:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.lg-virtual-table__canvas {
  position: absolute;
  inset: 0;
  pointer-events: none;
}

.lg-virtual-table__scroller {
  position: absolute;
  inset: 0;
  overflow: auto;
  overscroll-behavior: contain;
}

.lg-virtual-table.is-canvas .lg-virtual-table__scroller {
  scrollbar-color: var(--lg-border-strong) transparent;
}

.lg-virtual-table__head {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  height: 32px;
  background: var(--lg-surface-2);
  border-bottom: var(--lg-border-width) solid var(--lg-border);
}

.lg-virtual-table__body {
  position: relative;
}

.lg-virtual-table__row {
  position: absolute;
  left: 0;
  right: 0;
  display: flex;
  align-items: center;
  border-left: 3px solid transparent;
}

.lg-virtual-table__row:nth-child(even) {
  background: color-mix(in srgb, var(--lg-surface) 60%, transparent);
}

.lg-virtual-table__row.is-selected {
  border-left-color: var(--lg-primary);
  background: var(--lg-primary-soft);
}

.lg-virtual-table__cell {
  min-width: 0;
  padding: 0 12px;
  overflow: hidden;
  font-size: 12px;
  line-height: 32px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.lg-virtual-table__head .lg-virtual-table__cell {
  color: var(--lg-text-muted);
  font-size: 11px;
}

.lg-virtual-table__row .lg-virtual-table__cell[style*='right'] {
  font-family: var(--lg-font-mono);
}

.lg-virtual-table__sr {
  position: absolute;
  width: 1px;
  height: 1px;
  margin: 0;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
</style>
