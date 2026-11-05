<script setup lang="ts">
import { nextTick, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';

const props = defineProps<{
  tabs: { key: string; title: string; path: string }[];
  activeKey: string;
  serverLabel?: string;
}>();

const emit = defineEmits<{
  activate: [key: string];
  close: [key: string];
  create: [];
}>();

const stripRef = ref<HTMLElement | null>(null);

// 标签条被固定拖拽区压缩后会横向滚动，激活标签必须留在可视区内。
watch(
  () => [props.activeKey, props.tabs.length],
  () => {
    void nextTick(() => {
      const strip = stripRef.value;
      const active = strip?.querySelector<HTMLElement>('.lgfiles-tab.is-active');
      if (!strip || !active) return;
      const stripRect = strip.getBoundingClientRect();
      const activeRect = active.getBoundingClientRect();
      if (activeRect.left < stripRect.left) {
        strip.scrollLeft -= stripRect.left - activeRect.left;
      } else if (activeRect.right > stripRect.right) {
        strip.scrollLeft += activeRect.right - stripRect.right;
      }
    });
  },
  { immediate: true },
);

function onAuxClick(event: MouseEvent, key: string) {
  if (event.button !== 1) return;
  event.preventDefault();
  emit('close', key);
}
</script>

<template>
  <div class="lgfiles-tabbar" :title="serverLabel">
    <div ref="stripRef" class="lgfiles-tabbar__strip" role="tablist" aria-label="文件标签">
      <div
        v-for="tab in tabs"
        :key="tab.key"
        class="lgfiles-tab"
        :class="{ 'is-active': tab.key === activeKey }"
        role="tab"
        :aria-selected="tab.key === activeKey"
        :tabindex="tab.key === activeKey ? 0 : -1"
        :title="tab.path"
        data-win-drag-ignore
        @click="emit('activate', tab.key)"
        @keydown.enter.prevent="emit('activate', tab.key)"
        @keydown.space.prevent="emit('activate', tab.key)"
        @auxclick="onAuxClick($event, tab.key)"
      >
        <LgIcon name="folder" :size="13" />
        <span class="lgfiles-tab__label">{{ tab.title }}</span>
        <button
          type="button"
          class="lgfiles-tab__close"
          aria-label="关闭标签"
          @click.stop="emit('close', tab.key)"
        >
          <LgIcon name="x" :size="11" />
        </button>
      </div>
    </div>
    <button
      type="button"
      class="lgfiles-tab-add"
      aria-label="新建标签"
      title="新建标签 (Ctrl+T)"
      data-win-drag-ignore
      @click="emit('create')"
    >
      <LgIcon name="plus" :size="14" />
    </button>
    <span class="lgfiles-tabbar__drag" aria-hidden="true" />
  </div>
</template>
