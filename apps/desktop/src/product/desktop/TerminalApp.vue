<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { LgButton, LgIcon, LgInput, LgTerminal } from '@lightguard/lightui';
import { backendAvailable, ssh, toSshError } from '../ssh';
import type { TerminalEvent } from '../ssh';
import { retryServer } from '../store';
import { toast } from '../ui';
import type { Server } from '../types';

const props = defineProps<{
  server: Server;
}>();

type TabStatus = 'connecting' | 'connected' | 'closed';

interface TerminalHandle {
  focus(): void;
  write(text: string): void;
  search(term: string, direction?: 'next' | 'previous'): void;
  clearSearch(): void;
  terminal(): { getSelection(): string } | null;
}

interface TerminalTab {
  key: string;
  terminalId: string;
  title: string;
  status: TabStatus;
  detail: string;
  cols: number;
  rows: number;
  pending: string;
}

const MAX_TABS = 8;
const root = ref<HTMLElement | null>(null);
const tabs = ref<TerminalTab[]>([]);
const activeKey = ref('');
const handles = new Map<string, TerminalHandle>();
const searchOpen = ref(false);
const searchTerm = ref('');
const restoring = ref(false);
const menu = reactive({ open: false, x: 0, y: 0 });

let seq = 0;

const defaultTitle = computed(() => `${props.server.user}@${props.server.hostname || props.server.name}`);
const activeTab = computed(() => tabs.value.find((tab) => tab.key === activeKey.value) ?? null);
const canUse = computed(() => backendAvailable);
const online = computed(() => props.server.status === 'online' || props.server.status === 'warning');

function isOnline() {
  return online.value;
}

function statusLabel(status: TabStatus) {
  if (status === 'connecting') return '连接中';
  if (status === 'connected') return '已连接';
  return '已断开';
}

function setTerminalRef(key: string, el: unknown) {
  if (!el) {
    handles.delete(key);
    return;
  }
  handles.set(key, el as TerminalHandle);
  const tab = tabs.value.find((item) => item.key === key);
  if (tab?.pending) {
    handles.get(key)?.write(tab.pending);
    tab.pending = '';
  }
}

function writeTab(tab: TerminalTab, data: string) {
  const handle = handles.get(tab.key);
  if (handle) handle.write(data);
  else tab.pending += data;
}

function handleEvent(tab: TerminalTab, event: TerminalEvent) {
  if (event.type === 'data') {
    writeTab(tab, event.data);
  } else if (event.type === 'title') {
    const title = event.title.trim();
    if (title) tab.title = title.slice(0, 80);
  } else if (event.type === 'exit') {
    tab.status = 'closed';
    tab.detail = event.code === null ? '远端 Shell 已退出' : `远端 Shell 已退出（code ${event.code}）`;
  } else {
    tab.status = 'closed';
    tab.detail = event.reason || '连接已断开';
  }
}

async function openTerminal(tab: TerminalTab) {
  tab.status = 'connecting';
  tab.detail = '';
  try {
    const opened = await ssh.terminalOpen({
      serverId: props.server.id,
      cols: tab.cols,
      rows: tab.rows,
      onEvent: (event) => handleEvent(tab, event),
    });
    tab.terminalId = opened.terminalId;
    if (!tab.title) tab.title = opened.title || defaultTitle.value;
    tab.status = 'connected';
    await ssh.terminalResize(props.server.id, opened.terminalId, tab.cols, tab.rows).catch(() => undefined);
    await nextTick();
    handles.get(tab.key)?.focus();
  } catch (raw) {
    const error = toSshError(raw);
    tab.status = 'closed';
    tab.detail = [error.message, error.hint].filter(Boolean).join(' · ');
  }
}

async function newTab() {
  if (!canUse.value) return;
  if (tabs.value.length >= MAX_TABS) {
    toast({ title: '标签已达上限', message: `最多同时打开 ${MAX_TABS} 个终端标签。`, tone: 'warning' });
    return;
  }
  if (!isOnline()) {
    toast({ title: '会话未连接', message: '请先在控制台重新连接该服务器。', tone: 'warning' });
    return;
  }
  seq += 1;
  const base = defaultTitle.value;
  const existing = tabs.value.filter((tab) => tab.title.startsWith(base)).length;
  const tab: TerminalTab = {
    key: `term-${seq}`,
    terminalId: '',
    title: existing > 0 ? `${base} · ${existing + 1}` : base,
    status: 'connecting',
    detail: '',
    cols: 100,
    rows: 28,
    pending: '',
  };
  tabs.value.push(tab);
  const created = tabs.value[tabs.value.length - 1];
  activeKey.value = created.key;
  searchOpen.value = false;
  await nextTick();
  await openTerminal(created);
}

async function closeTab(tab: TerminalTab) {
  const index = tabs.value.findIndex((item) => item.key === tab.key);
  if (index < 0) return;
  tabs.value.splice(index, 1);
  handles.delete(tab.key);
  if (activeKey.value === tab.key) {
    const next = tabs.value[index] ?? tabs.value[index - 1] ?? null;
    activeKey.value = next?.key ?? '';
  }
  if (ssh.available && tab.terminalId) {
    await ssh.terminalClose(props.server.id, tab.terminalId).catch(() => undefined);
  }
}

function activate(tab: TerminalTab) {
  activeKey.value = tab.key;
  nextTick(() => handles.get(tab.key)?.focus());
}

function cycleTab(step: number) {
  if (tabs.value.length < 2) return;
  const index = tabs.value.findIndex((tab) => tab.key === activeKey.value);
  const next = (index + step + tabs.value.length) % tabs.value.length;
  activate(tabs.value[next]);
}

function sendTo(tab: TerminalTab, data: string) {
  if (tab.status === 'connected' && tab.terminalId) {
    void ssh.terminalWrite(props.server.id, tab.terminalId, data).catch(() => undefined);
  } else {
    writeTab(tab, data);
  }
}

function send(data: string) {
  const tab = activeTab.value;
  if (tab) sendTo(tab, data);
}

function clearScreen() {
  send('\u000c');
  handles.get(activeKey.value)?.focus();
}

async function copySelection() {
  const text = handles.get(activeKey.value)?.terminal()?.getSelection() ?? '';
  if (!text) {
    toast({ title: '没有选中内容', tone: 'warning', duration: 1600 });
    return;
  }
  try {
    await navigator.clipboard.writeText(text);
    toast({ title: '已复制选中内容', tone: 'success', duration: 1600 });
  } catch {
    toast({ title: '复制失败', message: '当前环境不允许访问剪贴板。', tone: 'warning' });
  }
}

async function pasteClipboard() {
  try {
    const text = await navigator.clipboard.readText();
    if (text) send(text);
  } catch {
    toast({ title: '粘贴失败', message: '当前环境不允许读取剪贴板。', tone: 'warning' });
  }
}

function applySearch(direction: 'next' | 'previous' = 'next') {
  const handle = handles.get(activeKey.value);
  if (!handle) return;
  if (!searchTerm.value) {
    handle.clearSearch();
    return;
  }
  handle.search(searchTerm.value, direction);
}

function toggleSearch() {
  searchOpen.value = !searchOpen.value;
  if (!searchOpen.value) {
    searchTerm.value = '';
    handles.get(activeKey.value)?.clearSearch();
  }
}

async function reopen(tab: TerminalTab) {
  writeTab(tab, `\u001b[2J\u001b[H\u001b[33m[LightGuard] 正在重新打开终端…\u001b[0m\r\n`);
  await openTerminal(tab);
}

async function reconnectTab(tab: TerminalTab) {
  if (restoring.value) return;
  restoring.value = true;
  try {
    if (!isOnline()) {
      const updated = await retryServer(props.server.id);
      if (!updated || (updated.status !== 'online' && updated.status !== 'warning')) {
        toast({
          title: '会话恢复失败',
          message: updated?.error?.message ?? '请检查网络与凭据后重试。',
          tone: 'error',
        });
        return;
      }
    }
    await reopen(tab);
    toast({ title: '会话已恢复', message: `${props.server.name} 的终端已重新打开`, tone: 'success' });
  } finally {
    restoring.value = false;
  }
}

function onKey(event: KeyboardEvent) {
  if (!root.value) return;
  const target = document.activeElement;
  if (!target || !root.value.contains(target)) return;

  const ctrl = event.ctrlKey || event.metaKey;
  if (ctrl && event.shiftKey && event.key.toLowerCase() === 't') {
    event.preventDefault();
    void newTab();
    return;
  }
  if (ctrl && event.shiftKey && event.key.toLowerCase() === 'w') {
    event.preventDefault();
    if (activeTab.value) void closeTab(activeTab.value);
    return;
  }
  if (ctrl && event.key === 'Tab') {
    event.preventDefault();
    cycleTab(event.shiftKey ? -1 : 1);
    return;
  }
  if (ctrl && event.shiftKey && event.key.toLowerCase() === 'f') {
    event.preventDefault();
    toggleSearch();
    return;
  }
  if (ctrl && event.shiftKey && event.key.toLowerCase() === 'c') {
    event.preventDefault();
    void copySelection();
    return;
  }
  if (ctrl && event.shiftKey && event.key.toLowerCase() === 'v') {
    event.preventDefault();
    void pasteClipboard();
  }
}

function openMenu(event: MouseEvent) {
  if (!activeTab.value) return;
  event.preventDefault();
  const rect = root.value?.getBoundingClientRect();
  menu.open = true;
  menu.x = event.clientX - (rect?.left ?? 0);
  menu.y = event.clientY - (rect?.top ?? 0);
}

function closeMenu() {
  menu.open = false;
}

function onResize(tab: TerminalTab, cols: number, rows: number) {
  tab.cols = cols;
  tab.rows = rows;
  if (ssh.available && tab.terminalId) {
    void ssh.terminalResize(props.server.id, tab.terminalId, cols, rows).catch(() => undefined);
  }
}

function runMenu(action: 'copy' | 'paste' | 'clear' | 'search') {
  closeMenu();
  if (action === 'copy') void copySelection();
  else if (action === 'paste') void pasteClipboard();
  else if (action === 'clear') clearScreen();
  else toggleSearch();
}

watch(
  () => props.server.status,
  async (status, previous) => {
    if (!root.value) return;
    const recovered =
      (status === 'online' || status === 'warning') &&
      (previous === 'error' || previous === 'offline' || previous === 'connecting');
    if (!recovered) return;
    const closedTabs = tabs.value.filter((tab) => tab.status === 'closed');
    if (!closedTabs.length) return;
    toast({ title: '会话已恢复', message: '正在重新打开已断开的终端标签', tone: 'info' });
    for (const tab of closedTabs) await reopen(tab);
  },
);

onMounted(() => {
  window.addEventListener('keydown', onKey, true);
  window.addEventListener('click', closeMenu);
  if (canUse.value && isOnline()) void newTab();
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey, true);
  window.removeEventListener('click', closeMenu);
  if (!ssh.available) return;
  for (const tab of tabs.value) {
    if (tab.terminalId) void ssh.terminalClose(props.server.id, tab.terminalId).catch(() => undefined);
  }
});
</script>

<template>
  <div ref="root" class="lgapp-terminal" @contextmenu="openMenu">
    <div class="lgapp-toolbar">
      <span class="lgapp-terminal__note">
        <LgIcon name="terminal" :size="13" />
        {{ server.user }}@{{ server.name }} · SSH 终端
      </span>
      <div class="lgapp-actions">
        <LgButton size="tiny" secondary :disabled="!canUse || !online || tabs.length >= MAX_TABS" @click="newTab">
          <template #icon><LgIcon name="plus" :size="12" /></template>
          新建标签
        </LgButton>
        <LgButton size="tiny" secondary :disabled="!activeTab" @click="toggleSearch">
          <template #icon><LgIcon name="search" :size="12" /></template>
          搜索
        </LgButton>
        <LgButton size="tiny" secondary :disabled="!activeTab" @click="clearScreen">
          <template #icon><LgIcon name="trash" :size="12" /></template>
          清屏
        </LgButton>
      </div>
    </div>

    <div v-if="!canUse" class="lgapp-empty">
      <strong>真实 SSH 仅桌面版可用</strong>
      <span>请在 LightGuard 桌面客户端中连接服务器后使用终端。</span>
    </div>

    <template v-else>
      <div v-if="tabs.length" class="term-tabs" role="tablist" aria-label="终端标签">
        <div
          v-for="tab in tabs"
          :key="tab.key"
          class="term-tab"
          :class="{ 'is-active': tab.key === activeKey, [`is-${tab.status}`]: true }"
          role="tab"
          :aria-selected="tab.key === activeKey"
          @click="activate(tab)"
        >
          <span class="term-tab__dot" aria-hidden="true" />
          <span class="term-tab__title" :title="tab.title">{{ tab.title }}</span>
          <span class="term-tab__state">{{ statusLabel(tab.status) }}</span>
          <button type="button" class="term-tab__close" aria-label="关闭标签" @click.stop="closeTab(tab)">
            <LgIcon name="x" :size="11" />
          </button>
        </div>
      </div>

      <div v-if="searchOpen" class="term-search">
        <LgInput
          v-model:value="searchTerm"
          size="small"
          placeholder="在当前终端中搜索"
          clearable
          @update:value="applySearch('next')"
          @keydown.enter.exact.prevent="applySearch('next')"
          @keydown.shift.enter.prevent="applySearch('previous')"
        >
          <template #prefix><LgIcon name="search" :size="13" /></template>
        </LgInput>
        <LgButton size="tiny" secondary @click="applySearch('previous')">
          <LgIcon name="chevron-up" :size="12" />
        </LgButton>
        <LgButton size="tiny" secondary @click="applySearch('next')">
          <LgIcon name="chevron-down" :size="12" />
        </LgButton>
        <LgButton size="tiny" secondary @click="toggleSearch">
          <LgIcon name="x" :size="12" />
        </LgButton>
      </div>

      <div v-if="!tabs.length" class="lgapp-empty">
        <strong>{{ online ? '尚未打开终端' : '会话未连接' }}</strong>
        <span>{{ online ? '点击「新建标签」打开一个交互式 Shell。' : '请先在控制台重新连接该服务器。' }}</span>
      </div>

      <div v-else class="term-panes">
        <div v-for="tab in tabs" v-show="tab.key === activeKey" :key="tab.key" class="term-pane">
          <LgTerminal
            :ref="(el) => setTerminalRef(tab.key, el)"
            height="100%"
            :label="tab.title"
            :user="server.user"
            :hostname="server.hostname || server.name"
            :on-data="(data: string) => sendTo(tab, data)"
            :on-resize="(cols: number, rows: number) => onResize(tab, cols, rows)"
            @click="menu.open = false"
          />
          <div v-if="tab.status === 'closed'" class="term-overlay">
            <strong>终端已断开</strong>
            <p>{{ tab.detail || '连接已结束，最后画面已保留。' }}</p>
            <LgButton size="small" type="primary" :disabled="restoring" @click="reconnectTab(tab)">
              <template #icon><LgIcon name="refresh" :size="13" /></template>
              {{ restoring ? '重连中…' : '重连会话' }}
            </LgButton>
          </div>
          <div v-else-if="tab.status === 'connecting' && !tab.terminalId" class="term-overlay is-quiet">
            <strong>正在打开终端…</strong>
            <p>申请 PTY 并启动远端 Shell。</p>
          </div>
        </div>
      </div>

      <p class="lgapp-terminal__note term-hint">
        <LgIcon name="info" :size="12" />
        Ctrl+Shift+T 新建 · Ctrl+Shift+W 关闭 · Ctrl+Tab 切换 · Ctrl+Shift+F 搜索 · Ctrl+Shift+C/V 复制粘贴
      </p>
    </template>

    <div v-if="menu.open" class="term-menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @click.stop>
      <button type="button" @click="runMenu('copy')"><LgIcon name="copy" :size="12" /> 复制</button>
      <button type="button" @click="runMenu('paste')"><LgIcon name="clipboard" :size="12" /> 粘贴</button>
      <button type="button" @click="runMenu('search')"><LgIcon name="search" :size="12" /> 搜索</button>
      <button type="button" @click="runMenu('clear')"><LgIcon name="trash" :size="12" /> 清屏</button>
    </div>
  </div>
</template>

<style scoped>
.lgapp-terminal {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  padding: 14px;
}

.lgapp-terminal .lgapp-actions {
  margin-left: auto;
}

.term-tabs {
  display: flex;
  gap: 4px;
  overflow-x: auto;
  padding-bottom: 2px;
}

.term-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 220px;
  padding: 4px 6px 4px 9px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: 11px;
  cursor: pointer;
  white-space: nowrap;
}

.term-tab.is-active {
  border-color: color-mix(in srgb, var(--lg-primary) 45%, transparent);
  background: var(--lg-primary-soft);
  color: var(--lg-text);
}

.term-tab__dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--lg-text-muted);
}

.term-tab.is-connected .term-tab__dot {
  background: var(--lg-success);
}

.term-tab.is-connecting .term-tab__dot {
  background: var(--lg-warning);
}

.term-tab.is-closed .term-tab__dot {
  background: var(--lg-danger);
}

.term-tab__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.term-tab__state {
  color: var(--lg-text-muted);
  font-size: 10px;
}

.term-tab__close {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.term-tab__close:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.term-search {
  display: flex;
  align-items: center;
  gap: 6px;
}

.term-search .lg-input {
  flex: 1;
}

.term-panes {
  position: relative;
  display: flex;
  flex: 1;
  width: 100%;
  min-width: 0;
  min-height: 0;
}

.term-pane {
  position: relative;
  display: flex;
  flex: 1;
  flex-direction: column;
  width: 100%;
  min-width: 0;
  min-height: 0;
}

.term-overlay {
  position: absolute;
  inset: 32px 1px 1px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 16px;
  border-radius: 0 0 var(--lg-radius-md) var(--lg-radius-md);
  background: color-mix(in srgb, var(--lg-bg) 82%, transparent);
  backdrop-filter: blur(3px);
  text-align: center;
}

.term-overlay.is-quiet {
  background: color-mix(in srgb, var(--lg-bg) 55%, transparent);
}

.term-overlay strong {
  font-size: 13px;
}

.term-overlay p {
  margin: 0;
  max-width: 360px;
  color: var(--lg-text-secondary);
  font-size: 11px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}

.term-hint {
  margin: 0;
}

.term-menu {
  position: absolute;
  z-index: 40;
  display: flex;
  flex-direction: column;
  min-width: 132px;
  padding: 4px;
  border: 1px solid var(--lg-border-strong);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface-glass);
  box-shadow: var(--lg-shadow-lg);
  backdrop-filter: blur(12px);
}

.term-menu button {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 9px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: 11px;
  text-align: left;
  cursor: pointer;
}

.term-menu button:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}
</style>
