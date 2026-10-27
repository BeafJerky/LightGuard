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
  resize: [x: number, y: number, w: number, h: number];
  disconnect: [];
}>();

const root = ref<HTMLElement | null>(null);
const meta = computed(() => appMeta(props.win.app));

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
let resizeDrag: {
  pointerId: number;
  startX: number;
  startY: number;
  originX: number;
  originY: number;
  originW: number;
  originH: number;
  direction: string;
} | null = null;

const MIN_W = 360;
const MIN_H = 240;

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

function onResizeMove(event: PointerEvent) {
  if (!resizeDrag) return;
  const bounds = root.value?.parentElement?.getBoundingClientRect();
  const width = bounds?.width ?? window.innerWidth;
  const height = bounds?.height ?? window.innerHeight;
  const maxHeight = height - 54;
  const { direction, originX, originY, originW, originH, startX, startY } = resizeDrag;
  const dx = event.clientX - startX;
  const dy = event.clientY - startY;
  let x = originX;
  let y = originY;
  let w = originW;
  let h = originH;

  if (direction.includes('e')) w = clamp(originW + dx, MIN_W, Math.max(MIN_W, width - originX));
  if (direction.includes('s')) h = clamp(originH + dy, MIN_H, Math.max(MIN_H, maxHeight - originY));
  if (direction.includes('w')) {
    const right = originX + originW;
    x = clamp(originX + dx, 0, Math.max(0, right - MIN_W));
    w = right - x;
  }
  if (direction.includes('n')) {
    const bottom = originY + originH;
    y = clamp(originY + dy, 0, Math.max(0, bottom - MIN_H));
    h = bottom - y;
  }

  emit('resize', Math.round(x), Math.round(y), Math.round(w), Math.round(h));
}

function stopResize() {
  resizeDrag = null;
  window.removeEventListener('pointermove', onResizeMove);
  window.removeEventListener('pointerup', stopResize);
  window.removeEventListener('pointercancel', stopResize);
}

function startResize(event: PointerEvent, direction: string) {
  if (event.button !== 0 || props.win.maximized) return;
  emit('focus');
  resizeDrag = {
    pointerId: event.pointerId,
    startX: event.clientX,
    startY: event.clientY,
    originX: props.win.x,
    originY: props.win.y,
    originW: props.win.w,
    originH: props.win.h,
    direction,
  };
  window.addEventListener('pointermove', onResizeMove);
  window.addEventListener('pointerup', stopResize);
  window.addEventListener('pointercancel', stopResize);
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

onBeforeUnmount(() => {
  stopDrag();
  stopResize();
});
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
    <template v-if="!win.maximized">
      <span class="lgdesk-win__resize is-n" @pointerdown.stop.prevent="startResize($event, 'n')" />
      <span class="lgdesk-win__resize is-s" @pointerdown.stop.prevent="startResize($event, 's')" />
      <span class="lgdesk-win__resize is-e" @pointerdown.stop.prevent="startResize($event, 'e')" />
      <span class="lgdesk-win__resize is-w" @pointerdown.stop.prevent="startResize($event, 'w')" />
      <span class="lgdesk-win__resize is-ne" @pointerdown.stop.prevent="startResize($event, 'ne')" />
      <span class="lgdesk-win__resize is-nw" @pointerdown.stop.prevent="startResize($event, 'nw')" />
      <span class="lgdesk-win__resize is-se" @pointerdown.stop.prevent="startResize($event, 'se')" />
      <span class="lgdesk-win__resize is-sw" @pointerdown.stop.prevent="startResize($event, 'sw')" />
      <span class="lgdesk-win__grip" aria-hidden="true" />
    </template>

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

    <div class="lgdesk-win__body" :class="{ 'is-terminal': win.app === 'terminal' }">
      <DesktopAppHost
        :app="win.app"
        :server="server"
        :session="session"
        :started-at="startedAt"
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

.lgdesk-win__grip {
  position: absolute;
  right: 4px;
  bottom: 4px;
  z-index: 21;
  width: 11px;
  height: 11px;
  background-image: repeating-linear-gradient(
    135deg,
    transparent 0 3px,
    var(--lg-border-strong) 3px 4px
  );
  opacity: 0.55;
  pointer-events: none;
}

.lgdesk-win__resize {
  position: absolute;
  z-index: 20;
}

.lgdesk-win__resize.is-n,
.lgdesk-win__resize.is-s {
  right: 8px;
  left: 8px;
  height: 5px;
}

.lgdesk-win__resize.is-n {
  top: 0;
  cursor: n-resize;
}

.lgdesk-win__resize.is-s {
  bottom: 0;
  cursor: s-resize;
}

.lgdesk-win__resize.is-e,
.lgdesk-win__resize.is-w {
  top: 8px;
  bottom: 8px;
  width: 5px;
}

.lgdesk-win__resize.is-e {
  right: 0;
  cursor: e-resize;
}

.lgdesk-win__resize.is-w {
  left: 0;
  cursor: w-resize;
}

.lgdesk-win__resize.is-ne,
.lgdesk-win__resize.is-nw,
.lgdesk-win__resize.is-se,
.lgdesk-win__resize.is-sw {
  width: 10px;
  height: 10px;
}

.lgdesk-win__resize.is-ne {
  top: 0;
  right: 0;
  cursor: ne-resize;
}

.lgdesk-win__resize.is-nw {
  top: 0;
  left: 0;
  cursor: nw-resize;
}

.lgdesk-win__resize.is-se {
  right: 0;
  bottom: 0;
  cursor: se-resize;
}

.lgdesk-win__resize.is-sw {
  bottom: 0;
  left: 0;
  cursor: sw-resize;
}

.lgdesk-win__controls {
  position: relative;
  z-index: 30;
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
  container-type: inline-size;
}

.lgdesk-win__body.is-terminal {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.lgdesk-win__body.is-terminal :deep(.lgapp-terminal) {
  flex: 1 1 auto;
  height: auto;
}

@media (max-width: 719px) {
  .lgdesk-win,
  .lgdesk-win.is-maximized {
    inset: 6px 6px 62px 6px;
    width: auto !important;
    height: auto !important;
    border-radius: var(--lg-radius-sm);
  }

  .lgdesk-win__resize,
  .lgdesk-win__grip {
    display: none;
  }
}
</style>
