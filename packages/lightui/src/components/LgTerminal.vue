<script setup lang="ts">
import { computed, inject, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { SearchAddon } from '@xterm/addon-search';
import { WebglAddon } from '@xterm/addon-webgl';
import { usePreferredReducedMotion } from '@vueuse/core';
import '@xterm/xterm/css/xterm.css';
import { LG_SCOPE_KEY } from '../theme/context';
import { useRenderBackend } from '../render/useRenderBackend';
import { toXtermTheme } from '../terminal/theme';
import { createSimulatedShell, type SimulatedShell } from '../terminal/simulator';

const props = withDefaults(
  defineProps<{
    height?: number | string;
    label?: string;
    user?: string;
    hostname?: string;
    readonly?: boolean;
    simulated?: boolean;
    onData?: (data: string) => void;
    onResize?: (cols: number, rows: number) => void;
  }>(),
  { height: 360, label: '终端', user: 'ubuntu', hostname: 'web-prod-01', readonly: false, simulated: false },
);

const backend = useRenderBackend('terminal');
const scope = inject(LG_SCOPE_KEY, null);
const reduced = usePreferredReducedMotion();

const host = ref<HTMLDivElement | null>(null);
const focused = ref(false);
const webglActive = ref(false);
const webglFailed = ref(false);

let terminal: Terminal | null = null;
let fit: FitAddon | null = null;
let searchAddon: SearchAddon | null = null;
let webgl: WebglAddon | null = null;
let shell: SimulatedShell | null = null;
let observer: ResizeObserver | null = null;
let offData: { dispose: () => void } | null = null;
let offResize: { dispose: () => void } | null = null;
let frame = 0;

const rendererLabel = computed(() => {
  if (webglActive.value) return 'WebGL';
  return webglFailed.value ? 'DOM · WebGL 回退' : 'DOM';
});

const rootStyle = computed(() => ({
  height: typeof props.height === 'number' ? `${props.height}px` : props.height,
  '--lg-terminal-bg': scope?.value.theme.color.bg ?? '#000',
}));

function applyTheme() {
  const theme = scope?.value.theme;
  if (!terminal || !theme) return;
  terminal.options.theme = toXtermTheme(theme);
  terminal.options.fontFamily = theme.font.mono;
}

function fitSoon() {
  if (frame) cancelAnimationFrame(frame);
  frame = requestAnimationFrame(() => {
    frame = 0;
    try {
      fit?.fit();
    } catch {
      // 容器尚未完成布局时 fit 会抛错，下一次 ResizeObserver 会补上。
    }
  });
}

function dropWebgl() {
  if (!webgl) return;
  try {
    webgl.dispose();
  } catch {
    // dispose 期间上下文可能已丢失。
  }
  webgl = null;
  webglActive.value = false;
  terminal?.refresh(0, terminal.rows - 1);
}

function syncRenderer() {
  if (!terminal) return;
  if (backend.value === 'webgl') {
    if (webgl || webglFailed.value) return;
    try {
      const addon = new WebglAddon();
      addon.onContextLoss(() => {
        webglFailed.value = true;
        dropWebgl();
      });
      terminal.loadAddon(addon);
      webgl = addon;
      webglActive.value = true;
    } catch {
      webglFailed.value = true;
      webglActive.value = false;
    }
  } else {
    dropWebgl();
  }
  fitSoon();
}

function focusTerminal() {
  terminal?.focus();
}

function onFocus() {
  focused.value = true;
}

function onBlur() {
  focused.value = false;
}

onMounted(() => {
  const theme = scope?.value.theme;
  if (!host.value || !theme) return;

  terminal = new Terminal({
    fontFamily: theme.font.mono,
    fontSize: 13,
    lineHeight: 1.38,
    cursorBlink: reduced.value !== 'reduce',
    cursorStyle: 'bar',
    scrollback: 2000,
    screenReaderMode: true,
    allowTransparency: false,
    theme: toXtermTheme(theme),
  });
  fit = new FitAddon();
  terminal.loadAddon(fit);
  searchAddon = new SearchAddon();
  terminal.loadAddon(searchAddon);
  terminal.open(host.value);
  terminal.textarea?.setAttribute('aria-label', props.label);

  if (props.simulated && !props.onData) {
    shell = createSimulatedShell({
      user: props.user,
      hostname: props.hostname,
      write: (text) => terminal?.write(text),
    });
    shell.start();
  }

  offData = terminal.onData((data) => {
    if (props.readonly) return;
    if (props.onData) props.onData(data);
    else shell?.input(data);
  });

  offResize = terminal.onResize(({ cols, rows }) => {
    props.onResize?.(cols, rows);
  });

  terminal.textarea?.addEventListener('focus', onFocus);
  terminal.textarea?.addEventListener('blur', onBlur);

  fitSoon();
  syncRenderer();

  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(() => fitSoon());
    observer.observe(host.value);
  }
});

onBeforeUnmount(() => {
  if (frame) cancelAnimationFrame(frame);
  observer?.disconnect();
  observer = null;
  offData?.dispose();
  offData = null;
  offResize?.dispose();
  offResize = null;
  terminal?.textarea?.removeEventListener('focus', onFocus);
  terminal?.textarea?.removeEventListener('blur', onBlur);
  dropWebgl();
  terminal?.dispose();
  terminal = null;
  shell = null;
  fit = null;
  searchAddon = null;
});

watch(
  () => scope?.value.theme.id,
  () => {
    applyTheme();
    fitSoon();
    nextTick(() => terminal?.refresh(0, terminal.rows - 1));
  },
);

watch(backend, syncRenderer);

watch(
  () => reduced.value,
  (value) => {
    if (terminal) terminal.options.cursorBlink = value !== 'reduce';
  },
);

defineExpose({
  focus: focusTerminal,
  write: (text: string) => terminal?.write(text),
  search: (term: string, direction: 'next' | 'previous' = 'next') => {
    if (!term) {
      searchAddon?.clearDecorations();
      return;
    }
    if (direction === 'previous') searchAddon?.findPrevious(term);
    else searchAddon?.findNext(term);
  },
  clearSearch: () => searchAddon?.clearDecorations(),
  terminal: () => terminal,
});
</script>

<template>
  <div
    class="lg-terminal"
    :class="{ 'is-focused': focused, 'is-webgl': webglActive }"
    :style="rootStyle"
    :data-lg-render="webglActive ? 'webgl' : 'dom'"
  >
    <header class="lg-terminal__bar">
      <span class="lg-terminal__title">
        <span class="lg-terminal__led" aria-hidden="true" />
        {{ label }}
      </span>
      <span class="lg-terminal__meta">
        <span class="lg-terminal__badge">{{ rendererLabel }}</span>
        <span class="lg-terminal__host">{{ user }}@{{ hostname }}</span>
      </span>
    </header>
    <div ref="host" class="lg-terminal__host" @click="focusTerminal" />
  </div>
</template>

<style scoped>
.lg-terminal {
  display: flex;
  flex-direction: column;
  width: 100%;
  min-width: 0;
  overflow: hidden;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-bg);
}

.lg-terminal.is-focused {
  border-color: var(--lg-border-strong);
}

.lg-terminal__bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 6px 10px;
  border-bottom: var(--lg-border-width) solid var(--lg-border);
  background: var(--lg-surface-2);
  font-size: var(--lg-fs-sm);
}

.lg-terminal__title {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--lg-text-secondary);
}

.lg-terminal__led {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--lg-success);
  box-shadow: 0 0 0 3px var(--lg-success-soft);
}

.lg-terminal__meta {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  color: var(--lg-text-muted);
}

.lg-terminal__badge {
  padding: 1px 7px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  font-size: var(--lg-fs-label);
  letter-spacing: 0.03em;
}

.is-webgl .lg-terminal__badge {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.lg-terminal__host {
  flex: 1;
  min-height: 0;
  cursor: text;
}

.lg-terminal__host :deep(.xterm) {
  height: 100%;
  padding: 8px 10px;
}

.lg-terminal__host :deep(.xterm-viewport) {
  /* xterm 自带 CSS 把 viewport 固定为纯黑，这里跟随当前主题背景。 */
  background-color: var(--lg-terminal-bg);
  scrollbar-width: thin;
  scrollbar-color: var(--lg-border-strong) transparent;
}
</style>
