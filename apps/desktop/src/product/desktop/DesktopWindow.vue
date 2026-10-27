<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import DesktopAppHost from './DesktopAppHost.vue';
import { appMeta, type DesktopAppKey } from './apps';
import type { DesktopWindowState } from './useDesktopWindows';
import type { Server } from '../types';

const props = defineProps<{
  win: DesktopWindowState;
  active: boolean;
  server: Server;
  session: string;
  startedAt: string;
}>();

const emit = defineEmits<{
  focus: [];
  minimize: [];
  maximize: [];
  close: [];
  move: [x: number, y: number];
  disconnect: [];
}>();

const root = ref<HTMLElement | null>(null);
const meta = computed(() => appMeta(props.win.app));
const terminalHeight = computed(() => Math.max(260, props.win.h - 122));

const windowStyle = computed(() =>
  props.win.maximized
    ? { zIndex: props.win.z }
    : {
        left: `${props.win.x}px`,
        top: `${props.win.y}px`,
        width: `${props.win.w}px`,
        height: `${props.win.h}px`,
        zIndex: props.win.z,
      },
);

let drag: { pointerId: number; startX: number; startY: number; originX: number; originY: number } | null = null;

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), Math.max(min, max));
}

function onPointerMove(event: PointerEvent) {
  if (!drag) return;
  const bounds = root.value?.parentElement?.getBoundingClientRect();
  const width = bounds?.width ?? window.innerWidth;
  const height = bounds?.height ?? window.innerHeight;
  const x = clamp(drag.originX + event.clientX - drag.startX, 0, Math.max(0, width - props.win.w));
  const y = clamp(drag.originY + event.clientY - drag.startY, 0, Math.max(0, height - props.win.h - 54));
  emit('move', Math.round(x), Math.round(y));
}

function stopDrag() {
  drag = null;
  window.removeEventListener('pointermove', onPointerMove);
  window.removeEventListener('pointerup', stopDrag);
  window.removeEventListener('pointercancel', stopDrag);
}

function onTitlePointerDown(event: PointerEvent) {
  if (event.button !== 0 || props.win.maximized) return;
  if ((event.target as HTMLElement).closest('.lgdesk-win__controls')) return;
  emit('focus');
  drag = {
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    originX: props.win.x,
    originY: props.win.y,
  };
  window.addEventListener('pointermove', onPointerMove);
  window.addEventListener('pointerup', stopDrag);
  window.addEventListener('pointercancel', stopDrag);
}

function onTitleDoubleClick(event: MouseEvent) {
  if ((event.target as HTMLElement).closest('.lgdesk-win__controls')) return;
  emit('maximize');
}

function onWindowKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape') return;
  emit('close');
}

onBeforeUnmount(stopDrag);
</script>

<template>
  <section
    ref="root"
    class="lgdesk-win"
    :class="{
      'is-active': active,
      'is-maximized': win.maximized,
      'is-minimized': win.minimized,
    }"
    :style="windowStyle"
    :aria-label="meta.title"
    role="dialog"
    :aria-modal="false"
    tabindex="-1"
    @pointerdown="emit('focus')"
    @keydown="onWindowKeydown"
  >
    <header
      class="lgdesk-win__bar"
      @pointerdown="onTitlePointerDown"
      @dblclick="onTitleDoubleClick"
    >
      <span class="lgdesk-win__icon" :class="meta.tile" aria-hidden="true">
        <LgIcon :name="meta.icon" :size="14" />
      </span>
      <div class="lgdesk-win__title">
        {{ meta.title }}
        <span>{{ server.ip }}</span>
      </div>
      <div class="lgdesk-win__controls">
        <button type="button" class="lgdesk-win__ctrl" aria-label="最小化" @click.stop="emit('minimize')">
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 8h10" /></svg>
        </button>
        <button
          type="button"
          class="lgdesk-win__ctrl"
          :aria-label="win.maximized ? '还原' : '最大化'"
          @click.stop="emit('maximize')"
        >
          <LgIcon :name="win.maximized ? 'copy' : 'maximize'" :size="13" />
        </button>
        <button type="button" class="lgdesk-win__ctrl is-close" aria-label="关闭" @click.stop="emit('close')">
          <LgIcon name="x" :size="13" />
        </button>
      </div>
    </header>

    <div class="lgdesk-win__body">
      <DesktopAppHost
        :app="win.app"
        :server="server"
        :session="session"
        :started-at="startedAt"
        :terminal-height="terminalHeight"
        @disconnect="emit('disconnect')"
      />
    </div>
  </section>
</template>

<style scoped>
.lgdesk-win {
  position: absolute;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: var(--lg-border-width) solid var(--lg-border-strong);
  border-radius: var(--lg-radius-md);
  background: var(--lg-bg);
  box-shadow: var(--lg-shadow-lg);
  outline: none;
  transition:
    box-shadow var(--lg-dur-base) var(--lg-ease),
    opacity var(--lg-dur-base) var(--lg-ease);
}

.lgdesk-win.is-active {
  border-color: color-mix(in srgb, var(--lg-primary) 36%, var(--lg-border-strong));
}

.lgdesk-win.is-minimized {
  display: none;
}

.lgdesk-win.is-maximized {
  inset: 6px 6px 60px 6px;
  width: auto;
  height: auto;
  border-radius: var(--lg-radius-sm);
}

.lgdesk-win__bar {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 6px 0 8px;
  border-bottom: var(--lg-border-width) solid var(--lg-border);
  background: var(--lg-surface-2);
  cursor: grab;
  touch-action: none;
}

.lgdesk-win__bar:active {
  cursor: grabbing;
}

.lgdesk-win__icon {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 22px;
  height: 22px;
  border-radius: 6px;
}

.lgdesk-win__title {
  display: flex;
  flex: 1;
  align-items: baseline;
  gap: 7px;
  min-width: 0;
  overflow: hidden;
  color: var(--lg-text);
  font-size: 11.5px;
  font-weight: 600;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.lgdesk-win__title span {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 9.5px;
  font-weight: 400;
}

.lgdesk-win__controls {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 1px;
}

.lgdesk-win__ctrl {
  display: grid;
  place-items: center;
  width: 26px;
  height: 24px;
  padding: 0;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.lgdesk-win__ctrl:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.lgdesk-win__ctrl.is-close:hover {
  background: var(--lg-danger);
  color: #fff;
}

.lgdesk-win__ctrl svg {
  width: 12px;
  height: 12px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.6;
  stroke-linecap: round;
}

.lgdesk-win__body {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: auto;
  background: var(--lg-bg);
}

@media (max-width: 719px) {
  .lgdesk-win,
  .lgdesk-win.is-maximized {
    inset: 6px 6px 62px 6px;
    width: auto !important;
    height: auto !important;
    border-radius: var(--lg-radius-sm);
  }
}
</style>
