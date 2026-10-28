<script setup lang="ts">
import { LgIcon } from '@lightguard/lightui';
import type { TreeState } from './useFiles';

withDefaults(defineProps<{ nodes: TreeState[]; activePath: string; depth?: number }>(), {
  depth: 0,
});

const emit = defineEmits<{
  toggle: [path: string];
  select: [path: string];
  retry: [path: string];
  open: [path: string];
}>();

function onKeydown(event: KeyboardEvent, path: string, node: TreeState) {
  if (event.key === 'ArrowRight' && node.kind === 'dir' && !node.expanded) {
    event.preventDefault();
    emit('toggle', path);
  } else if (event.key === 'ArrowLeft' && node.expanded) {
    event.preventDefault();
    emit('toggle', path);
  } else if (event.key === 'Enter') {
    event.preventDefault();
    emit('open', path);
  } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault();
    const container = (event.currentTarget as HTMLElement).closest('.lgfiles-tree');
    const rows = Array.from(container?.querySelectorAll<HTMLElement>('[data-tree-path]') ?? []);
    const index = rows.indexOf(event.currentTarget as HTMLElement);
    const next = Math.min(rows.length - 1, Math.max(0, index + (event.key === 'ArrowDown' ? 1 : -1)));
    rows[next]?.focus();
  }
}
</script>

<template>
  <ul class="lgfiles-tree__list">
    <li v-for="node in nodes" :key="node.path">
      <div
        class="lgfiles-tree__row"
        :class="{ 'is-active': activePath === node.path, 'is-link': node.kind === 'symlink' }"
        :style="{ paddingLeft: `${8 + depth * 14}px` }"
        :data-tree-path="node.path"
        tabindex="0"
        role="treeitem"
        :aria-expanded="node.kind === 'dir' ? node.expanded : undefined"
        @click="emit('select', node.path)"
        @dblclick="node.kind === 'dir' ? emit('toggle', node.path) : emit('open', node.path)"
        @keydown="onKeydown($event, node.path, node)"
      >
        <button
          v-if="node.kind === 'dir'"
          type="button"
          class="lgfiles-tree__twist"
          :aria-label="node.expanded ? '收起' : '展开'"
          @click.stop="emit('toggle', node.path)"
        >
          <LgIcon :name="node.expanded ? 'chevron-down' : 'chevron-right'" :size="12" />
        </button>
        <span v-else class="lgfiles-tree__twist" />
        <LgIcon :name="node.kind === 'symlink' ? 'link' : 'folder'" :size="13" />
        <span class="lgfiles-tree__name">{{ node.name }}</span>
        <span v-if="node.loading" class="lgfiles-tree__spin" aria-label="加载中" />
      </div>
      <div v-if="node.error" class="lgfiles-tree__error">
        <span>{{ node.error }}</span>
        <button type="button" @click.stop="emit('retry', node.path)">重试</button>
      </div>
      <FileTree
        v-if="node.expanded && node.children && node.children.length"
        :nodes="node.children"
        :depth="depth + 1"
        :active-path="activePath"
        @toggle="emit('toggle', $event)"
        @select="emit('select', $event)"
        @retry="emit('retry', $event)"
        @open="emit('open', $event)"
      />
      <div
        v-else-if="node.expanded && node.children && node.children.length === 0"
        class="lgfiles-tree__empty"
        :style="{ paddingLeft: `${30 + depth * 14}px` }"
      >
        无子目录
      </div>
    </li>
  </ul>
</template>
