<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import {
  LgButton,
  LgEmpty,
  LgIcon,
  LgInput,
  LgKpi,
  LgSelect,
  LgSurface,
} from '@lightguard/lightui';
import CredentialsDialog from './CredentialsDialog.vue';
import ServerCard from './ServerCard.vue';
import ServerEditDialog from './ServerEditDialog.vue';
import { openServerDesktop } from './router';
import { backendAvailable, fleet, removeServer, retryServer, state } from './store';
import { confirm, openWizard, toast } from './ui';
import type { Server, ServerStatus } from './types';

type FilterStatus = 'all' | 'online' | 'attention' | 'offline';
type SortKey = 'status' | 'name' | 'cpu' | 'latency';

const query = ref('');
const filter = ref<FilterStatus>('all');
const group = ref('all');
const sort = ref<SortKey>('status');
const credentialsFor = ref<Server | null>(null);
const editing = ref<Server | null>(null);
const retrying = ref<string[]>([]);

const STATUS_PRIORITY: Record<ServerStatus, number> = {
  error: 0,
  warning: 1,
  offline: 2,
  connecting: 3,
  online: 4,
};

const health = computed(() => {
  const total = Math.max(1, fleet.value.total);
  return {
    online: (fleet.value.online / total) * 100,
    attention: (fleet.value.attention / total) * 100,
    offline: (fleet.value.offline / total) * 100,
    connecting: (fleet.value.connecting / total) * 100,
  };
});

const coverage = computed(() => ({
  groups: new Set(state.servers.map((server) => server.group)).size,
  regions: new Set(state.servers.map((server) => server.region)).size,
}));

const groupOf = (server: Server) => server.group || '未分组';

const groupOptions = computed(() => {
  const counts = new Map<string, number>();
  for (const server of state.servers) {
    const name = groupOf(server);
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  const names = [...counts.keys()].sort((a, b) => {
    if (a === '未分组') return 1;
    if (b === '未分组') return -1;
    return a.localeCompare(b, 'zh-Hans-CN');
  });
  return [
    { label: `全部分组 (${state.servers.length})`, value: 'all' },
    ...names.map((name) => ({ label: `${name} (${counts.get(name)})`, value: name })),
  ];
});

watch(
  () => state.servers.map(groupOf).join('\u0000'),
  () => {
    if (group.value !== 'all' && !state.servers.some((server) => groupOf(server) === group.value)) {
      group.value = 'all';
    }
  },
);

const availability = computed(() =>
  fleet.value.total ? Math.round((fleet.value.online / fleet.value.total) * 100) : 0,
);

const onlineHint = computed(() => {
  const base = `${availability.value}% 可用率`;
  return fleet.value.connecting ? `${base} · ${fleet.value.connecting} 台连接中` : base;
});

const attentionSummary = computed(() => {
  const parts: string[] = [];
  if (fleet.value.warning) parts.push(`${fleet.value.warning} 台资源偏高`);
  if (fleet.value.error) parts.push(`${fleet.value.error} 台连接失败`);
  return parts.join(' · ');
});

const offlineHint = computed(() => {
  if (fleet.value.connecting) return `${fleet.value.connecting} 台正在重连`;
  return fleet.value.offline ? '等待重新连接' : '无离线主机';
});

const counts = computed(() => ({
  all: state.servers.length,
  online: state.servers.filter((server) => server.status === 'online').length,
  attention: state.servers.filter(
    (server) => server.status === 'warning' || server.status === 'error',
  ).length,
  offline: state.servers.filter((server) => server.status === 'offline').length,
}));

const filters: Array<{ key: FilterStatus; label: string }> = [
  { key: 'all', label: '全部' },
  { key: 'online', label: '在线' },
  { key: 'attention', label: '需关注' },
  { key: 'offline', label: '离线' },
];

const sortOptions = [
  { label: '状态优先', value: 'status' },
  { label: '名称', value: 'name' },
  { label: 'CPU 降序', value: 'cpu' },
  { label: '延迟升序', value: 'latency' },
];

const visible = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  const list = state.servers.filter((server) => {
    if (group.value !== 'all' && groupOf(server) !== group.value) return false;
    if (filter.value === 'online' && server.status !== 'online') return false;
    if (filter.value === 'attention' && server.status !== 'warning' && server.status !== 'error') {
      return false;
    }
    if (filter.value === 'offline' && server.status !== 'offline') return false;
    if (!keyword) return true;
    return [server.name, server.ip, server.role, server.group, ...server.tags]
      .join(' ')
      .toLowerCase()
      .includes(keyword);
  });
  return list.slice().sort((a, b) => {
    if (sort.value === 'name') return a.name.localeCompare(b.name);
    if (sort.value === 'cpu') return b.metrics.cpu - a.metrics.cpu;
    if (sort.value === 'latency') {
      const left = a.status === 'online' || a.status === 'warning' ? a.latency : Number.MAX_SAFE_INTEGER;
      const right = b.status === 'online' || b.status === 'warning' ? b.latency : Number.MAX_SAFE_INTEGER;
      return left - right;
    }
    const byStatus = STATUS_PRIORITY[a.status] - STATUS_PRIORITY[b.status];
    return byStatus !== 0 ? byStatus : a.name.localeCompare(b.name);
  });
});

const hasFilters = computed(
  () => Boolean(query.value.trim()) || filter.value !== 'all' || group.value !== 'all',
);

function resetFilters() {
  query.value = '';
  filter.value = 'all';
  group.value = 'all';
}

function onOpen(server: Server) {
  if (server.status === 'online' || server.status === 'warning') {
    openServerDesktop(server.id);
    return;
  }
  toast({
    title: '无法进入桌面',
    message: '连接恢复后可打开服务器桌面。',
    tone: 'warning',
  });
}

async function onRetry(server: Server) {
  if (retrying.value.includes(server.id)) return;
  retrying.value.push(server.id);
  const updated = await retryServer(server.id);
  retrying.value = retrying.value.filter((id) => id !== server.id);
  if (!updated) return;
  if (updated.status === 'online' || updated.status === 'warning') {
    toast({ title: '重新连接成功', message: `${updated.name} 已恢复实时数据`, tone: 'success' });
  } else {
    toast({
      title: '连接仍然失败',
      message: updated.error?.message ?? '请检查网络与凭据后重试。',
      tone: 'error',
    });
  }
}

function onCredentials(server: Server) {
  credentialsFor.value = server;
}

function onEdit(server: Server) {
  editing.value = server;
}

function onServerSaved(server: Server) {
  editing.value = null;
  toast({ title: '服务器已更新', message: `${server.name} 的配置已保存`, tone: 'success' });
}

function onEditCredentials(server: Server) {
  editing.value = null;
  credentialsFor.value = server;
}

function saveCredentials() {
  const server = credentialsFor.value;
  if (!server) return;
  credentialsFor.value = null;
  toast({ title: '凭据已更新', message: `${server.name} 已重新连接`, tone: 'success' });
}

async function onRemove(server: Server) {
  const ok = await confirm({
    title: '移除服务器',
    message: `将从工作区移除 ${server.name}（${server.ip}:${server.port}）。历史连接记录会保留。`,
    confirmLabel: '移除',
    tone: 'danger',
  });
  if (!ok) return;
  await removeServer(server.id);
  if (credentialsFor.value?.id === server.id) credentialsFor.value = null;
  toast({ title: '已移除服务器', message: server.name, tone: 'success' });
}

function applyAttention() {
  filter.value = 'attention';
}
</script>

<template>
  <div class="overview">
    <section class="overview-stats" aria-label="运行概览">
      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <span class="overview-stat__icon"><LgIcon name="server" :size="15" /></span>
        <LgKpi
          label="主机总数"
          :value="fleet.total"
          unit="台"
          :hint="
            fleet.total
              ? `${coverage.groups} 个分组 · ${coverage.regions} 个区域`
              : '尚未添加服务器'
          "
        />
        <span v-if="fleet.total > 0" class="overview-stat__meter" aria-hidden="true">
          <i class="is-online" :style="{ width: `${health.online}%` }" />
          <i class="is-warning" :style="{ width: `${health.attention}%` }" />
          <i class="is-offline" :style="{ width: `${health.offline}%` }" />
          <i class="is-connecting" :style="{ width: `${health.connecting}%` }" />
        </span>
      </LgSurface>

      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <span class="overview-stat__icon is-success"><LgIcon name="wifi" :size="15" /></span>
        <LgKpi
          label="当前在线"
          :value="fleet.online"
          :unit="`/ ${fleet.total} 台`"
          :tone="fleet.online > 0 ? 'success' : 'default'"
          :hint="onlineHint"
        />
        <span class="overview-stat__meter" aria-hidden="true">
          <i class="is-online" :style="{ width: `${health.online}%` }" />
        </span>
      </LgSurface>

      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <span class="overview-stat__icon is-warning"><LgIcon name="alert" :size="15" /></span>
        <LgKpi
          label="需要关注"
          :value="fleet.attention"
          unit="台"
          :tone="fleet.attention > 0 ? 'warning' : 'default'"
          :hint="attentionSummary || '暂无异常'"
        />
        <span class="overview-stat__meter" aria-hidden="true">
          <i class="is-warning" :style="{ width: `${health.attention}%` }" />
        </span>
      </LgSurface>

      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <span class="overview-stat__icon"><LgIcon name="power" :size="15" /></span>
        <LgKpi
          label="当前离线"
          :value="fleet.offline"
          unit="台"
          :hint="offlineHint"
        />
        <span class="overview-stat__meter" aria-hidden="true">
          <i class="is-offline" :style="{ width: `${health.offline}%` }" />
        </span>
      </LgSurface>
    </section>

    <section v-if="fleet.attention > 0" class="overview-alert" role="status">
      <span class="overview-alert__icon" aria-hidden="true">
        <LgIcon name="alert" :size="17" />
      </span>
      <div class="overview-alert__body">
        <strong>{{ fleet.attention }} 台主机需要关注</strong>
        <span>{{ attentionSummary }}</span>
      </div>
      <LgButton size="small" @click="applyAttention">只看需关注</LgButton>
    </section>

    <section class="overview-toolbar" aria-label="服务器筛选">
      <div class="overview-search">
        <LgInput v-model:value="query" placeholder="搜索名称、地址、角色或标签" clearable>
          <template #prefix><LgIcon name="search" :size="14" /></template>
        </LgInput>
      </div>

      <div class="overview-segment" role="tablist" aria-label="按状态筛选">
        <button
          v-for="item in filters"
          :key="item.key"
          type="button"
          role="tab"
          :aria-selected="filter === item.key"
          :class="{ 'is-active': filter === item.key }"
          @click="filter = item.key"
        >
          {{ item.label }}
          <span>{{ counts[item.key] }}</span>
        </button>
      </div>

      <div class="overview-toolbar__right">
        <LgSelect
          v-model:value="group"
          size="small"
          :options="groupOptions"
          class="overview-group"
          aria-label="按分组筛选"
        />
        <LgSelect v-model:value="sort" size="small" :options="sortOptions" class="overview-sort" aria-label="排序方式" />
      </div>
    </section>

    <div v-if="state.servers.length === 0" class="overview-empty">
      <LgEmpty description="还没有服务器。添加第一台 SSH 主机开始使用。">
        <LgButton type="primary" @click="openWizard">
          <template #icon><LgIcon name="plus" :size="15" /></template>
          添加服务器
        </LgButton>
      </LgEmpty>
    </div>

    <div v-else-if="visible.length === 0" class="overview-empty">
      <LgEmpty description="没有匹配的服务器">
        <LgButton v-if="hasFilters" @click="resetFilters">清除筛选</LgButton>
        <LgButton v-else type="primary" @click="openWizard">添加服务器</LgButton>
      </LgEmpty>
    </div>

    <section v-else class="overview-grid" aria-label="服务器列表">
      <ServerCard
        v-for="server in visible"
        :key="server.id"
        :server="server"
        @open="onOpen"
        @edit="onEdit"
        @retry="onRetry"
        @credentials="onCredentials"
        @remove="onRemove"
      />
    </section>

    <footer class="overview-foot">
      <span>显示 {{ visible.length }} / {{ state.servers.length }} 台主机</span>
      <span v-if="fleet.sampled" class="overview-foot__net">
        <LgIcon name="network" :size="11" />
        全部已连接主机实时吞吐：下行 {{ fleet.down }} MB/s · 上行 {{ fleet.up }} MB/s
      </span>
      <span>{{ backendAvailable ? '桌面直连 · 只读采集' : '真实 SSH 仅桌面版可用' }}</span>
    </footer>

    <CredentialsDialog
      :server="credentialsFor"
      @close="credentialsFor = null"
      @saved="saveCredentials"
    />

    <ServerEditDialog
      :server="editing"
      @close="editing = null"
      @saved="onServerSaved"
      @credentials="onEditCredentials"
    />
  </div>
</template>

<style scoped>
.overview {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.overview-stats {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 14px;
}

.overview-stat {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  min-height: 132px;
  overflow: hidden;
}

.overview-stat::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(135deg, color-mix(in srgb, var(--lg-text) 3%, transparent), transparent 46%);
  pointer-events: none;
}

.overview-stat__icon {
  position: absolute;
  top: 15px;
  right: 15px;
  z-index: 1;
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface-2);
  color: var(--lg-text-secondary);
}

.overview-stat__icon.is-success {
  background: var(--lg-success-soft);
  color: var(--lg-success);
}

.overview-stat__icon.is-warning {
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.overview-stat__meter {
  position: absolute;
  right: 16px;
  bottom: 16px;
  z-index: 1;
  display: flex;
  gap: 2px;
  width: 96px;
  height: 6px;
  overflow: hidden;
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface-2);
}

.overview-stat__meter i {
  display: block;
  height: 100%;
  border-radius: var(--lg-radius-full);
}

.overview-stat__meter i.is-online {
  background: var(--lg-success);
}

.overview-stat__meter i.is-warning {
  background: var(--lg-warning);
}

.overview-stat__meter i.is-offline {
  background: var(--lg-danger);
}

.overview-stat__meter i.is-connecting {
  background: var(--lg-text-muted);
}

.overview-alert {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, var(--lg-warning) 30%, var(--lg-border));
  border-radius: var(--lg-radius-md);
  background: var(--lg-warning-soft);
}

.overview-alert__icon {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 9px;
  background: color-mix(in srgb, var(--lg-warning) 16%, transparent);
  color: var(--lg-warning);
}

.overview-alert__body {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.overview-alert__body strong {
  color: var(--lg-text);
  font-size: var(--lg-fs-body);
  font-weight: 600;
}

.overview-alert__body span {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
}

.overview-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.overview-search {
  flex: 1 1 240px;
  max-width: 360px;
  min-width: 0;
}

.overview-segment {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
}

.overview-segment button {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 5px 9px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
  white-space: nowrap;
}

.overview-segment button span {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}

.overview-segment button:hover {
  color: var(--lg-text);
}

.overview-segment button.is-active {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-text);
  font-weight: 600;
  box-shadow: var(--lg-shadow-sm);
}

.overview-segment button.is-active span {
  color: var(--lg-primary);
}

.overview-toolbar__right {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}

.overview-group {
  width: 132px;
}

.overview-sort {
  width: 118px;
}

.overview-refresh {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 9px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.overview-refresh:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.overview-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
  gap: 13px;
}

.overview-empty {
  padding: 46px 16px;
  border: 1px dashed var(--lg-border-strong);
  border-radius: var(--lg-radius-lg);
  background: color-mix(in srgb, var(--lg-surface) 62%, transparent);
}

.overview-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
}

.overview-foot__net {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--lg-text-secondary);
  font-variant-numeric: tabular-nums;
}

@media (max-width: 1200px) {
  .overview-stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 820px) {
  .overview-toolbar {
    align-items: stretch;
    flex-wrap: wrap;
  }

  .overview-search {
    flex-basis: 100%;
    max-width: none;
  }

  .overview-segment {
    flex: 1 1 auto;
    overflow-x: auto;
  }

  .overview-toolbar__right {
    flex: 1 1 auto;
    justify-content: flex-end;
    margin-left: 0;
  }
}

@media (max-width: 540px) {
  .overview-stats {
    grid-template-columns: minmax(0, 1fr);
  }

  .overview-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .overview-foot {
    flex-direction: column;
    gap: 2px;
  }
}
</style>
