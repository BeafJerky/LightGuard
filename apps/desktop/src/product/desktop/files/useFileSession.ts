import { computed, reactive, ref, watch, type ComputedRef, type Ref } from 'vue';
import type { Server } from '../../types';
import {
  ssh,
  toSshError,
  type FileEntry,
  type FileKind,
  type SshBackend,
} from '../../ssh';
import { loadIconScheme, saveIconScheme, type IconScheme } from './iconScheme';
import type { ViewMode } from './listModel';
import { TreeCache, treeChildren, type TreeItem } from './treeCache';

export interface TreeState extends TreeItem {
  expanded: boolean;
  loading: boolean;
  error: string;
  children: TreeState[] | null;
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

export interface FileSession {
  usable: ComputedRef<boolean>;
  offline: ComputedRef<boolean>;
  homePath: Ref<string>;
  roots: Ref<TreeState[]>;
  downloads: Ref<DownloadTask[]>;
  mode: Ref<ViewMode>;
  showHidden: Ref<boolean>;
  iconScheme: Ref<IconScheme>;
  generation: Ref<number>;
  ensureBoot(): Promise<void>;
  refreshTree(): void;
  expandNode(path: string, force?: boolean): Promise<void>;
  toggleNode(path: string): Promise<void>;
  retryNode(path: string): Promise<void>;
  expandPath(path: string): Promise<void>;
  resolveKind(entry: FileEntry): Promise<FileKind>;
  enqueueDownload(entry: FileEntry, kind: FileKind): void;
  cancelDownload(id: string): void;
  removeDownload(id: string): void;
  setMode(mode: ViewMode): void;
  setShowHidden(show: boolean): void;
  setIconScheme(scheme: IconScheme): void;
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

const VIEW_KEY = 'lg.files.view';

function loadViewMode(): ViewMode {
  if (typeof window === 'undefined') return 'details';
  try {
    const stored = window.localStorage.getItem(VIEW_KEY);
    return stored === 'grid' || stored === 'list' || stored === 'details' ? stored : 'details';
  } catch {
    return 'details';
  }
}

function saveViewMode(mode: ViewMode): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(VIEW_KEY, mode);
  } catch {
    // Private windows can reject storage; the choice then lasts for the session only.
  }
}

export function useFileSession(server: Server, client: SshBackend = ssh): FileSession {
  const homePath = ref('');
  const roots = ref<TreeState[]>([]);
  const downloads = ref<DownloadTask[]>([]);
  const iconScheme = ref<IconScheme>(loadIconScheme());
  const mode = ref<ViewMode>(loadViewMode());
  const showHidden = ref(false);
  const generation = ref(0);

  let cache = new TreeCache((path) => loadEntries(path));
  let bootPromise: Promise<void> | null = null;

  const offline = computed(() => server.status === 'offline' || server.status === 'error');
  const usable = computed(() => client.available && Boolean(server.sessionId));

  async function loadEntries(path: string): Promise<FileEntry[]> {
    const result = await client.sftpList(server.id, path);
    return result.entries;
  }

  async function ensureBoot(): Promise<void> {
    if (!usable.value) return;
    if (bootPromise) return bootPromise;
    bootPromise = (async () => {
      const home = await client.sftpHome(server.id);
      homePath.value = home;
      roots.value = buildRoots(home, server.disks.map((disk) => disk.mount));
      await expandPath(home);
    })().catch((caught) => {
      bootPromise = null;
      throw caught;
    });
    return bootPromise;
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

  function refreshTree(): void {
    cache.clear();
    resetTree(roots.value);
  }

  async function resolveKind(entry: FileEntry): Promise<FileKind> {
    if (entry.kind !== 'symlink') return entry.kind;
    const resolved = await client.sftpStat(server.id, entry.path);
    return resolved.targetKind ?? resolved.kind;
  }

  function enqueueDownload(entry: FileEntry, kind: FileKind): void {
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
    showHidden.value = show;
  }

  function setMode(next: ViewMode): void {
    if (mode.value === next) return;
    mode.value = next;
    saveViewMode(next);
  }

  function setIconScheme(scheme: IconScheme): void {
    if (iconScheme.value === scheme) return;
    iconScheme.value = scheme;
    saveIconScheme(scheme);
  }

  watch(
    () => server.sessionId,
    (next, previous) => {
      if (next === previous) return;
      cache = new TreeCache((path) => loadEntries(path));
      bootPromise = null;
      roots.value = [];
      homePath.value = '';
      generation.value += 1;
      if (next && usable.value) void ensureBoot().catch(() => {});
    },
  );

  return {
    usable,
    offline,
    homePath,
    roots,
    downloads,
    mode,
    showHidden,
    iconScheme,
    generation,
    ensureBoot,
    refreshTree,
    expandNode,
    toggleNode,
    retryNode,
    expandPath,
    resolveKind,
    enqueueDownload,
    cancelDownload,
    removeDownload,
    setMode,
    setShowHidden,
    setIconScheme,
  };
}
