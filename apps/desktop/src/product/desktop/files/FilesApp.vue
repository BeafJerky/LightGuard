<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../../types';
import type { FileEntry, FileKind } from '../../ssh/backend';
import { toSshError } from '../../ssh';
import { openPermitPanel, permitOf } from '../../guard/store';
import { openAudit } from '../../router';
import { toast } from '../../ui';
import FileList from './FileList.vue';
import FilePreview from './FilePreview.vue';
import FileTree from './FileTree.vue';
import NewFolderDialog from './NewFolderDialog.vue';
import { useFiles } from './useFiles';
import './files.css';

const props = defineProps<{ server: Server }>();
const files = useFiles(props.server);

const address = ref('');
const drawerOpen = ref(false);
const menuOpen = ref<'view' | 'more' | ''>('');
const context = ref<{ entry: FileEntry; x: number; y: number } | null>(null);
const pendingDownload = ref<FileEntry | null>(null);
const addressInput = ref<HTMLInputElement | null>(null);
const rootRef = ref<HTMLElement | null>(null);
const newFolderOpen = ref(false);
const newFolderButton = ref<HTMLButtonElement | null>(null);
const permit = computed(() => permitOf(props.server.id));

const viewOptions = [
  { mode: 'grid' as const, label: '小图标' },
  { mode: 'list' as const, label: '列表' },
  { mode: 'details' as const, label: '详细信息' },
];

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
    pendingDownload.value = null;
    menuOpen.value = '';
  },
  { immediate: true },
);

function toggleMenu(name: 'view' | 'more') {
  menuOpen.value = menuOpen.value === name ? '' : name;
}

function chooseMode(mode: 'grid' | 'list' | 'details') {
  files.setMode(mode);
  menuOpen.value = '';
}

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

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

// FileList 给出的是视口坐标；.lgfiles 是 inline-size 容器（fixed 的包含块），
// 因此这里换算成应用内坐标并夹在应用范围内，避免菜单被窗口裁剪。
function openContext(payload: { entry: FileEntry; x: number; y: number }) {
  files.select(payload.entry);
  const rect = rootRef.value?.getBoundingClientRect();
  if (!rect) {
    context.value = payload;
    return;
  }
  context.value = {
    entry: payload.entry,
    x: clamp(payload.x - rect.left, 8, rect.width - 180),
    y: clamp(payload.y - rect.top, 8, rect.height - 214),
  };
}

function onRowSelect(entry: FileEntry) {
  files.select(entry);
  context.value = null;
}

// Every download entry point resolves the target kind first. Directories need
// an explicit confirmation because the archive keeps symlinks as-is and
// extraction is the user's responsibility.
async function requestDownload(entry: FileEntry): Promise<void> {
  let kind: FileKind;
  try {
    kind = await files.resolveKind(entry);
  } catch (caught) {
    pendingDownload.value = null;
    files.notice.value = toSshError(caught).message;
    return;
  }
  if (kind === 'dir') {
    pendingDownload.value = entry;
    return;
  }
  pendingDownload.value = null;
  await files.download(entry);
}

async function confirmDownload(): Promise<void> {
  const entry = pendingDownload.value;
  pendingDownload.value = null;
  if (entry) await files.download(entry);
}

function dismissDownload(): void {
  pendingDownload.value = null;
}

// 「新建文件夹」是 v1 唯一的写入口：锁定时只负责把用户送到许可面板，解锁后才开对话框。
// 工具栏按钮保持可聚焦（用 aria-disabled 而不是 disabled），否则「为什么点不动」无从发现。
function onNewFolder() {
  if (permit.value.locked) {
    openPermitPanel();
    return;
  }
  newFolderOpen.value = true;
}

function closeNewFolder() {
  newFolderOpen.value = false;
  // 对话框关闭后焦点回到工具栏按钮，键盘用户不会掉到 body 上。
  void nextTick(() => {
    if (newFolderButton.value?.isConnected) newFolderButton.value.focus();
  });
}

async function onFolderCreated(name: string) {
  closeNewFolder();
  toast({ title: '已创建文件夹', message: name, tone: 'success' });
  // 复用文件应用既有的刷新路径，不在这里另写一遍列目录。
  await files.refresh();
  const entry = files.entries.value.find((item) => item.name === name && item.kind === 'dir');
  if (entry) files.select(entry);
}

function onNewFolderNeedPermit() {
  closeNewFolder();
  openPermitPanel();
}

function onViewAudit() {
  closeNewFolder();
  openAudit();
}

async function contextAction(action: 'open' | 'preview' | 'download' | 'copy' | 'meta') {
  const entry = context.value?.entry;
  context.value = null;
  if (!entry) return;
  if (action === 'open') await files.open(entry);
  else if (action === 'preview') {
    files.previewOpen.value = true;
    files.select(entry);
    await files.loadPreview(entry);
  } else if (action === 'download') await requestDownload(entry);
  else if (action === 'copy') {
    try { await navigator.clipboard.writeText(entry.path); } catch { files.notice.value = '复制路径失败'; }
  } else {
    files.previewOpen.value = true;
    files.select(entry);
    if (files.preview.mode === 'none') files.preview.mode = 'meta';
  }
}

function onGlobalKey(event: KeyboardEvent) {
  if (event.defaultPrevented) return;
  // 快捷键只在焦点位于本应用窗口内时生效，避免抢走其他窗口的按键。
  const win = rootRef.value?.closest('.lgdesk-win');
  if (win && event.target instanceof Node && !win.contains(event.target)) return;
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
    // 对话框开着时 Esc 交给它自己（LgModal 的 close-on-esc），不要顺手清掉背后的面板。
    if (newFolderOpen.value) return;
    context.value = null;
    menuOpen.value = '';
    dismissDownload();
    if (files.previewOpen.value) files.closePreview();
  }
}

function onPointerDown(event: PointerEvent) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest('.lgfiles-context, .lgfiles-menu, [data-menu-toggle]')) return;
  context.value = null;
  menuOpen.value = '';
}

onMounted(() => {
  void files.boot();
  window.addEventListener('keydown', onGlobalKey);
  window.addEventListener('pointerdown', onPointerDown);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKey);
  window.removeEventListener('pointerdown', onPointerDown);
});

watch(
  () => props.server.sessionId,
  (session) => {
    if (session && files.canUse.value && !files.currentPath.value) void files.boot();
  },
);
</script>

<template>
  <div ref="rootRef" class="lgfiles">
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
        <button
          ref="newFolderButton"
          type="button"
          class="lgfiles-icon-btn lgfiles-new-folder"
          :class="{ 'is-locked': permit.locked }"
          data-action="new-folder"
          aria-label="新建文件夹"
          :aria-disabled="permit.locked ? 'true' : undefined"
          :title="permit.locked ? '需先解锁写操作' : '新建文件夹'"
          @click="onNewFolder"
        >
          <LgIcon name="folder-plus" :size="15" />
        </button>
        <form class="lgfiles-address" @submit.prevent="submitAddress">
          <input
            ref="addressInput"
            v-model="address"
            aria-label="当前路径"
            spellcheck="false"
            autocomplete="off"
            @keydown.esc.stop.prevent="resetAddress"
          />
        </form>
        <label class="lgfiles-search">
          <LgIcon name="search" :size="13" />
          <input v-model="files.view.query" aria-label="筛选当前目录" placeholder="筛选当前目录" />
        </label>
        <div class="lgfiles-menu-anchor">
          <button
            type="button"
            class="lgfiles-icon-btn"
            :class="{ 'is-on': menuOpen === 'view' }"
            aria-label="视图"
            data-menu-toggle
            @click="toggleMenu('view')"
          >
            <LgIcon name="grid" :size="15" />
          </button>
          <div v-if="menuOpen === 'view'" class="lgfiles-menu" role="menu" aria-label="视图">
            <button
              v-for="option in viewOptions"
              :key="option.mode"
              type="button"
              class="lgfiles-menu__item"
              :class="{ 'is-checked': files.view.mode === option.mode }"
              role="menuitemradio"
              :aria-checked="files.view.mode === option.mode"
              @click="chooseMode(option.mode)"
            >
              <LgIcon v-if="files.view.mode === option.mode" name="check" :size="11" />
              <span v-else class="lgfiles-menu__blank" />
              {{ option.label }}
            </button>
          </div>
        </div>
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
        <div class="lgfiles-menu-anchor">
          <button
            type="button"
            class="lgfiles-icon-btn"
            :class="{ 'is-on': menuOpen === 'more' }"
            aria-label="更多"
            data-menu-toggle
            @click="toggleMenu('more')"
          >
            <LgIcon name="more" :size="15" />
          </button>
          <div v-if="menuOpen === 'more'" class="lgfiles-menu">
            <button type="button" @click="files.setShowHidden(!files.view.showHidden); menuOpen = ''">
              {{ files.view.showHidden ? '隐藏隐藏文件' : '显示隐藏文件' }}
            </button>
            <button type="button" @click="files.refresh(); menuOpen = ''">清空缓存并刷新</button>
          </div>
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
            :reset-key="files.currentPath.value"
            :entries="files.visibleRows.value"
            :selected="files.selectedPath.value"
            :loading="files.loading.value"
            :error="files.error.value?.message ?? ''"
            :truncated="files.truncated.value"
            :sort-key="files.view.sortKey"
            :sort-dir="files.view.sortDir"
            :mode="files.view.mode"
            :offline="files.offline.value"
            @select="onRowSelect"
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
          @download="files.selectedEntry.value && requestDownload(files.selectedEntry.value)"
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

      <div v-if="pendingDownload" class="lgfiles-confirm">
        <div
          class="lgfiles-confirm__dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="lgfiles-confirm-title"
        >
          <strong id="lgfiles-confirm-title">下载文件夹？</strong>
          <p>「{{ pendingDownload.name }}」将打包为 .tar.gz 下载。</p>
          <p>符号链接按原样打包、不跟随目标；解压风险自负。</p>
          <div class="lgfiles-confirm__actions">
            <button type="button" @click="dismissDownload">取消</button>
            <button type="button" class="is-primary" @click="confirmDownload">继续下载</button>
          </div>
        </div>
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

    <NewFolderDialog
      :open="newFolderOpen"
      :server-id="server.id"
      :server-name="server.name || server.id"
      :dir="files.currentPath.value"
      @close="closeNewFolder"
      @created="onFolderCreated"
      @need-permit="onNewFolderNeedPermit"
      @view-audit="onViewAudit"
    />
  </div>
</template>
