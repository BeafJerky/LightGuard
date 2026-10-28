<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../../types';
import type { FileEntry } from '../../ssh/backend';
import FileList from './FileList.vue';
import FilePreview from './FilePreview.vue';
import FileTree from './FileTree.vue';
import { useFiles } from './useFiles';
import './files.css';

const props = defineProps<{ server: Server }>();
const files = useFiles(props.server);

const address = ref('');
const drawerOpen = ref(false);
const menuOpen = ref(false);
const context = ref<{ entry: FileEntry; x: number; y: number } | null>(null);
const addressInput = ref<HTMLInputElement | null>(null);

const folderCount = computed(() =>
  files.visibleRows.value.filter((entry) => (entry.targetKind ?? entry.kind) === 'dir').length,
);
const selectedIndex = computed(() =>
  files.visibleRows.value.findIndex((entry) => entry.path === files.selectedPath.value),
);
const statusText = computed(() => {
  const total = files.visibleRows.value.length;
  const parts = [`共 ${total} 项`, `${folderCount.value} 个文件夹`];
  if (selectedIndex.value >= 0) {
    parts.push(`已选 1 项`);
    const entry = files.visibleRows.value[selectedIndex.value];
    if (entry.kind !== 'dir') parts.push(`大小 ${entry.size} B`);
  }
  return parts.join(' · ');
});
const activeDownloads = computed(() =>
  files.downloads.value.filter((task) => task.status === 'waiting' || task.status === 'running'),
);
// 「连接已断开」只在曾经建立过会话时才有意义；从未连接时提示先连接。
const sessionLost = computed(() => Boolean(props.server.sessionId) && files.offline.value);

watch(
  () => files.currentPath.value,
  (path) => {
    address.value = path;
    drawerOpen.value = false;
    context.value = null;
    menuOpen.value = false;
  },
  { immediate: true },
);

function submitAddress() {
  const target = address.value.trim();
  if (target) void files.navigate(target);
  else address.value = files.currentPath.value;
}

function resetAddress() {
  address.value = files.currentPath.value;
}

function focusAddress() {
  addressInput.value?.focus();
  addressInput.value?.select();
}

function openContext(payload: { entry: FileEntry; x: number; y: number }) {
  files.select(payload.entry);
  context.value = payload;
}

async function contextAction(action: 'open' | 'preview' | 'download' | 'copy' | 'meta') {
  const entry = context.value?.entry;
  context.value = null;
  if (!entry) return;
  if (action === 'open') await files.open(entry);
  else if (action === 'preview') {
    files.select(entry);
    await files.loadPreview(entry);
  } else if (action === 'download') await files.download(entry);
  else if (action === 'copy') {
    try { await navigator.clipboard.writeText(entry.path); } catch { files.notice.value = '复制路径失败'; }
  } else {
    files.select(entry);
  }
}

function onGlobalKey(event: KeyboardEvent) {
  if (event.ctrlKey && event.key.toLowerCase() === 'l') {
    event.preventDefault();
    focusAddress();
  } else if (event.altKey && event.key === 'ArrowLeft') {
    event.preventDefault();
    void files.go(-1);
  } else if (event.altKey && event.key === 'ArrowRight') {
    event.preventDefault();
    void files.go(1);
  } else if (event.key === 'Escape') {
    context.value = null;
    menuOpen.value = false;
    if (files.previewOpen.value) files.closePreview();
  }
}

onMounted(() => {
  void files.boot();
  window.addEventListener('keydown', onGlobalKey);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKey);
});

watch(
  () => props.server.sessionId,
  (session) => {
    if (session && files.canUse.value && !files.currentPath.value) void files.boot();
  },
);
</script>

<template>
  <div class="lgfiles">
    <div v-if="!files.canUse.value" class="lgfiles-blank">
      <LgIcon name="folder" :size="30" />
      <strong>{{ sessionLost ? '连接已断开' : '请先连接服务器' }}</strong>
      <span>{{ sessionLost ? '重新连接后打开文件应用，或回到桌面重试。' : '文件浏览依赖现有 SSH 会话。' }}</span>
    </div>

    <template v-else>
      <div class="lgfiles-toolbar">
        <button type="button" class="lgfiles-icon-btn" aria-label="后退" :disabled="!files.canBack.value" @click="files.go(-1)">
          <LgIcon name="chevron-left" :size="15" />
        </button>
        <button type="button" class="lgfiles-icon-btn" aria-label="前进" :disabled="!files.canForward.value" @click="files.go(1)">
          <LgIcon name="chevron-right" :size="15" />
        </button>
        <button type="button" class="lgfiles-icon-btn" aria-label="上级" :disabled="!files.canUp.value" @click="files.goUp()">
          <LgIcon name="arrow-up" :size="15" />
        </button>
        <button type="button" class="lgfiles-icon-btn" aria-label="刷新" @click="files.refresh()">
          <LgIcon name="refresh" :size="15" />
        </button>
        <form class="lgfiles-address" @submit.prevent="submitAddress">
          <input
            ref="addressInput"
            v-model="address"
            aria-label="当前路径"
            spellcheck="false"
            autocomplete="off"
            @keydown.esc="resetAddress"
          />
        </form>
        <label class="lgfiles-search">
          <LgIcon name="search" :size="13" />
          <input v-model="files.view.query" aria-label="筛选当前目录" placeholder="筛选当前目录" />
        </label>
        <button
          type="button"
          class="lgfiles-icon-btn"
          :class="{ 'is-on': files.previewOpen.value }"
          aria-label="预览"
          @click="files.togglePreview()"
        >
          <LgIcon name="eye" :size="15" />
        </button>
        <button type="button" class="lgfiles-icon-btn lgfiles-only-narrow" aria-label="目录树" @click="drawerOpen = !drawerOpen">
          <LgIcon name="menu" :size="15" />
        </button>
        <button type="button" class="lgfiles-icon-btn" aria-label="更多" @click="menuOpen = !menuOpen">
          <LgIcon name="more" :size="15" />
        </button>
        <div v-if="menuOpen" class="lgfiles-menu">
          <button type="button" @click="files.setShowHidden(!files.view.showHidden); menuOpen = false">
            {{ files.view.showHidden ? '隐藏隐藏文件' : '显示隐藏文件' }}
          </button>
          <button type="button" @click="files.refresh(); menuOpen = false">清空缓存并刷新</button>
        </div>
      </div>

      <div class="lgfiles-body">
        <aside class="lgfiles-tree" :class="{ 'is-open': drawerOpen }" aria-label="目录树">
          <FileTree
            :nodes="files.roots.value"
            :active-path="files.currentPath.value"
            @toggle="files.toggleNode"
            @select="files.selectTree"
            @retry="files.retryNode"
            @open="files.openTree"
          />
        </aside>

        <section class="lgfiles-main">
          <FileList
            :key="files.currentPath.value"
            :entries="files.visibleRows.value"
            :selected="files.selectedPath.value"
            :loading="files.loading.value"
            :error="files.error.value?.message ?? ''"
            :truncated="files.truncated.value"
            :sort-key="files.view.sortKey"
            :sort-dir="files.view.sortDir"
            :offline="files.offline.value"
            @select="files.select"
            @open="files.open"
            @sort="files.toggleSort"
            @back="files.goUp()"
            @refresh="files.refresh()"
            @context="openContext"
          />
          <footer class="lgfiles-status">
            <span>{{ statusText }}</span>
            <span v-if="activeDownloads.length" class="lgfiles-status__transfer">
              <LgIcon name="download" :size="12" />
              {{ activeDownloads.length }} 个传输中
            </span>
          </footer>
        </section>

        <FilePreview
          v-if="files.previewOpen.value"
          :preview="files.preview"
          :entry="files.selectedEntry.value"
          @close="files.closePreview"
          @retry="files.preview.entry && files.loadPreview(files.preview.entry)"
          @download="files.selectedEntry.value && files.download(files.selectedEntry.value)"
        />
      </div>

      <div v-if="context" class="lgfiles-context" :style="{ left: `${context.x}px`, top: `${context.y}px` }">
        <button type="button" @click="contextAction('open')">打开</button>
        <button type="button" @click="contextAction('preview')">预览</button>
        <button type="button" @click="contextAction('download')">
          {{ (context.entry.targetKind ?? context.entry.kind) === 'dir' ? '下载文件夹' : '下载' }}
        </button>
        <button type="button" @click="contextAction('copy')">复制路径</button>
        <button type="button" @click="contextAction('meta')">属性</button>
        <button type="button" @click="files.refresh(); context = null">刷新</button>
      </div>

      <div v-if="files.downloads.value.length" class="lgfiles-downloads">
        <article v-for="task in files.downloads.value" :key="task.id" :class="`is-${task.status}`">
          <div class="lgfiles-downloads__text">
            <strong>{{ task.name }}</strong>
            <span>{{ task.message }}</span>
          </div>
          <div class="lgfiles-downloads__bar">
            <i :style="{ width: `${task.total ? Math.min(100, (task.transferred / task.total) * 100) : 8}%` }" />
          </div>
          <button
            v-if="task.status === 'waiting' || task.status === 'running'"
            type="button"
            @click="files.cancelDownload(task.id)"
          >
            取消
          </button>
          <button v-else type="button" @click="files.removeDownload(task.id)">关闭</button>
        </article>
      </div>

      <div v-if="files.offline.value" class="lgfiles-offline">
        <strong>连接已断开</strong>
        <span>当前列表为最后一次结果，重新连接后点击刷新。</span>
      </div>

      <p v-if="files.notice.value" class="lgfiles-notice">{{ files.notice.value }}</p>
    </template>
  </div>
</template>
