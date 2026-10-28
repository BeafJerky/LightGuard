<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { PermitMinutes } from '../types';
import type { PermitStatus } from '../ssh';
import { closePermitPanel, guardState, openPermitPanel } from './store';
import { permitAria, permitVisual } from './view';

const props = defineProps<{
  serverId: string;
  status: PermitStatus;
  defaultMinutes: PermitMinutes;
  writeActive: boolean;
  nowMs: number;
  autoLocked: boolean;
}>();

const emit = defineEmits<{
  unlock: [minutes: number];
  lock: [];
}>();

const trigger = ref<HTMLButtonElement | null>(null);
const panel = ref<HTMLElement | null>(null);
const minutes = ref<PermitMinutes>(props.defaultMinutes);

const visual = computed(() => permitVisual(props.status, props.nowMs));
const open = computed(() => guardState.permitPanelOpen);
const options: PermitMinutes[] = [5, 15, 30];

watch(
  () => props.defaultMinutes,
  (value) => {
    minutes.value = value;
  },
);

watch(open, async (value) => {
  if (!value) return;
  minutes.value = props.defaultMinutes;
  await nextTick();
  panel.value?.querySelector<HTMLButtonElement>('[aria-pressed="true"]')?.focus();
});

function toggle() {
  if (open.value) closePermitPanel();
  else openPermitPanel();
}

function close() {
  closePermitPanel();
  trigger.value?.focus();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !open.value) return;
  event.stopPropagation();
  close();
}

document.addEventListener('keydown', onKeydown, true);

onBeforeUnmount(() => {
  document.removeEventListener('keydown', onKeydown, true);
  if (guardState.permitPanelOpen) closePermitPanel();
});
</script>

<template>
  <div class="permit-control" :class="`is-${visual.state}`">
    <button
      ref="trigger"
      type="button"
      class="permit-control__trigger"
      :aria-label="permitAria(status, nowMs)"
      :aria-pressed="!status.locked"
      :aria-expanded="open"
      aria-haspopup="dialog"
      @click="toggle"
    >
      <LgIcon :name="visual.icon" :size="13" />
      <span class="permit-control__label">{{ visual.label }}</span>
      <span v-if="visual.state === 'unlocked'" class="permit-control__pulse" aria-hidden="true" />
    </button>

    <section v-if="open" ref="panel" class="permit-control__panel" role="dialog" aria-label="允许写操作">
      <header class="permit-control__head">
        <strong>允许写操作</strong>
        <span>{{ status.locked ? '当前已锁定' : `剩余 ${visual.remaining}` }}</span>
      </header>
      <p class="permit-control__note">
        仅对本服务器生效，到期自动上锁；终端命令不受此开关约束。
      </p>
      <div class="permit-control__segments" role="group" aria-label="解锁时长">
        <button
          v-for="value in options"
          :key="value"
          type="button"
          :data-minutes="value"
          :class="{ 'is-active': minutes === value }"
          :aria-pressed="minutes === value"
          @click="minutes = value"
        >
          {{ value }} 分钟
        </button>
      </div>
      <div class="permit-control__actions">
        <button
          type="button"
          class="permit-control__primary"
          :disabled="writeActive || guardState.busy"
          @click="emit('unlock', minutes)"
        >
          {{ status.locked ? `解锁 ${minutes} 分钟` : `延长至 ${minutes} 分钟` }}
        </button>
        <button
          v-if="!status.locked"
          type="button"
          class="permit-control__secondary"
          :disabled="writeActive || guardState.busy"
          @click="emit('lock')"
        >
          立即上锁
        </button>
      </div>
      <p v-if="writeActive" class="permit-control__hint">写操作进行中，完成后可调整许可。</p>
      <p v-if="guardState.error" class="permit-control__error">{{ guardState.error }}</p>
    </section>

    <span v-if="autoLocked" class="permit-control__sr" role="status">已自动上锁</span>
  </div>
</template>

<style scoped>
.permit-control {
  position: relative;
  display: flex;
  align-items: center;
}

.permit-control__trigger {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 28px;
  padding: 0 7px;
  border: var(--lg-border-width) solid transparent;
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-muted);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  cursor: pointer;
}

.permit-control__trigger:hover {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-text);
}

.permit-control__trigger:focus-visible {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-text);
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.permit-control.is-unlocked .permit-control__trigger {
  color: var(--lg-warning);
}

.permit-control.is-expiring .permit-control__trigger {
  color: var(--lg-danger);
}

.permit-control__pulse {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: currentColor;
  animation: permit-breath 1.8s ease-in-out infinite;
}

.permit-control__panel {
  position: absolute;
  top: calc(100% + 8px);
  right: 0;
  z-index: 240;
  width: 320px;
  max-width: calc(100vw - 24px);
  padding: 14px;
  border: var(--lg-border-width) solid var(--lg-border-strong);
  border-radius: var(--lg-radius-lg);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-lg);
  color: var(--lg-text);
  font-size: 12px;
  text-align: left;
}

.permit-control__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}

.permit-control__head strong {
  font-size: 13px;
}

.permit-control__head span {
  color: var(--lg-text-muted);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}

.permit-control__note {
  margin: 8px 0 12px;
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  line-height: 1.6;
}

.permit-control__segments {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
}

.permit-control__segments button {
  height: 30px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface-2);
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  cursor: pointer;
}

.permit-control__segments button.is-active {
  border-color: var(--lg-warning);
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.permit-control__actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.permit-control__primary,
.permit-control__secondary {
  flex: 1;
  height: 32px;
  border-radius: var(--lg-radius-sm);
  font-size: 12px;
  cursor: pointer;
}

.permit-control__primary {
  border: var(--lg-border-width) solid var(--lg-warning);
  background: var(--lg-warning);
  color: var(--lg-bg);
  font-weight: 600;
}

.permit-control__secondary {
  border: var(--lg-border-width) solid var(--lg-border);
  background: transparent;
  color: var(--lg-text-secondary);
}

.permit-control__primary:disabled,
.permit-control__secondary:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.permit-control__hint,
.permit-control__error {
  margin: 10px 0 0;
  font-size: 11px;
  line-height: 1.5;
}

.permit-control__hint {
  color: var(--lg-text-muted);
}

.permit-control__error {
  color: var(--lg-danger);
}

.permit-control__sr {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}

@keyframes permit-breath {
  0%,
  100% {
    opacity: 0.35;
  }
  50% {
    opacity: 1;
  }
}

@media (prefers-reduced-motion: reduce) {
  .permit-control__pulse {
    animation: none;
    opacity: 1;
  }
}
</style>
