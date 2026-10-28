<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { FileEntry } from '../../ssh/backend';
import { fileIcon, fileTypeLabel, formatSize, formatTime } from './format';
import { moveSelection, type SortDirection, type SortKey } from './listModel';

const props = defineProps<{
  entries: FileEntry[];
  selected: string;
  loading: boolean;
  error: string;
  truncated: boolean;
  sortKey: SortKey;
  sortDir: SortDirection;
  offline: boolean;
}>();

const emit = defineEmits<{
  select: [entry: FileEntry];
  open: [entry: FileEntry];
  sort: [key: SortKey];
  back: [];
  refresh: [];
  context: [payload: { entry: FileEntry; x: number; y: number }];
}>();

const ROW_HEIGHT = 30;
const OVERSCAN = 6;
const viewport = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const viewportHeight = ref(0);

const startIndex = computed(() =>
  Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - OVERSCAN),
);
const endIndex = computed(() =>
  Math.min(
    props.entries.length,
    Math.ceil((scrollTop.value + viewportHeight.value) / ROW_HEIGHT) + OVERSCAN,
  ),
);
const rows = computed(() =>
  props.entries.slice(startIndex.value, endIndex.value).map((entry, offset) => ({
    entry,
    index: startIndex.value + offset,
  })),
);

const columns = [
  { key: 'name' as SortKey, label: '名称', className: 'is-name' },
  { key: 'size' as SortKey, label: '大小', className: 'is-size' },
  { key: 'mtime' as SortKey, label: '修改时间', className: 'is-time' },
  { key: 'type' as SortKey, label: '类型', className: 'is-type' },
  { key: 'mode' as SortKey, label: '权限', className: 'is-mode' },
  { key: 'owner' as SortKey, label: '所有者', className: 'is-owner' },
];

function updateHeight() {
  viewportHeight.value = viewport.value?.clientHeight ?? 0;
}

function onScroll() {
  scrollTop.value = viewport.value?.scrollTop ?? 0;
}

function scrollTo(index: number) {
  const element = viewport.value;
  if (!element) return;
  const top = index * ROW_HEIGHT;
  if (top < element.scrollTop) element.scrollTop = top;
  else if (top + ROW_HEIGHT > element.scrollTop + element.clientHeight) {
    element.scrollTop = top + ROW_HEIGHT - element.clientHeight;
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
    PageUp: 'page-up',
    PageDown: 'page-down',
    Home: 'home',
    End: 'end',
  }[event.key] as Parameters<typeof moveSelection>[2] | undefined;
  if (!action) return;
  event.preventDefault();
  const page = Math.max(1, Math.floor(viewportHeight.value / ROW_HEIGHT) - 1);
  const next = moveSelection(props.entries, props.selected, action, page);
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

onMounted(() => {
  updateHeight();
  window.addEventListener('resize', updateHeight);
});
onBeforeUnmount(() => window.removeEventListener('resize', updateHeight));
watch(() => props.entries.length, () => {
  if (scrollTop.value > props.entries.length * ROW_HEIGHT) {
    scrollTop.value = 0;
    if (viewport.value) viewport.value.scrollTop = 0;
  }
});
</script>

<template>
  <div class="lgfiles-list" :class="{ 'is-offline': offline }">
    <div class="lgfiles-list__head" role="row">
      <button
        v-for="column in columns"
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
      <div v-else class="lgfiles-list__canvas" :style="{ height: `${entries.length * ROW_HEIGHT}px` }">
        <div
          v-for="row in rows"
          :key="row.entry.path"
          class="lgfiles-list__row"
          :class="{ 'is-selected': selected === row.entry.path }"
          :style="{ top: `${row.index * ROW_HEIGHT}px` }"
          :data-path="row.entry.path"
          role="option"
          :aria-selected="selected === row.entry.path"
          @click="emit('select', row.entry)"
          @dblclick="emit('open', row.entry)"
          @contextmenu.prevent="emit('context', { entry: row.entry, x: $event.clientX, y: $event.clientY })"
        >
          <span class="lgfiles-list__cell is-name">
            <LgIcon :name="fileIcon(row.entry)" :size="14" />
            <span class="lgfiles-list__name">{{ row.entry.name }}</span>
          </span>
          <span class="lgfiles-list__cell is-size">{{ row.entry.kind === 'dir' ? '—' : formatSize(row.entry.size) }}</span>
          <span class="lgfiles-list__cell is-time">{{ formatTime(row.entry.mtime) }}</span>
          <span class="lgfiles-list__cell is-type">{{ fileTypeLabel(row.entry) }}</span>
          <span class="lgfiles-list__cell is-mode">{{ row.entry.modeText }}</span>
          <span class="lgfiles-list__cell is-owner">{{ owner(row.entry) }}</span>
        </div>
      </div>
    </div>

    <p v-if="truncated" class="lgfiles-list__truncated">目录项过多，仅显示前 20,000 项</p>
  </div>
</template>
