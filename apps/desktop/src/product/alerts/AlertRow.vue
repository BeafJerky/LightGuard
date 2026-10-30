<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { AlertItem, AlertSeverity, AlertStatus } from '../ssh';
import { validateIgnoreReason } from './store';
import {
  DURATION_OPTIONS,
  durationToUntil,
  relativeText,
  type DurationOption,
} from './duration';

const props = defineProps<{ alert: AlertItem; nowMs: number }>();

const emit = defineEmits<{
  viewServer: [serverId: string];
  ignore: [payload: { reason: string; untilMs: number }];
  mute: [payload: { serverId: string; ruleId: string | null; untilMs: number }];
}>();

type PanelMode = 'none' | 'ignore' | 'mute';
type OpenMode = Exclude<PanelMode, 'none'>;

const SEVERITY_META: Record<AlertSeverity, { label: string; icon: string }> = {
  critical: { label: '紧急', icon: 'alert' },
  warning: { label: '警告', icon: 'alert' },
  info: { label: '提示', icon: 'info' },
};

const STATUS_LABEL: Record<AlertStatus, string> = {
  active: '活动中',
  resolved: '已恢复',
  ignored: '已忽略',
};

const mode = ref<PanelMode>('none');
const duration = ref<DurationOption>('30m');
const reason = ref('');
const error = ref('');

const severity = computed(() => SEVERITY_META[props.alert.severity]);
const statusLabel = computed(() => STATUS_LABEL[props.alert.status]);
const since = computed(() => relativeText(props.alert.firstSeenAt, props.nowMs));
const panelId = computed(() => `alert-row-panel-${props.alert.id}`);
const openFor = computed<OpenMode | null>(() => (mode.value === 'none' ? null : mode.value));

/**
 * `nowMs` 只是渲染用的参考时刻，父级可能隔一阵才刷新：
 * 忽略/静音的截止时间必须从真实时钟起算，否则会生成一个已经过去的时长。
 */
function nextUntil(option: DurationOption): number {
  return durationToUntil(option, new Date());
}

function toggle(next: OpenMode) {
  mode.value = mode.value === next ? 'none' : next;
  error.value = '';
}

function closePanel(event?: KeyboardEvent) {
  // 面板没开时把 Escape 让给外层对话框。
  if (mode.value === 'none') return;
  event?.stopPropagation();
  mode.value = 'none';
  error.value = '';
}

function confirmIgnore() {
  const problem = validateIgnoreReason(reason.value);
  if (problem) {
    error.value = problem;
    return;
  }
  emit('ignore', { reason: reason.value.trim(), untilMs: nextUntil(duration.value) });
  mode.value = 'none';
  error.value = '';
  reason.value = '';
}

function confirmMute() {
  emit('mute', {
    serverId: props.alert.serverId,
    ruleId: props.alert.ruleId,
    untilMs: nextUntil(duration.value),
  });
  mode.value = 'none';
  error.value = '';
}
</script>

<template>
  <article
    class="alert-row"
    :class="[`is-${alert.severity}`, `is-${alert.status}`]"
    :data-alert-id="alert.id"
    @keydown.esc="closePanel"
  >
    <span class="alert-row__severity" :aria-label="`严重度：${severity.label}`">
      <LgIcon :name="severity.icon" :size="15" />
      {{ severity.label }}
    </span>

    <div class="alert-row__body">
      <div class="alert-row__head">
        <strong>{{ alert.title }}</strong>
        <span class="alert-row__status">
          <span v-if="alert.status === 'active'" class="alert-row__pulse" aria-hidden="true" />
          {{ statusLabel }}
          <template v-if="alert.status === 'active'"> · {{ since }}</template>
        </span>
      </div>
      <p class="alert-row__detail">
        {{ alert.serverName }} · {{ alert.detail || alert.ruleId }}
        <template v-if="alert.status === 'ignored' && alert.ignoreReason">
          · 忽略原因：{{ alert.ignoreReason }}
        </template>
      </p>

      <div
        v-if="openFor"
        :id="panelId"
        class="alert-row__panel"
        role="group"
        :aria-label="openFor === 'ignore' ? '忽略此告警' : '静音此告警'"
      >
        <div class="alert-row__durations" role="group" aria-label="持续时长">
          <button
            v-for="option in DURATION_OPTIONS"
            :key="option.id"
            type="button"
            :class="{ 'is-active': duration === option.id }"
            :aria-pressed="duration === option.id"
            @click="duration = option.id"
          >
            {{ option.label }}
          </button>
        </div>
        <label v-if="openFor === 'ignore'" class="alert-row__reason">
          <span>忽略原因（必填，最多 200 字）</span>
          <input
            v-model="reason"
            type="text"
            aria-label="忽略原因"
            maxlength="200"
            @input="error = ''"
          />
        </label>
        <p v-if="error" class="alert-row__error" role="alert">{{ error }}</p>
        <div class="alert-row__actions">
          <button type="button" @click="closePanel()">取消</button>
          <button
            v-if="openFor === 'ignore'"
            type="button"
            class="is-primary"
            data-action="confirm-ignore"
            @click="confirmIgnore"
          >
            确认忽略
          </button>
          <button
            v-else
            type="button"
            class="is-primary"
            data-action="confirm-mute"
            @click="confirmMute"
          >
            确认静音
          </button>
        </div>
      </div>
    </div>

    <div class="alert-row__ops">
      <button
        type="button"
        data-action="view-server"
        :aria-label="`查看 ${alert.serverName} 的服务器`"
        @click="emit('viewServer', alert.serverId)"
      >
        <LgIcon name="monitor" :size="12" />
        查看服务器
      </button>
      <template v-if="alert.status === 'active'">
        <button
          type="button"
          :class="{ 'is-active': mode === 'mute' }"
          data-action="mute"
          :aria-label="`静音 ${alert.serverName} 的告警`"
          :aria-expanded="mode === 'mute'"
          :aria-controls="mode === 'mute' ? panelId : undefined"
          @click="toggle('mute')"
        >
          <LgIcon name="bell" :size="12" />
          静音
        </button>
        <button
          type="button"
          :class="{ 'is-active': mode === 'ignore' }"
          data-action="ignore"
          :aria-label="`忽略 ${alert.serverName} 的告警`"
          :aria-expanded="mode === 'ignore'"
          :aria-controls="mode === 'ignore' ? panelId : undefined"
          @click="toggle('ignore')"
        >
          <LgIcon name="eye-off" :size="12" />
          忽略
        </button>
      </template>
    </div>
  </article>
</template>

<style scoped>
.alert-row {
  display: grid;
  grid-template-columns: 64px minmax(0, 1fr) auto;
  gap: 12px;
  padding: 12px 14px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.alert-row + .alert-row {
  margin-top: 8px;
}

.alert-row__severity {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  align-self: start;
  padding-top: 1px;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  white-space: nowrap;
}

.alert-row.is-warning .alert-row__severity {
  color: var(--lg-warning);
}

.alert-row.is-critical .alert-row__severity {
  color: var(--lg-danger);
}

.alert-row__body {
  min-width: 0;
}

.alert-row__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}

.alert-row__head strong {
  font-size: var(--lg-fs-body);
  font-weight: 600;
  overflow-wrap: anywhere;
}

.alert-row__status {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 5px;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  white-space: nowrap;
}

.alert-row.is-active .alert-row__status {
  color: var(--lg-text-secondary);
}

.alert-row__pulse {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--lg-warning);
  animation: alert-breath 1.8s ease-in-out infinite;
}

.alert-row.is-critical .alert-row__pulse {
  background: var(--lg-danger);
}

.alert-row.is-info .alert-row__pulse {
  background: var(--lg-info);
}

.alert-row__detail {
  margin: 4px 0 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  line-height: 1.6;
  overflow-wrap: anywhere;
}

.alert-row__ops {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-start;
  justify-content: flex-end;
  gap: 4px;
}

.alert-row__ops button {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: var(--lg-control-h-sm);
  padding: 0 9px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
  white-space: nowrap;
}

.alert-row__ops button:hover {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.alert-row__ops button.is-active {
  border-color: var(--lg-primary);
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.alert-row__panel {
  margin-top: 10px;
  padding: 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface-2);
}

.alert-row__durations {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.alert-row__durations button {
  height: var(--lg-control-h-sm);
  padding: 0 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.alert-row__durations button.is-active {
  border-color: var(--lg-primary);
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.alert-row__reason {
  display: grid;
  gap: 4px;
  margin-top: 10px;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
}

.alert-row__reason input {
  height: var(--lg-control-h);
  padding: 0 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  color: var(--lg-text);
  font-size: var(--lg-fs-sm);
}

.alert-row__reason input:focus-visible {
  border-color: var(--lg-primary);
  outline: 2px solid var(--lg-focus);
  outline-offset: 1px;
}

.alert-row__error {
  margin: 8px 0 0;
  color: var(--lg-danger);
  font-size: var(--lg-fs-sm);
}

.alert-row__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 10px;
}

.alert-row__actions button {
  height: var(--lg-control-h);
  padding: 0 12px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.alert-row__actions button.is-primary {
  border-color: var(--lg-primary);
  background: var(--lg-primary);
  color: var(--lg-on-primary);
}

.alert-row__ops button:focus-visible,
.alert-row__durations button:focus-visible,
.alert-row__actions button:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 1px;
}

@keyframes alert-breath {
  0%,
  100% {
    opacity: 0.35;
  }
  50% {
    opacity: 1;
  }
}

@media (max-width: 900px) {
  .alert-row {
    grid-template-columns: minmax(0, 1fr);
  }

  .alert-row__ops {
    justify-content: flex-start;
  }
}

@media (prefers-reduced-motion: reduce) {
  .alert-row__pulse {
    animation: none;
    opacity: 1;
  }
}
</style>
