<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { FileEntry } from '../../ssh/backend';
import { fileIcon, fileTypeLabel, formatSize, formatTime } from './format';
import {
  moveSelection,
  type MoveAction,
  type SortDirection,
  type SortKey,
  type ViewMode,
} from './listModel';

const props = defineProps<{
  entries: FileEntry[];
  selected: string;
  loading: boolean;
  error: string;
  truncated: boolean;
  sortKey: SortKey;
  sortDir: SortDirection;
  offline: boolean;
  mode: ViewMode;
  /** Changes on directory navigation without remounting the list. */
  resetKey: string;
}>();

const emit = defineEmits<{
  select: [entry: FileEntry];
  open: [entry: FileEntry];
  sort: [key: SortKey];
  back: [];
  refresh: [];
  context: [payload: { entry: FileEntry; x: number; y: number }];
}>();

const DETAIL_ROW_HEIGHT = 34;
const LIST_ROW_HEIGHT = 32;
const GRID_ROW_HEIGHT = 104;
const GRID_CELL_MIN = 96;
const GRID_GAP = 8;
const OVERSCAN = 6;

const viewport = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);
const viewportWidth = ref(0);

const rowHeight = computed(() =>
  props.mode === 'grid'
    ? GRID_ROW_HEIGHT
    : props.mode === 'details'
      ? DETAIL_ROW_HEIGHT
      : LIST_ROW_HEIGHT,
);
const gridColumns = computed(() =>
  Math.max(1, Math.floor((viewportWidth.value + GRID_GAP) / (GRID_CELL_MIN + GRID_GAP))),
);
const columns = computed(() => (props.mode === 'grid' ? gridColumns.value : 1));
const rowCount = computed(() => Math.ceil(props.entries.length / columns.value));
const startRow = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / rowHeight.value) - OVERSCAN),
);
const endRow = computed(() =>
  Math.min(
    rowCount.value,
    Math.ceil((scrollTop.value + viewportHeight.value) / rowHeight.value) + OVERSCAN,
  ),
);
const rows = computed(() => {
  const from = startRow.value * columns.value;
  return props.entries
    .slice(from, endRow.value * columns.value)
    .map((entry, offset) => ({ entry, index: from + offset }));
});
const gridRows = computed(() => {
  const cols = gridColumns.value;
  const result: { row: number; top: number; cells: { entry: FileEntry; index: number }[] }[] = [];
  for (let row = startRow.value; row < endRow.value; row += 1) {
    const from = row * cols;
    result.push({
      row,
      top: row * rowHeight.value,
      cells: props.entries
        .slice(from, from + cols)
        .map((entry, offset) => ({ entry, index: from + offset })),
    });
  }
  return result;
});

const headers = [
  { key: 'name' as SortKey, label: '名称', className: 'is-name' },
  { key: 'size' as SortKey, label: '大小', className: 'is-size' },
  { key: 'mtime' as SortKey, label: '修改时间', className: 'is-time' },
  { key: 'type' as SortKey, label: '类型', className: 'is-type' },
  { key: 'mode' as SortKey, label: '权限', className: 'is-mode' },
  { key: 'owner' as SortKey, label: '所有者', className: 'is-owner' },
];

function updateMetrics() {
  const element = viewport.value;
  viewportHeight.value = element?.clientHeight ?? 0;
  viewportWidth.value = element?.clientWidth ?? 0;
}

function onScroll() {
  scrollTop.value = viewport.value?.scrollTop ?? 0;
}

function scrollTo(index: number) {
  const element = viewport.value;
  if (!element) return;
  const top = Math.floor(index / columns.value) * rowHeight.value;
  if (top < element.scrollTop) element.scrollTop = top;
  else if (top + rowHeight.value > element.scrollTop + element.clientHeight) {
    element.scrollTop = top + rowHeight.value - element.clientHeight;
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter') {
    const entry = props.entries.find((item) => item.path === props.selected);
    if (entry) {
      event.preventDefault();
      emit('open', entry);
    }
    return;
  }
  if (event.key === 'Backspace') {
    event.preventDefault();
    emit('back');
    return;
  }
  if (event.key === 'F5') {
    event.preventDefault();
    emit('refresh');
    return;
  }
  const action = {
    ArrowUp: 'up',
    ArrowDown: 'down',
    ArrowLeft: 'left',
    ArrowRight: 'right',
    PageUp: 'page-up',
    PageDown: 'page-down',
    Home: 'home',
    End: 'end',
  }[event.key] as MoveAction | undefined;
  if (!action) return;
  // Horizontal movement only exists in the icon grid; elsewhere the browser
  // keeps its default caret behavior.
  if ((action === 'left' || action === 'right') && props.mode !== 'grid') return;
  event.preventDefault();
  const pageRows = Math.max(1, Math.floor(viewportHeight.value / rowHeight.value) - 1);
  const next = moveSelection(
    props.entries,
    props.selected,
    action,
    pageRows * columns.value,
    columns.value,
  );
  const entry = props.entries.find((item) => item.path === next);
  if (entry) {
    emit('select', entry);
    scrollTo(props.entries.indexOf(entry));
  }
}

function owner(entry: FileEntry) {
  if (entry.uid === null && entry.gid === null) return '—';
  return `${entry.uid ?? '—'}:${entry.gid ?? '—'}`;
}

let observer: ResizeObserver | null = null;

onMounted(() => {
  updateMetrics();
  if (typeof ResizeObserver !== 'undefined' && viewport.value) {
    observer = new ResizeObserver(updateMetrics);
    observer.observe(viewport.value);
  }
  window.addEventListener('resize', updateMetrics);
});
onBeforeUnmount(() => {
  observer?.disconnect();
  window.removeEventListener('resize', updateMetrics);
});
watch(
  () => props.entries.length,
  () => {
    const max = Math.max(0, rowCount.value * rowHeight.value - viewportHeight.value);
    if (scrollTop.value > max) {
      scrollTop.value = 0;
      if (viewport.value) viewport.value.scrollTop = 0;
    }
  },
);
watch(
  () => props.mode,
  async () => {
    await nextTick();
    updateMetrics();
    const element = viewport.value;
    const max = Math.max(0, rowCount.value * rowHeight.value - viewportHeight.value);
    scrollTop.value = element ? Math.min(element.scrollTop, max) : 0;
    if (element && element.scrollTop > max) element.scrollTop = max;
  },
);
// Navigation resets the viewport in place and keeps it focused: remounting the
// component (the old `:key`) dropped the tabindex="0" viewport and killed
// Backspace/arrow/F5 for the rest of the session.
watch(() => props.resetKey, async () => {
  scrollTop.value = 0;
  await nextTick();
  const element = viewport.value;
  if (!element) return;
  element.scrollTop = 0;
  element.focus();
});
</script>

<template>
  <div class="lgfiles-list" :class="[`is-${mode}`, { 'is-offline': offline }]">
    <div v-if="mode === 'details'" class="lgfiles-list__head" role="row">
      <button
        v-for="column in headers"
        :key="column.key"
        type="button"
        class="lgfiles-list__cell"
        :class="[column.className, { 'is-sorted': sortKey === column.key }]"
        :data-sort="column.key"
        @click="emit('sort', column.key)"
      >
        {{ column.label }}
        <span v-if="sortKey === column.key" class="lgfiles-list__arrow">{{ sortDir === 'asc' ? '↑' : '↓' }}</span>
      </button>
    </div>

    <div
      ref="viewport"
      class="lgfiles-list__viewport"
      tabindex="0"
      role="listbox"
      aria-label="文件列表"
      @scroll="onScroll"
      @keydown="onKeydown"
    >
      <div v-if="loading" class="lgfiles-list__skeleton">
        <i v-for="index in 8" :key="index" />
      </div>
      <div v-else-if="error" class="lgfiles-list__state">
        <strong>无法读取此目录</strong>
        <span>{{ error }}</span>
        <button type="button" @click="emit('refresh')">重试</button>
      </div>
      <div v-else-if="entries.length === 0" class="lgfiles-list__state">
        <strong>此文件夹为空</strong>
        <span>没有可见的文件或文件夹。</span>
      </div>
      <div
        v-else-if="mode === 'grid'"
        class="lgfiles-list__canvas"
        :style="{ height: `${rowCount * rowHeight}px` }"
      >
        <div
          v-for="row in gridRows"
          :key="row.row"
          class="lgfiles-grid__row"
          :style="{
            top: `${row.top}px`,
            height: `${rowHeight}px`,
            gridTemplateColumns: `repeat(${gridColumns}, minmax(0, 1fr))`,
          }"
        >
          <div
            v-for="cell in row.cells"
            :key="cell.entry.path"
            class="lgfiles-grid__cell"
            :class="{ 'is-selected': selected === cell.entry.path }"
            :data-path="cell.entry.path"
            role="option"
            :aria-selected="selected === cell.entry.path"
            :title="cell.entry.name"
            @click="emit('select', cell.entry)"
            @dblclick="emit('open', cell.entry)"
            @contextmenu.prevent="emit('context', { entry: cell.entry, x: $event.clientX, y: $event.clientY })"
          >
            <LgIcon :name="fileIcon(cell.entry)" :size="30" />
            <span class="lgfiles-grid__name">{{ cell.entry.name }}</span>
            <span class="lgfiles-grid__meta">{{ cell.entry.kind === 'dir' ? '' : formatSize(cell.entry.size) }}</span>
          </div>
        </div>
      </div>
      <div v-else class="lgfiles-list__canvas" :style="{ height: `${rowCount * rowHeight}px` }">
        <div
          v-for="row in rows"
          :key="row.entry.path"
          class="lgfiles-list__row"
          :class="{ 'is-selected': selected === row.entry.path, 'is-compact': mode === 'list' }"
          :style="{ top: `${row.index * rowHeight}px`, height: `${rowHeight}px` }"
          :data-path="row.entry.path"
          role="option"
          :aria-selected="selected === row.entry.path"
          @click="emit('select', row.entry)"
          @dblclick="emit('open', row.entry)"
          @contextmenu.prevent="emit('context', { entry: row.entry, x: $event.clientX, y: $event.clientY })"
        >
          <span class="lgfiles-list__cell is-name">
            <LgIcon :name="fileIcon(row.entry)" :size="16" />
            <span class="lgfiles-list__name">{{ row.entry.name }}</span>
          </span>
          <span v-if="mode === 'list'" class="lgfiles-list__cell is-size">
            {{ row.entry.kind === 'dir' ? '—' : formatSize(row.entry.size) }}
          </span>
          <template v-else>
            <span class="lgfiles-list__cell is-size">{{ row.entry.kind === 'dir' ? '—' : formatSize(row.entry.size) }}</span>
            <span class="lgfiles-list__cell is-time">{{ formatTime(row.entry.mtime) }}</span>
            <span class="lgfiles-list__cell is-type">{{ fileTypeLabel(row.entry) }}</span>
            <span class="lgfiles-list__cell is-mode">{{ row.entry.modeText }}</span>
            <span class="lgfiles-list__cell is-owner">{{ owner(row.entry) }}</span>
          </template>
        </div>
      </div>
    </div>

    <p v-if="truncated" class="lgfiles-list__truncated">目录项过多，仅显示前 20,000 项</p>
  </div>
</template>
