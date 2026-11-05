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
  type MoveAction,
  type SortKey,
  type ViewMode,
} from './listModel';
import { useFileSession, type FileSession } from './useFileSession';
import { basename, parentOf } from './remotePath';

export type { DownloadStatus, DownloadTask, TreeState } from './useFileSession';

export type PreviewMode = 'none' | 'meta' | 'text' | 'image';

export interface PreviewState {
  mode: PreviewMode;
  entry: FileEntry | null;
  text: TextPreview | null;
  image: ImagePreview | null;
  loading: boolean;
  error: SshCommandError | null;
}

export function useFiles(server: Server, client: SshBackend = ssh, shared?: FileSession) {
  const session = shared ?? useFileSession(server, client);
  const { usable, offline, homePath, roots, downloads, iconScheme } = session;

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
  const view = reactive<ListViewState>({
    sortKey: 'name',
    sortDir: 'asc',
    query: '',
    get showHidden() {
      return session.showHidden.value;
    },
    set showHidden(next: boolean) {
      session.setShowHidden(next);
    },
    get mode() {
      return session.mode.value;
    },
    set mode(next: ViewMode) {
      session.setMode(next);
    },
  });

  let navToken = 0;
  let previewToken = 0;
  let openToken = 0;

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

  async function boot(initialPath?: string): Promise<void> {
    if (!usable.value) return;
    try {
      await session.ensureBoot();
      const target = initialPath || homePath.value;
      if (!target) return;
      await navigate(target);
      if (currentPath.value) await session.expandPath(currentPath.value);
    } catch (caught) {
      error.value = toSshError(caught);
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

  async function open(entry: FileEntry): Promise<void> {
    select(entry);
    const token = openToken;
    let kind: FileKind;
    try {
      kind = await session.resolveKind(entry);
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
        await session.expandPath(currentPath.value);
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
    if (previewOpen.value) {
      closePreview();
      return;
    }
    previewOpen.value = true;
    if (preview.entry && preview.mode === 'none') {
      void loadPreview(preview.entry);
    }
  }

  function closePreview(): void {
    previewOpen.value = false;
    previewToken += 1;
    preview.loading = false;
    // A closed pane must not keep the previous file's mode, or reopening it
    // skips loadPreview and keeps showing the placeholder meta view.
    preview.mode = 'none';
  }

  async function refresh(): Promise<void> {
    if (!currentPath.value) {
      await boot();
      return;
    }
    session.refreshTree();
    await navigate(currentPath.value, false);
    if (currentPath.value) await session.expandPath(currentPath.value);
  }

  async function selectTree(path: string): Promise<void> {
    await navigate(path);
    await session.expandPath(currentPath.value);
  }

  async function openTree(path: string): Promise<void> {
    await session.toggleNode(path);
    await navigate(path);
  }

  function toggleSortView(key: SortKey): void {
    const next = toggleSort(view, key);
    view.sortKey = next.sortKey;
    view.sortDir = next.sortDir;
  }

  async function download(entry: FileEntry): Promise<void> {
    try {
      const kind = await session.resolveKind(entry);
      session.enqueueDownload(entry, kind);
    } catch (caught) {
      error.value = toSshError(caught);
    }
  }

  watch(
    () => session.generation.value,
    () => {
      navToken += 1;
      previewToken += 1;
      openToken += 1;
      currentPath.value = '';
      listing.value = null;
      entries.value = [];
      selectedPath.value = '';
      history.value = [];
      historyIndex.value = -1;
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
      if (canUse.value) void boot();
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
    iconScheme,
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
    toggleNode: session.toggleNode,
    retryNode: session.retryNode,
    toggleSort: toggleSortView,
    togglePreview,
    closePreview,
    loadPreview,
    download,
    cancelDownload: session.cancelDownload,
    removeDownload: session.removeDownload,
    setShowHidden: session.setShowHidden,
    setMode: session.setMode,
    setIconScheme: session.setIconScheme,
    selectTree,
    openTree,
    resolveKind: session.resolveKind,
    basename,
    moveSelection: (action: MoveAction, page: number, columns = 1) =>
      moveSelection(visibleRows.value, selectedPath.value, action, page, columns),
  };
}
