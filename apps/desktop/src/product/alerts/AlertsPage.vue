<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { AlertFilterName, AlertItem } from '../ssh';
import { navigate, navigateTo, openServerDesktop, route } from '../router';
import { toast } from '../ui';
import AlertRow from './AlertRow.vue';
import AuditTable from './AuditTable.vue';
import {
  alertsState,
  ignoreAlert,
  loadAlerts,
  loadMore,
  muteServer,
  subscribeAlerts,
} from './store';

const filters: { id: AlertFilterName; label: string }[] = [
  { id: 'open', label: '未处理' },
  { id: 'handled', label: '已处理' },
  { id: 'all', label: '全部' },
];

// 静音只作用于资源阈值规则；登录、指纹变更与离线告警不走静音，不能给出会静音的承诺。
const MUTABLE_RULES = new Set(['cpu', 'memory', 'disk', 'load']);

const tab = computed<'alerts' | 'audit'>({
  get: () => (route.value.name === 'alerts' && route.value.tab === 'audit' ? 'audit' : 'alerts'),
  set: (value) => navigate({ name: 'alerts', tab: value }),
});

const nowMs = ref(Date.now());
const monitorText = computed(() =>
  alertsState.loginMonitor
    .filter((entry) => !entry.available)
    .map((entry) => `${entry.serverId}：${entry.reason || '未检测到登录日志'}`)
    .join('；'),
);

let stopAlerts: () => void = () => {};
let ticker = 0;

onMounted(() => {
  if (tab.value === 'alerts') void loadAlerts();
  stopAlerts = subscribeAlerts(() => {
    if (route.value.name === 'alerts' && tab.value === 'alerts') void loadAlerts();
  });
  ticker = window.setInterval(() => (nowMs.value = Date.now()), 30_000);
});

onBeforeUnmount(() => {
  stopAlerts();
  window.clearInterval(ticker);
});

function onViewServer(serverId: string) {
  openServerDesktop(serverId);
}

async function onIgnore(alert: AlertItem, payload: { reason: string; untilMs: number }) {
  if (await ignoreAlert(alert.id, payload.reason, payload.untilMs)) {
    toast({
      title: '已忽略该告警',
      message: `${alert.serverName} · ${alert.title}，原因：${payload.reason}`,
      tone: 'info',
    });
  }
}

async function onMute(payload: { serverId: string; ruleId: string | null; untilMs: number }) {
  if (payload.ruleId !== null && !MUTABLE_RULES.has(payload.ruleId)) {
    toast({
      title: '该规则不支持静音',
      message: '静音只作用于 CPU、内存、磁盘与负载告警，登录、指纹变更与离线告警不受影响。',
      tone: 'warning',
    });
    return;
  }
  if (await muteServer(payload.serverId, payload.ruleId, payload.untilMs)) {
    toast({ title: '已静音', message: '在到期前不会为该规则发送提醒', tone: 'info' });
  }
}
</script>

<template>
  <section class="alerts-page">
    <div class="alerts-tabs" role="tablist" aria-label="告警中心">
      <button
        type="button"
        role="tab"
        :aria-selected="tab === 'alerts'"
        :class="{ 'is-active': tab === 'alerts' }"
        @click="tab = 'alerts'"
      >
        告警
      </button>
      <button
        type="button"
        role="tab"
        :aria-selected="tab === 'audit'"
        :class="{ 'is-active': tab === 'audit' }"
        @click="tab = 'audit'"
      >
        操作审计
      </button>
    </div>

    <template v-if="tab === 'alerts'">
      <div class="alerts-filters" role="group" aria-label="告警状态">
        <button
          v-for="filter in filters"
          :key="filter.id"
          type="button"
          :class="{ 'is-active': alertsState.filter === filter.id }"
          :aria-pressed="alertsState.filter === filter.id"
          @click="loadAlerts(filter.id)"
        >
          {{ filter.label }}
        </button>
        <span class="alerts-count">{{ alertsState.activeCount }} 条未处理</span>
      </div>

      <p v-if="alertsState.error" class="alerts-error" role="alert">{{ alertsState.error }}</p>

      <p v-if="monitorText" class="alerts-monitor">
        <LgIcon name="alert" :size="14" />
        <span>登录监控不可用：{{ monitorText }}</span>
      </p>

      <p v-if="alertsState.loading && !alertsState.items.length" class="alerts-loading" role="status">
        正在加载告警…
      </p>

      <div v-else-if="!alertsState.items.length && !alertsState.error" class="alerts-empty">
        <LgIcon name="check" :size="22" />
        <p>{{ alertsState.filter === 'open' ? '没有未处理告警' : '没有符合条件的告警' }}</p>
        <span>已启用规则：CPU / 内存 / 磁盘 / 负载 / 离线 / 指纹变更 / 登录异常</span>
        <button type="button" @click="navigateTo('settings')">去设置阈值</button>
      </div>

      <template v-else>
        <AlertRow
          v-for="alert in alertsState.items"
          :key="alert.id"
          :alert="alert"
          :now-ms="nowMs"
          @view-server="onViewServer"
          @ignore="(payload) => onIgnore(alert, payload)"
          @mute="onMute"
        />
        <button
          v-if="!alertsState.exhausted && alertsState.items.length"
          type="button"
          class="alerts-more"
          :disabled="alertsState.loading"
          @click="loadMore()"
        >
          {{ alertsState.loading ? '加载中…' : '加载更多' }}
        </button>
      </template>
    </template>

    <AuditTable v-else />
  </section>
</template>

<style scoped>
.alerts-page {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.alerts-tabs {
  display: inline-flex;
  gap: 4px;
  align-self: flex-start;
  padding: 3px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface);
}

.alerts-tabs button {
  height: var(--lg-control-h);
  padding: 0 16px;
  border: 0;
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.alerts-tabs button.is-active {
  background: var(--lg-primary);
  color: var(--lg-on-primary);
  font-weight: 600;
}

.alerts-tabs button:focus-visible,
.alerts-filters button:focus-visible,
.alerts-empty button:focus-visible,
.alerts-more:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 1px;
}

.alerts-filters {
  display: flex;
  align-items: center;
  gap: 6px;
}

.alerts-filters button {
  height: var(--lg-control-h-sm);
  padding: 0 12px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.alerts-filters button.is-active {
  border-color: var(--lg-primary);
  color: var(--lg-primary);
}

.alerts-count {
  margin-left: auto;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
}

.alerts-error {
  margin: 0;
  color: var(--lg-danger);
  font-size: var(--lg-fs-sm);
}

.alerts-monitor {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 0;
  padding: 9px 12px;
  border: var(--lg-border-width) solid color-mix(in srgb, var(--lg-warning) 30%, var(--lg-border));
  border-radius: var(--lg-radius-sm);
  background: var(--lg-warning-soft);
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
}

.alerts-loading,
.alerts-empty {
  margin: 24px 0;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  text-align: center;
}

.alerts-empty {
  display: grid;
  justify-items: center;
  gap: 6px;
  padding: 28px 16px;
  border: var(--lg-border-width) dashed var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.alerts-empty p {
  margin: 0;
  color: var(--lg-text);
  font-size: var(--lg-fs-body);
}

.alerts-empty button {
  height: var(--lg-control-h);
  margin-top: 4px;
  padding: 0 12px;
  border: var(--lg-border-width) solid var(--lg-primary);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-primary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.alerts-more {
  align-self: center;
  height: var(--lg-control-h);
  padding: 0 16px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.alerts-more:disabled {
  opacity: 0.6;
  cursor: default;
}
</style>
