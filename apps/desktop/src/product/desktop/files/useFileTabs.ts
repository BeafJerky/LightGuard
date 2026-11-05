import { computed, shallowRef, ref, type ComputedRef, type Ref, type ShallowRef } from 'vue';
import { ssh, type SshBackend } from '../../ssh';
import type { Server } from '../../types';
import { useFileSession, type FileSession } from './useFileSession';
import { useFiles } from './useFiles';

export const MAX_TABS = 8;

export interface FileTab {
  key: string;
  files: ReturnType<typeof useFiles>;
}

export interface FileTabs {
  session: FileSession;
  tabs: ShallowRef<FileTab[]>;
  activeKey: Ref<string>;
  active: ComputedRef<FileTab | null>;
  closed: Ref<boolean>;
  createTab(initialPath?: string): FileTab;
  closeTab(key: string): void;
  activate(key: string): void;
  cycle(delta: number): void;
}

export function useFileTabs(server: Server, client: SshBackend = ssh): FileTabs {
  const session = useFileSession(server, client);
  const tabs = shallowRef<FileTab[]>([]);
  const activeKey = ref('');
  const closed = ref(false);
  let counter = 0;

  const active = computed(() => tabs.value.find((tab) => tab.key === activeKey.value) ?? null);

  function createTab(initialPath?: string): FileTab {
    if (tabs.value.length >= MAX_TABS) {
      return (active.value ?? tabs.value[tabs.value.length - 1])!;
    }
    const tab: FileTab = {
      key: `file-${++counter}`,
      files: useFiles(server, client, session),
    };
    tabs.value = [...tabs.value, tab];
    activeKey.value = tab.key;
    closed.value = false;
    void tab.files.boot(initialPath);
    return tab;
  }

  function activate(key: string): void {
    if (!tabs.value.some((tab) => tab.key === key)) return;
    activeKey.value = key;
  }

  function closeTab(key: string): void {
    const index = tabs.value.findIndex((tab) => tab.key === key);
    if (index < 0) return;
    const wasActive = activeKey.value === key;
    const next = tabs.value.filter((tab) => tab.key !== key);
    tabs.value = next;
    if (!next.length) {
      activeKey.value = '';
      closed.value = true;
      return;
    }
    if (wasActive) activeKey.value = next[Math.max(0, index - 1)].key;
  }

  function cycle(delta: number): void {
    if (tabs.value.length < 2) return;
    const index = tabs.value.findIndex((tab) => tab.key === activeKey.value);
    const next = (index + delta + tabs.value.length) % tabs.value.length;
    activeKey.value = tabs.value[next].key;
  }

  createTab();

  return { session, tabs, activeKey, active, closed, createTab, closeTab, activate, cycle };
}
