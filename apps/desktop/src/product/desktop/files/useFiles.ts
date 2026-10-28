import { computed, reactive, ref, watch } from 'vue';
import type { Server } from '../../types';
import {
  ssh,
  toSshError,
  type FileEntry,
  type FileKind,
  type FileListing,
  type ImagePreview,
  type SshBackend,
  type SshCommandError,
  type TextPreview,
} from '../../ssh';
import {
  isImagePreviewable,
  isTextPreviewable,
} from './format';
import {
  moveSelection,
  toggleSort,
  visibleEntries,
  type ListViewState,
  type SortKey,
} from './listModel';
import { basename, parentOf } from './remotePath';
import { TreeCache, treeChildren, type TreeItem } from './treeCache';

export interface TreeState extends TreeItem {
  expanded: boolean;
  loading: boolean;
  error: string;
  children: TreeState[] | null;
}

export type PreviewMode = 'none' | 'meta' | 'text' | 'image';

export interface PreviewState {
  mode: PreviewMode;
  entry: FileEntry | null;
  text: TextPreview | null;
  image: ImagePreview | null;
  loading: boolean;
  error: SshCommandError | null;
}

export type DownloadStatus = 'waiting' | 'running' | 'saved' | 'canceled' | 'error';

export interface DownloadTask {
  id: string;
  name: string;
  path: string;
  kind: FileKind;
  transferred: number;
  total: number | null;
  status: DownloadStatus;
  message: string;
  targetPath: string;
}

function createNode(item: TreeItem): TreeState {
  return { ...item, expanded: false, loading: false, error: '', children: null };
}

function buildRoots(home: string, mounts: string[]): TreeState[] {
  const roots: TreeState[] = [];
  const seen = new Set<string>();
  const push = (path: string, name: string) => {
    if (!path || seen.has(path)) return;
    seen.add(path);
    roots.push(createNode({ path, name, kind: 'dir' }));
  };
  push(home, '主目录');
  push('/', '根目录');
  for (const mount of mounts) push(mount, mount === '/' ? '根目录' : mount);
  return roots;
}

function resetTree(nodes: TreeState[]): void {
  for (const node of nodes) {
    node.children = null;
    node.expanded = false;
    node.error = '';
  }
}

function findNode(nodes: TreeState[], path: string): TreeState | null {
  for (const node of nodes) {
    if (node.path === path) return node;
    const found = node.children ? findNode(node.children, path) : null;
    if (found) return found;
  }
  return null;
}

function transferId(): string {
  return `d${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

export function useFiles(server: Server, client: SshBackend = ssh) {
  const homePath = ref('');
  const roots = ref<TreeState[]>([]);
  const currentPath = ref('');
  const listing = ref<FileListing | null>(null);
  const entries = ref<FileEntry[]>([]);
  const loading = ref(false);
  const error = ref<SshCommandError | null>(null);
  const selectedPath = ref('');
  const notice = ref('');
  const history = ref<string[]>([]);
  const historyIndex = ref(-1);
  const previewOpen = ref(false);
  const downloads = ref<DownloadTask[]>([]);
  const view = reactive<ListViewState>({
    sortKey: 'name',
    sortDir: 'asc',
    query: '',
    showHidden: false,
  });

  let cache = new TreeCache((path) => loadEntries(path));
  let navToken = 0;
  let previewToken = 0;
  let openToken = 0;

  const offline = computed(() => server.status === 'offline' || server.status === 'error');
  const usable = computed(() => client.available && Boolean(server.sessionId));
  const canUse = computed(() => usable.value && !offline.value);
  const visibleRows = computed(() => visibleEntries(entries.value, view));
  const truncated = computed(() => listing.value?.truncated ?? false);
  const selectedEntry = computed(
    () => entries.value.find((entry) => entry.path === selectedPath.value) ?? null,
  );
  const canBack = computed(() => historyIndex.value > 0);
  const canForward = computed(() => historyIndex.value >= 0 && historyIndex.value < history.value.length - 1);
  const parentPath = computed(() => {
    const parent = parentOf(currentPath.value);
    return parent && parent !== currentPath.value ? parent : null;
  });
  const canUp = computed(() => parentPath.value !== null);

  const preview = reactive<PreviewState>({
    mode: 'none',
    entry: null,
    text: null,
    image: null,
    loading: false,
    error: null,
  });

  async function loadEntries(path: string): Promise<FileEntry[]> {
    const result = await client.sftpList(server.id, path);
    return result.entries;
  }

  async function load(path: string, token: number): Promise<FileListing | null> {
    const result = await client.sftpList(server.id, path);
    if (token !== navToken) return null;
    listing.value = result;
    entries.value = result.entries;
    currentPath.value = result.canonicalPath;
    if (!entries.value.some((entry) => entry.path === selectedPath.value)) {
      selectedPath.value = '';
    }
    return result;
  }

  async function navigate(path: string, push = true): Promise<boolean> {
    if (!usable.value) return false;
    const token = ++navToken;
    openToken += 1;
    loading.value = true;
    error.value = null;
    try {
      const result = await load(path, token);
      if (!result) return false;
      if (push) {
        history.value = [...history.value.slice(0, historyIndex.value + 1), result.canonicalPath];
        historyIndex.value = history.value.length - 1;
      }
      view.query = '';
      loading.value = false;
      return true;
    } catch (caught) {
      if (token !== navToken) return false;
      error.value = toSshError(caught);
      loading.value = false;
      return false;
    }
  }

  async function go(delta: number): Promise<boolean> {
    const index = historyIndex.value + delta;
    if (index < 0 || index >= history.value.length) return false;
    const target = history.value[index];
    const ok = await navigate(target, false);
    if (!ok) return false;
    history.value[index] = currentPath.value;
    historyIndex.value = index;
    return true;
  }

  async function goUp(): Promise<boolean> {
    const parent = parentPath.value;
    return parent ? navigate(parent) : false;
  }

  async function boot(): Promise<void> {
    if (!usable.value) return;
    try {
      const home = await client.sftpHome(server.id);
      homePath.value = home;
      roots.value = buildRoots(home, server.disks.map((disk) => disk.mount));
      await navigate(home);
      await expandPath(home);
    } catch (caught) {
      error.value = toSshError(caught);
    }
  }

  async function expandNode(path: string, force = false): Promise<void> {
    const node = findNode(roots.value, path);
    if (!node || node.kind === 'symlink') return;
    node.expanded = true;
    if (node.children && !force) return;
    node.loading = true;
    node.error = '';
    try {
      const loaded = await cache.load(path, force);
      node.children = treeChildren(loaded).map(createNode);
    } catch (caught) {
      node.error = toSshError(caught).message;
    } finally {
      node.loading = false;
    }
  }

  async function toggleNode(path: string): Promise<void> {
    const node = findNode(roots.value, path);
    if (!node || node.kind === 'symlink') return;
    if (node.expanded) {
      node.expanded = false;
      return;
    }
    await expandNode(path);
  }

  async function retryNode(path: string): Promise<void> {
    const node = findNode(roots.value, path);
    if (!node || node.loading) return;
    node.error = '';
    await expandNode(path, true);
  }

  async function expandPath(path: string): Promise<void> {
    const root = roots.value.find((node) =>
      node.path === '/' ? path.startsWith('/') : path === node.path || path.startsWith(`${node.path}/`),
    );
    if (!root) return;
    if (!root.expanded) await expandNode(root.path);
    if (root.kind === 'symlink') return;
    const relative = root.path === '/' ? path.slice(1) : path.slice(root.path.length + 1);
    let cursor = root;
    let walked = root.path;
    for (const segment of relative.split('/').filter(Boolean)) {
      walked = walked === '/' ? `/${segment}` : `${walked}/${segment}`;
      const child = findNode(cursor.children ?? [], walked);
      if (!child || child.kind === 'symlink') break;
      if (!child.expanded) await expandNode(walked);
      cursor = child;
    }
  }

  function select(entry: FileEntry): void {
    selectedPath.value = entry.path;
    previewToken += 1;
    openToken += 1;
    preview.entry = entry;
    preview.text = null;
    preview.image = null;
    preview.error = null;
    preview.loading = false;
    if (preview.mode !== 'none') preview.mode = 'meta';
  }

  async function resolveKind(entry: FileEntry): Promise<FileKind> {
    if (entry.kind !== 'symlink') return entry.kind;
    const resolved = await client.sftpStat(server.id, entry.path);
    return resolved.targetKind ?? resolved.kind;
  }

  async function open(entry: FileEntry): Promise<void> {
    select(entry);
    const token = openToken;
    let kind: FileKind;
    try {
      kind = await resolveKind(entry);
    } catch (caught) {
      if (token !== openToken) return;
      error.value = toSshError(caught);
      return;
    }
    if (token !== openToken) return;
    if (kind === 'dir') {
      const ok = await navigate(entry.path);
      if (ok) {
        previewOpen.value = false;
        await expandPath(currentPath.value);
      }
      return;
    }
    previewOpen.value = true;
    await loadPreview(entry);
  }

  async function loadPreview(entry: FileEntry): Promise<void> {
    const token = ++previewToken;
    preview.entry = entry;
    preview.text = null;
    preview.image = null;
    preview.error = null;
    preview.loading = true;
    try {
      if (isImagePreviewable(entry.name) && entry.kind !== 'other') {
        const image = await client.sftpPreviewImage(server.id, entry.path);
        if (token !== previewToken) return;
        preview.image = image;
        preview.mode = 'image';
      } else if (isTextPreviewable(entry.name) && entry.kind !== 'other') {
        const text = await client.sftpPreviewText(server.id, entry.path);
        if (token !== previewToken) return;
        preview.text = text;
        preview.mode = 'text';
      } else {
        preview.mode = 'meta';
      }
    } catch (caught) {
      if (token !== previewToken) return;
      preview.error = toSshError(caught);
      preview.mode = 'meta';
    }
    preview.loading = false;
  }

  function togglePreview(): void {
    previewOpen.value = !previewOpen.value;
    if (previewOpen.value && preview.entry && preview.mode === 'none') {
      void loadPreview(preview.entry);
    }
  }

  function closePreview(): void {
    previewOpen.value = false;
    previewToken += 1;
    preview.loading = false;
  }

  async function refresh(): Promise<void> {
    if (!currentPath.value) {
      await boot();
      return;
    }
    cache.clear();
    resetTree(roots.value);
    await navigate(currentPath.value, false);
    if (currentPath.value) await expandPath(currentPath.value);
  }

  async function selectTree(path: string): Promise<void> {
    await navigate(path);
    await expandPath(currentPath.value);
  }

  async function openTree(path: string): Promise<void> {
    const node = findNode(roots.value, path);
    if (!node) return;
    if (node.kind === 'dir') await toggleNode(path);
    await navigate(path);
  }

  function toggleSortView(key: SortKey): void {
    const next = toggleSort(view, key);
    view.sortKey = next.sortKey;
    view.sortDir = next.sortDir;
  }

  async function download(entry: FileEntry): Promise<void> {
    let kind: FileKind;
    try {
      kind = await resolveKind(entry);
    } catch (caught) {
      error.value = toSshError(caught);
      return;
    }
    const task = reactive<DownloadTask>({
      id: transferId(),
      name: entry.name,
      path: entry.path,
      kind,
      transferred: 0,
      total: null,
      status: 'waiting',
      message: '等待中',
      targetPath: '',
    });
    downloads.value = [task, ...downloads.value];
    void runDownload(task);
  }

  async function runDownload(task: DownloadTask): Promise<void> {
    // Stay in `waiting` until the backend reports real progress: with the
    // transfer queue this is what makes "等待中" observable before a slot opens.
    try {
      const result = await client.sftpDownload({
        serverId: server.id,
        path: task.path,
        kind: task.kind,
        transferId: task.id,
        onEvent: (event) => {
          if (event.type === 'progress') {
            if (task.status === 'waiting') {
              task.status = 'running';
              task.message = '传输中';
            }
            task.transferred = event.transferred;
            task.total = event.total;
          }
        },
      });
      if (result.status === 'canceled') {
        task.status = 'canceled';
        task.message = '已取消';
      } else {
        task.status = 'saved';
        task.targetPath = result.targetPath ?? '';
        task.message = result.targetPath ? `已保存到 ${result.targetPath}` : '已保存';
      }
    } catch (caught) {
      task.status = 'error';
      task.message = toSshError(caught).message;
      notice.value = task.message;
    }
  }

  function cancelDownload(id: string): void {
    void client.sftpTransferCancel(id).catch(() => {});
    const task = downloads.value.find((item) => item.id === id);
    if (task) task.message = '正在取消';
  }

  function removeDownload(id: string): void {
    downloads.value = downloads.value.filter((item) => item.id !== id);
  }

  function setShowHidden(show: boolean): void {
    view.showHidden = show;
  }

  watch(
    () => server.sessionId,
    (next, previous) => {
      if (next === previous) return;
      navToken += 1;
      previewToken += 1;
      cache = new TreeCache((path) => loadEntries(path));
      roots.value = [];
      history.value = [];
      historyIndex.value = -1;
      homePath.value = '';
      currentPath.value = '';
      entries.value = [];
      listing.value = null;
      selectedPath.value = '';
      loading.value = false;
      error.value = null;
      notice.value = '';
      view.query = '';
      previewOpen.value = false;
      preview.mode = 'none';
      preview.entry = null;
      preview.text = null;
      preview.image = null;
      preview.error = null;
      preview.loading = false;
      if (next && canUse.value) void boot();
    },
  );

  return {
    canUse,
    offline,
    homePath,
    roots,
    currentPath,
    entries,
    visibleRows,
    listing,
    loading,
    error,
    truncated,
    selectedPath,
    selectedEntry,
    view,
    notice,
    historyIndex,
    canBack,
    canForward,
    canUp,
    previewOpen,
    preview,
    downloads,
    boot,
    navigate,
    go,
    goUp,
    refresh,
    select,
    open,
    toggleNode,
    retryNode,
    toggleSort: toggleSortView,
    togglePreview,
    closePreview,
    loadPreview,
    download,
    cancelDownload,
    removeDownload,
    setShowHidden,
    selectTree,
    openTree,
    resolveKind,
    basename,
    moveSelection: (action: 'up' | 'down' | 'page-up' | 'page-down' | 'home' | 'end', page: number) =>
      moveSelection(visibleRows.value, selectedPath.value, action, page),
  };
}
