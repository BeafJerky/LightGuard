<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { LgIcon, LG_SCOPE_KEY } from '@lightguard/lightui';
import type { Server } from '../../types';
import type { FileEntry, FileKind } from '../../ssh/backend';
import { ssh, toSshError, type UploadSource } from '../../ssh';
import { openPermitPanel, permitOf } from '../../guard/store';
import { openAudit } from '../../router';
import { toast } from '../../ui';
import FileList from './FileList.vue';
import FilePreview from './FilePreview.vue';
import FileTree from './FileTree.vue';
import NewFolderDialog from './NewFolderDialog.vue';
import RenameDialog from './RenameDialog.vue';
import PermissionDialog from './PermissionDialog.vue';
import DeleteDialog from './DeleteDialog.vue';
import UploadDialog from './UploadDialog.vue';
import SearchPanel from './SearchPanel.vue';
import { pathCrumbs, visibleCrumbs } from './breadcrumb';
import {
  loadFavorites,
  loadRecents,
  pushRecent,
  toggleFavorite,
} from './favorites';
import { resolveIconScheme, type IconScheme } from './iconScheme';
import type { FileTab } from './useFileTabs';
import type { SortKey } from './listModel';

const props = defineProps<{ tab: FileTab; server: Server; active: boolean }>();
const emit = defineEmits<{ 'open-tab': [path: string] }>();
const files = props.tab.files;
const scope = inject(LG_SCOPE_KEY, null);
const themeId = computed(() => scope?.value.theme.id ?? 'light');
const iconScheme = computed(() => resolveIconScheme(files.iconScheme.value, themeId.value));

const address = ref('');
const addressEditing = ref(false);
const drawerOpen = ref(false);
const menuOpen = ref<'sort' | 'view' | 'more' | ''>('');
const context = ref<{ entry: FileEntry; x: number; y: number } | null>(null);
const pendingDownload = ref<FileEntry | null>(null);
const addressInput = ref<HTMLInputElement | null>(null);
const rootRef = ref<HTMLElement | null>(null);
const newFolderOpen = ref(false);
const newFolderButton = ref<HTMLButtonElement | null>(null);
const renameTarget = ref<FileEntry | null>(null);
const permissionTarget = ref<FileEntry | null>(null);
const deleteTarget = ref<FileEntry | null>(null);
const uploadOpen = ref(false);
const uploadFiles = ref<UploadSource[]>([]);
const searchOpen = ref(false);
const favoritesOpen = ref(false);
const favoritesList = ref<string[]>([]);
const recentsList = ref<string[]>([]);
const permit = computed(() => permitOf(props.server.id));

const viewOptions = [
  { mode: 'grid' as const, label: '小图标' },
  { mode: 'list' as const, label: '列表' },
  { mode: 'details' as const, label: '详细信息' },
];

const iconOptions: { value: IconScheme; label: string }[] = [
  { value: 'auto', label: '跟随主题' },
  { value: 'line', label: '线性 + 分类色' },
  { value: 'color', label: '彩色图标' },
];

const sortOptions: { key: SortKey; label: string }[] = [
  { key: 'name', label: '名称' },
  { key: 'size', label: '大小' },
  { key: 'mtime', label: '修改时间' },
  { key: 'type', label: '类型' },
  { key: 'mode', label: '权限' },
  { key: 'owner', label: '所有者' },
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
const crumbs = computed(() => pathCrumbs(files.currentPath.value || '/'));
const crumbView = computed(() => visibleCrumbs(crumbs.value));
// 「连接已断开」只在曾经建立过会话时才有意义；从未连接时提示先连接。
const sessionLost = computed(() => Boolean(props.server.sessionId) && files.offline.value);

watch(
  () => files.currentPath.value,
  (path) => {
    address.value = path;
    addressEditing.value = false;
    drawerOpen.value = false;
    context.value = null;
    pendingDownload.value = null;
    menuOpen.value = '';
    if (path) {
      pushRecent(props.server.id, path);
      favoritesList.value = loadFavorites(props.server.id);
      recentsList.value = loadRecents(props.server.id);
    }
  },
  { immediate: true },
);

function toggleMenu(name: 'sort' | 'view' | 'more') {
  menuOpen.value = menuOpen.value === name ? '' : name;
}

function chooseSort(key: SortKey) {
  files.toggleSort(key);
  menuOpen.value = '';
}

function chooseMode(mode: 'grid' | 'list' | 'details') {
  files.setMode(mode);
  menuOpen.value = '';
}

function chooseIconScheme(scheme: IconScheme) {
  files.setIconScheme(scheme);
  menuOpen.value = '';
}

function submitAddress() {
  const target = address.value.trim();
  addressEditing.value = false;
  if (target) void files.navigate(target);
  else address.value = files.currentPath.value;
}

function cancelEdit() {
  if (!addressEditing.value) return;
  address.value = files.currentPath.value;
  addressEditing.value = false;
}

function focusAddress() {
  addressEditing.value = true;
  void nextTick(() => {
    addressInput.value?.focus();
    addressInput.value?.select();
  });
}

function navigateTo(path: string) {
  if (path === files.currentPath.value) return;
  addressEditing.value = false;
  void files.navigate(path);
}

function onCrumbsClick(event: MouseEvent) {
  const target = event.target;
  if (target instanceof Element && target.closest('button')) return;
  focusAddress();
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

// FileList 给出的是视口坐标；.lgfiles-view 是 inline-size 容器（fixed 的包含块），
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

function onRowSelect(
  entry: FileEntry,
  modifiers?: { toggle?: boolean; range?: boolean },
) {
  const mode = modifiers?.range ? 'range' : modifiers?.toggle ? 'toggle' : 'single';
  files.select(entry, mode);
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

function openWriteEntry(entry: FileEntry, target: 'rename' | 'permission' | 'delete') {
  context.value = null;
  if (permit.value.locked) {
    openPermitPanel();
    return;
  }
  if (target === 'rename') renameTarget.value = entry;
  else if (target === 'permission') permissionTarget.value = entry;
  else deleteTarget.value = entry;
}

async function onRenamed(path: string) {
  renameTarget.value = null;
  toast({ title: '已重命名', message: path, tone: 'success' });
  await files.refresh();
  const entry = files.entries.value.find((item) => item.path === path);
  if (entry) files.select(entry);
}

async function onPermissionChanged() {
  const entry = permissionTarget.value;
  permissionTarget.value = null;
  toast({ title: '权限已修改', message: entry?.name ?? '', tone: 'success' });
  await files.refresh();
}

async function onDeleted() {
  const entry = deleteTarget.value;
  deleteTarget.value = null;
  toast({ title: '已删除', message: entry?.name ?? '', tone: 'success' });
  if (entry && files.currentPath.value === entry.path) {
    await files.goUp();
  } else {
    await files.refresh();
  }
}

function onWriteNeedPermit(close: () => void) {
  close();
  openPermitPanel();
}

async function onUpload() {
  if (permit.value.locked) {
    openPermitPanel();
    return;
  }
  const picked = await ssh.pickUploadPaths();
  if (!picked.length) return;
  uploadFiles.value = picked;
  uploadOpen.value = true;
}

function closeUpload() {
  uploadOpen.value = false;
  uploadFiles.value = [];
}

async function onUploaded(count: number) {
  closeUpload();
  toast({ title: '已上传', message: `${count} 个文件`, tone: 'success' });
  await files.refresh();
}

async function onSearchOpen(entry: FileEntry) {
  searchOpen.value = false;
  const index = entry.path.lastIndexOf('/');
  const parent = index <= 0 ? '/' : entry.path.slice(0, index);
  await files.navigate(parent);
  const found = files.entries.value.find((item) => item.path === entry.path);
  if (found) files.select(found);
}

function toggleFavorites() {
  favoritesOpen.value = !favoritesOpen.value;
  favoritesList.value = loadFavorites(props.server.id);
  recentsList.value = loadRecents(props.server.id);
}

function toggleCurrentFavorite() {
  const path = files.currentPath.value;
  if (!path) return;
  const added = toggleFavorite(props.server.id, path);
  favoritesList.value = loadFavorites(props.server.id);
  toast({ title: added ? '已收藏' : '已取消收藏', message: path, tone: 'success' });
}

function goFavorite(path: string) {
  favoritesOpen.value = false;
  void files.navigate(path);
}

async function batchDownload() {
  const entries = files.selectedEntries.value;
  if (!entries.length) return;
  const targetDir = await ssh.pickDownloadDir();
  if (!targetDir) return;
  for (const entry of entries) {
    await files.downloadTo(entry, targetDir);
  }
  toast({
    title: '已加入下载队列',
    message: `${entries.length} 项 → 所选目录`,
    tone: 'success',
  });
}

async function contextAction(action: 'open' | 'new-tab' | 'preview' | 'download' | 'copy' | 'meta') {
  const entry = context.value?.entry;
  context.value = null;
  if (!entry) return;
  if (action === 'new-tab') emit('open-tab', entry.path);
  else if (action === 'open') await files.open(entry);
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
  if (!props.active) return;
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
  if (!props.active) return;
  const target = event.target;
  if (!(target instanceof Element)) return;
  if (target.closest('.lgfiles-context, .lgfiles-menu, [data-menu-toggle]')) return;
  context.value = null;
  menuOpen.value = '';
}

onMounted(() => {
  window.addEventListener('keydown', onGlobalKey);
  window.addEventListener('pointerdown', onPointerDown);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKey);
  window.removeEventListener('pointerdown', onPointerDown);
});
</script>

<template>
  <div ref="rootRef" class="lgfiles-view" :class="{ 'is-active': active }">
    <div v-if="!files.canUse.value" class="lgfiles-blank">
      <LgIcon name="folder" :size="30" />
      <strong>{{ sessionLost ? '连接已断开' : '请先连接服务器' }}</strong>
      <span>{{ sessionLost ? '重新连接后打开文件应用，或回到桌面重试。' : '文件浏览依赖现有 SSH 会话。' }}</span>
    </div>

    <template v-else>
      <div class="lgfiles-toolbar">
        <div class="lgfiles-toolbar__row lgfiles-toolbar__row--nav">
          <div class="lgfiles-toolbar__group">
            <button type="button" class="lgfiles-icon-btn" aria-label="后退" :disabled="!files.canBack.value" @click="files.go(-1)">
              <LgIcon name="chevron-left" :size="18" />
            </button>
            <button type="button" class="lgfiles-icon-btn" aria-label="前进" :disabled="!files.canForward.value" @click="files.go(1)">
              <LgIcon name="chevron-right" :size="18" />
            </button>
            <button type="button" class="lgfiles-icon-btn" aria-label="上级" :disabled="!files.canUp.value" @click="files.goUp()">
              <LgIcon name="arrow-up" :size="18" />
            </button>
            <button type="button" class="lgfiles-icon-btn" aria-label="刷新" @click="files.refresh()">
              <LgIcon name="refresh" :size="18" />
            </button>
          </div>
          <form class="lgfiles-address" aria-keyshortcuts="Control+L" @submit.prevent="submitAddress">
            <div
              v-show="!addressEditing"
              class="lgfiles-crumbs"
              role="navigation"
              aria-label="当前路径"
              title="点击空白处或按 Ctrl+L 编辑路径"
              @click="onCrumbsClick"
            >
              <LgIcon name="folder" :size="15" class="lgfiles-crumbs__icon" />
              <template v-for="(item, index) in crumbView" :key="item.kind === 'ellipsis' ? 'more' : item.crumb.path">
                <button
                  v-if="item.kind === 'ellipsis'"
                  type="button"
                  class="lgfiles-crumb__more"
                  aria-label="显示完整路径"
                  :title="item.hidden.map((crumb) => crumb.path).join(' / ')"
                  @click.stop="focusAddress"
                >
                  …
                </button>
                <button
                  v-else
                  type="button"
                  class="lgfiles-crumb"
                  :class="{ 'is-current': item.crumb.path === files.currentPath.value }"
                  :data-crumb-path="item.crumb.path"
                  :title="item.crumb.path"
                  @click.stop="navigateTo(item.crumb.path)"
                >
                  {{ item.crumb.label }}
                </button>
                <LgIcon
                  v-if="index < crumbView.length - 1"
                  name="chevron-right"
                  :size="12"
                  class="lgfiles-crumbs__sep"
                />
              </template>
            </div>
            <input
              v-show="addressEditing"
              ref="addressInput"
              v-model="address"
              aria-label="当前路径"
              spellcheck="false"
              autocomplete="off"
              @blur="cancelEdit"
              @keydown.esc.stop.prevent="cancelEdit"
            />
          </form>
          <label class="lgfiles-search">
            <LgIcon name="search" :size="16" />
            <input v-model="files.view.query" aria-label="筛选当前目录" placeholder="筛选当前目录" />
          </label>
          <button
            type="button"
            class="lgfiles-cmd"
            data-action="deep-search"
            aria-label="递归搜索"
            title="递归搜索"
            @click="searchOpen = true"
          >
            <LgIcon name="search" :size="15" />
            <span class="lgfiles-cmd__label">递归</span>
          </button>
          <button
            type="button"
            class="lgfiles-cmd"
            data-action="favorites"
            aria-label="收藏与最近"
            title="收藏与最近"
            @click="toggleFavorites"
          >
            <LgIcon name="spark" :size="15" />
            <span class="lgfiles-cmd__label">收藏</span>
          </button>
        </div>

        <div class="lgfiles-toolbar__row lgfiles-toolbar__row--actions">
          <button
            ref="newFolderButton"
            type="button"
            class="lgfiles-cmd lgfiles-new-folder"
            :class="{ 'is-locked': permit.locked }"
            data-action="new-folder"
            aria-label="新建文件夹"
            :aria-disabled="permit.locked ? 'true' : undefined"
            :title="permit.locked ? '需先解锁写操作' : '新建文件夹'"
            @click="onNewFolder"
          >
            <LgIcon name="folder-plus" :size="16" />
            <span class="lgfiles-cmd__label">新建文件夹</span>
          </button>
          <button
            type="button"
            class="lgfiles-cmd"
            :class="{ 'is-locked': permit.locked }"
            data-action="upload"
            aria-label="上传"
            :aria-disabled="permit.locked ? 'true' : undefined"
            :title="permit.locked ? '需先解锁写操作' : '上传文件'"
            @click="onUpload"
          >
            <LgIcon name="arrow-up" :size="16" />
            <span class="lgfiles-cmd__label">上传</span>
          </button>
          <span class="lgfiles-toolbar__divider" aria-hidden="true" />
          <div class="lgfiles-menu-anchor">
            <button
              type="button"
              class="lgfiles-cmd"
              :class="{ 'is-on': menuOpen === 'sort' }"
              aria-label="排序"
              data-menu-toggle
              @click="toggleMenu('sort')"
            >
              <LgIcon name="sort" :size="16" />
              <span class="lgfiles-cmd__label">排序</span>
              <LgIcon name="chevron-down" :size="12" class="lgfiles-cmd__chev" />
            </button>
            <div v-if="menuOpen === 'sort'" class="lgfiles-menu lgfiles-menu--start" role="menu" aria-label="排序">
              <button
                v-for="option in sortOptions"
                :key="option.key"
                type="button"
                class="lgfiles-menu__item lgfiles-menu__item--sort"
                :class="{ 'is-checked': files.view.sortKey === option.key }"
                role="menuitemradio"
                :aria-checked="files.view.sortKey === option.key"
                @click="chooseSort(option.key)"
              >
                <LgIcon v-if="files.view.sortKey === option.key" name="check" :size="13" />
                <span v-else class="lgfiles-menu__blank" />
                {{ option.label }}
                <span v-if="files.view.sortKey === option.key" class="lgfiles-menu__dir" aria-hidden="true">
                  {{ files.view.sortDir === 'asc' ? '↑' : '↓' }}
                </span>
              </button>
            </div>
          </div>
          <div class="lgfiles-menu-anchor">
            <button
              type="button"
              class="lgfiles-cmd"
              :class="{ 'is-on': menuOpen === 'view' }"
              aria-label="视图"
              data-menu-toggle
              @click="toggleMenu('view')"
            >
              <LgIcon name="grid" :size="16" />
              <span class="lgfiles-cmd__label">查看</span>
              <LgIcon name="chevron-down" :size="12" class="lgfiles-cmd__chev" />
            </button>
            <div v-if="menuOpen === 'view'" class="lgfiles-menu lgfiles-menu--start" role="menu" aria-label="视图">
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
                <LgIcon v-if="files.view.mode === option.mode" name="check" :size="13" />
                <span v-else class="lgfiles-menu__blank" />
                {{ option.label }}
              </button>
              <span class="lgfiles-menu__divider" aria-hidden="true" />
              <span class="lgfiles-menu__group">文件图标</span>
              <button
                v-for="option in iconOptions"
                :key="option.value"
                type="button"
                class="lgfiles-menu__item"
                :class="{
                  'is-checked': files.iconScheme.value === option.value,
                  'lgfiles-menu__item--meta': option.value === 'auto',
                }"
                role="menuitemradio"
                :aria-checked="files.iconScheme.value === option.value"
                @click="chooseIconScheme(option.value)"
              >
                <LgIcon v-if="files.iconScheme.value === option.value" name="check" :size="13" />
                <span v-else class="lgfiles-menu__blank" />
                <span>{{ option.label }}</span>
                <span v-if="option.value === 'auto'" class="lgfiles-menu__dir" aria-hidden="true">
                  {{ iconScheme === 'color' ? '彩色' : '线性' }}
                </span>
              </button>
            </div>
          </div>
          <span class="lgfiles-toolbar__spacer" />
          <button type="button" class="lgfiles-icon-btn lgfiles-only-narrow" aria-label="目录树" @click="drawerOpen = !drawerOpen">
            <LgIcon name="menu" :size="16" />
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
              <LgIcon name="more" :size="18" />
            </button>
            <div v-if="menuOpen === 'more'" class="lgfiles-menu">
              <button type="button" @click="files.setShowHidden(!files.view.showHidden); menuOpen = ''">
                {{ files.view.showHidden ? '隐藏隐藏文件' : '显示隐藏文件' }}
              </button>
              <button type="button" @click="files.refresh(); menuOpen = ''">清空缓存并刷新</button>
            </div>
          </div>
          <button
            type="button"
            class="lgfiles-cmd"
            :class="{ 'is-on': files.previewOpen.value }"
            aria-label="预览"
            @click="files.togglePreview()"
          >
            <LgIcon name="eye" :size="16" />
            <span class="lgfiles-cmd__label">预览</span>
          </button>
        </div>
      </div>

      <div
        v-if="files.selectedPaths.value.length > 1"
        class="lgfiles-batch"
        role="status"
      >
        <span>已选 {{ files.selectedPaths.value.length }} 项</span>
        <button type="button" data-action="batch-download" @click="batchDownload">下载</button>
        <button type="button" data-action="batch-clear" @click="files.clearSelection()">清空</button>
      </div>

      <div v-if="favoritesOpen" class="lgfiles-favorites">
        <div class="lgfiles-favorites__head">
          <strong>收藏与最近</strong>
          <button type="button" data-action="favorite-current" @click="toggleCurrentFavorite">
            收藏当前目录
          </button>
        </div>
        <p class="lgfiles-favorites__label">收藏</p>
        <p v-if="!favoritesList.length" class="lgfiles-favorites__empty">还没有收藏目录。</p>
        <button
          v-for="path in favoritesList"
          :key="`fav-${path}`"
          type="button"
          class="lgfiles-favorites__item"
          :data-path="path"
          @click="goFavorite(path)"
        >
          {{ path }}
        </button>
        <p class="lgfiles-favorites__label">最近</p>
        <p v-if="!recentsList.length" class="lgfiles-favorites__empty">还没有访问记录。</p>
        <button
          v-for="path in recentsList"
          :key="`recent-${path}`"
          type="button"
          class="lgfiles-favorites__item"
          @click="goFavorite(path)"
        >
          {{ path }}
        </button>
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
            :selected-paths="files.selectedPaths.value"
            :loading="files.loading.value"
            :error="files.error.value?.message ?? ''"
            :truncated="files.truncated.value"
            :sort-key="files.view.sortKey"
            :sort-dir="files.view.sortDir"
            :mode="files.view.mode"
            :scheme="iconScheme"
            :offline="files.offline.value"
            @select="onRowSelect"
            @open="files.open"
            @sort="files.toggleSort"
            @back="files.goUp()"
            @refresh="files.refresh()"
            @context="openContext"
            @open-tab="(entry: FileEntry) => emit('open-tab', entry.path)"
          />
          <footer class="lgfiles-status">
            <span>{{ statusText }}</span>
            <span v-if="activeDownloads.length" class="lgfiles-status__transfer">
              <LgIcon name="download" :size="14" />
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
        <button
          v-if="(context.entry.targetKind ?? context.entry.kind) === 'dir'"
          type="button"
          @click="contextAction('new-tab')"
        >
          在新标签页打开
        </button>
        <button type="button" @click="contextAction('preview')">预览</button>
        <button type="button" @click="contextAction('download')">
          {{ (context.entry.targetKind ?? context.entry.kind) === 'dir' ? '下载文件夹' : '下载' }}
        </button>
        <button type="button" @click="contextAction('copy')">复制路径</button>
        <button type="button" @click="contextAction('meta')">属性</button>
        <button
          type="button"
          data-action="rename"
          :aria-disabled="permit.locked ? 'true' : undefined"
          @click="openWriteEntry(context.entry, 'rename')"
        >
          重命名
        </button>
        <button
          type="button"
          data-action="permission"
          :aria-disabled="permit.locked ? 'true' : undefined"
          @click="openWriteEntry(context.entry, 'permission')"
        >
          权限
        </button>
        <button
          type="button"
          data-action="delete"
          :aria-disabled="permit.locked ? 'true' : undefined"
          @click="openWriteEntry(context.entry, 'delete')"
        >
          删除
        </button>
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
    <RenameDialog
      v-if="renameTarget"
      :open="Boolean(renameTarget)"
      :server-id="server.id"
      :server-name="server.name || server.id"
      :path="renameTarget.path"
      :current-name="renameTarget.name"
      @close="renameTarget = null"
      @renamed="onRenamed"
      @need-permit="onWriteNeedPermit(() => (renameTarget = null))"
      @view-audit="onViewAudit"
    />
    <PermissionDialog
      v-if="permissionTarget"
      :open="Boolean(permissionTarget)"
      :server-id="server.id"
      :server-name="server.name || server.id"
      :path="permissionTarget.path"
      :current-mode="permissionTarget.mode.toString(8).padStart(3, '0')"
      @close="permissionTarget = null"
      @changed="onPermissionChanged"
      @need-permit="onWriteNeedPermit(() => (permissionTarget = null))"
      @view-audit="onViewAudit"
    />
    <DeleteDialog
      v-if="deleteTarget"
      :open="Boolean(deleteTarget)"
      :server-id="server.id"
      :server-name="server.name || server.id"
      :path="deleteTarget.path"
      :kind="deleteTarget.kind === 'dir' ? 'dir' : 'file'"
      @close="deleteTarget = null"
      @deleted="onDeleted"
      @need-permit="onWriteNeedPermit(() => (deleteTarget = null))"
      @view-audit="onViewAudit"
    />
    <UploadDialog
      v-if="uploadOpen"
      :open="uploadOpen"
      :server-id="server.id"
      :server-name="server.name || server.id"
      :dir="files.currentPath.value"
      :files="uploadFiles"
      @close="closeUpload"
      @uploaded="onUploaded"
      @need-permit="onWriteNeedPermit(closeUpload)"
      @view-audit="onViewAudit"
    />
    <SearchPanel
      v-if="searchOpen"
      :open="searchOpen"
      :server-id="server.id"
      :server-name="server.name || server.id"
      :root="files.currentPath.value"
      @close="searchOpen = false"
      @open="onSearchOpen"
    />
  </div>
</template>
