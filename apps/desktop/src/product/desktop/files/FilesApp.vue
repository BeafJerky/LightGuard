<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import type { Server } from '../../types';
import FileTabBar from './FileTabBar.vue';
import FilesView from './FilesView.vue';
import { useFileTabs, type FileTab } from './useFileTabs';
import './files.css';

const props = defineProps<{ server: Server }>();
const emit = defineEmits<{ 'close-window': [] }>();

const tabs = useFileTabs(props.server);
const rootRef = ref<HTMLElement | null>(null);
const barTarget = ref<HTMLElement | null>(null);

function tabTitle(tab: FileTab): string {
  const path = tab.files.currentPath.value;
  if (!path) return '加载中';
  if (path === tab.files.homePath.value) return '主目录';
  if (path === '/') return '根目录';
  return path.split('/').filter(Boolean).pop() || path;
}

const tabViews = computed(() =>
  tabs.tabs.value.map((tab) => ({
    key: tab.key,
    title: tabTitle(tab),
    path: tab.files.currentPath.value || '主目录',
  })),
);

function onCreate() {
  tabs.createTab(tabs.active.value?.files.currentPath.value || undefined);
}

function focusActiveView() {
  void nextTick(() => {
    const view = rootRef.value?.querySelector<HTMLElement>(
      '.lgfiles-view.is-active .lgfiles-list__viewport',
    );
    if (view) view.focus();
    else rootRef.value?.closest<HTMLElement>('.lgdesk-win')?.focus();
  });
}

function onCloseTab(key: string) {
  const wasActive = key === tabs.activeKey.value;
  tabs.closeTab(key);
  if (wasActive) focusActiveView();
}

function onActivateTab(key: string) {
  tabs.activate(key);
  focusActiveView();
}

function onGlobalKey(event: KeyboardEvent) {
  if (event.defaultPrevented) return;
  const win = rootRef.value?.closest('.lgdesk-win');
  if (win && event.target instanceof Node && !win.contains(event.target)) return;
  if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 't') {
    event.preventDefault();
    onCreate();
  } else if (event.ctrlKey && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'w') {
    event.preventDefault();
    if (tabs.active.value) onCloseTab(tabs.active.value.key);
  } else if (event.ctrlKey && event.key === 'Tab') {
    event.preventDefault();
    tabs.cycle(event.shiftKey ? -1 : 1);
    focusActiveView();
  } else if (event.ctrlKey && !event.shiftKey && /^[1-8]$/.test(event.key)) {
    const tab = tabs.tabs.value[Number(event.key) - 1];
    if (tab) {
      event.preventDefault();
      onActivateTab(tab.key);
    }
  }
}

watch(
  () => tabs.closed.value,
  (closed) => {
    if (closed) emit('close-window');
  },
);

onMounted(() => {
  const win = rootRef.value?.closest('.lgdesk-win');
  barTarget.value = (win?.querySelector('.lgdesk-win__bar') as HTMLElement | null) ?? null;
  window.addEventListener('keydown', onGlobalKey);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKey);
});
</script>

<template>
  <div ref="rootRef" class="lgfiles">
    <Teleport v-if="barTarget" :to="barTarget">
      <FileTabBar
        :tabs="tabViews"
        :active-key="tabs.activeKey.value"
        :server-label="`文件 · ${server.ip}`"
        @activate="onActivateTab"
        @close="onCloseTab"
        @create="onCreate"
      />
    </Teleport>
    <FileTabBar
      v-else
      class="is-inline"
      :tabs="tabViews"
      :active-key="tabs.activeKey.value"
      :server-label="`文件 · ${server.ip}`"
      @activate="onActivateTab"
      @close="onCloseTab"
      @create="onCreate"
    />

    <div v-if="!tabs.tabs.value.length" class="lgfiles-blank">
      <strong>没有打开的标签</strong>
    </div>

    <FilesView
      v-for="tab in tabs.tabs.value"
      v-show="tab.key === tabs.activeKey.value"
      :key="tab.key"
      :tab="tab"
      :server="server"
      :active="tab.key === tabs.activeKey.value"
      @open-tab="tabs.createTab"
    />
  </div>
</template>
