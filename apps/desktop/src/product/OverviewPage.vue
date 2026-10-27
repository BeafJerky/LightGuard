<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  LgButton,
  LgEmpty,
  LgIcon,
  LgInput,
  LgKpi,
  LgSelect,
  LgSparkline,
  LgSurface,
} from '@lightguard/lightui';
import CredentialsDialog from './CredentialsDialog.vue';
import ServerCard from './ServerCard.vue';
import { openServerDesktop } from './router';
import { fleet, removeServer, retryServer, state, tickAll, updateCredentials } from './store';
import { confirm, openWizard, toast } from './ui';
import type { AuthKind, Server, ServerStatus } from './types';

type FilterStatus = 'all' | 'online' | 'attention' | 'offline';
type SortKey = 'status' | 'name' | 'cpu' | 'latency';

const query = ref('');
const filter = ref<FilterStatus>('all');
const sort = ref<SortKey>('status');
const credentialsFor = ref<Server | null>(null);
const retrying = ref<string[]>([]);

const STATUS_PRIORITY: Record<ServerStatus, number> = {
  error: 0,
  warning: 1,
  offline: 2,
  connecting: 3,
  online: 4,
};

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

const hasFilters = computed(() => Boolean(query.value.trim()) || filter.value !== 'all');

function resetFilters() {
  query.value = '';
  filter.value = 'all';
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

function saveCredentials(auth: AuthKind) {
  const server = credentialsFor.value;
  if (!server) return;
  updateCredentials(server.id, auth);
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
  removeServer(server.id);
  if (credentialsFor.value?.id === server.id) credentialsFor.value = null;
  toast({ title: '已移除服务器', message: server.name, tone: 'success' });
}

function refreshAll() {
  tickAll();
  toast({
    title: '实时数据已刷新',
    message: `${fleet.value.sampled} 台在线主机已更新`,
    tone: 'success',
    duration: 2200,
  });
}

function applyAttention() {
  filter.value = 'attention';
}
</script>

<template>
  <div class="overview">
    <section class="overview-stats" aria-label="运行概览">
      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <LgKpi
          label="主机总数"
          :value="fleet.total"
          unit="台"
          :hint="`在线 ${fleet.online} · 需关注 ${fleet.attention}`"
        />
      </LgSurface>

      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <LgKpi
          label="平均 CPU"
          :value="fleet.avgCpu"
          unit="%"
          tone="primary"
          :hint="`${fleet.sampled} 台在线主机采样`"
        />
        <LgSparkline
          :values="fleet.cpuSeries"
          :height="30"
          :max="100"
          color="var(--lg-primary)"
          label="全部主机 CPU 平均趋势"
        />
      </LgSurface>

      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <LgKpi
          label="平均内存"
          :value="fleet.avgMem"
          unit="%"
          :hint="`${fleet.sampled} 台在线主机采样`"
        />
        <LgSparkline
          :values="fleet.memSeries"
          :height="30"
          :max="100"
          color="var(--lg-accent)"
          label="全部主机内存平均趋势"
        />
      </LgSurface>

      <LgSurface tone="solid" padding="sm" class="overview-stat">
        <LgKpi
          label="实时下行流量"
          :value="fleet.down"
          unit="MB/s"
          :tone="fleet.attention > 0 ? 'warning' : 'success'"
          :hint="`上行 ${fleet.up} MB/s · ${fleet.sampled} 台采样`"
        />
      </LgSurface>
    </section>

    <section v-if="fleet.attention > 0" class="overview-alert" role="status">
      <span class="overview-alert__icon" aria-hidden="true">
        <LgIcon name="alert" :size="17" />
      </span>
      <div class="overview-alert__body">
        <strong>{{ fleet.attention }} 台主机需要关注</strong>
        <span>
          {{ fleet.warning ? `${fleet.warning} 台资源偏高` : '' }}
          {{ fleet.warning && fleet.failed ? ' · ' : '' }}
          {{ fleet.failed ? `${fleet.failed} 台连接失败或离线` : '' }}
        </span>
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
        <LgSelect v-model:value="sort" size="small" :options="sortOptions" class="overview-sort" aria-label="排序方式" />
        <button type="button" class="overview-refresh" @click="refreshAll">
          <LgIcon name="refresh" :size="14" />
          刷新
        </button>
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
        @retry="onRetry"
        @credentials="onCredentials"
        @remove="onRemove"
      />
    </section>

    <footer class="overview-foot">
      <span>显示 {{ visible.length }} / {{ state.servers.length }} 台主机</span>
      <span>演示数据 · 所有操作仅在本浏览器生效</span>
    </footer>

    <CredentialsDialog
      :server="credentialsFor"
      @close="credentialsFor = null"
      @save="saveCredentials"
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
  gap: 12px;
}

.overview-stat {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
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
  font-size: 12.5px;
  font-weight: 600;
}

.overview-alert__body span {
  color: var(--lg-text-muted);
  font-size: 11px;
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
  font-size: 11.5px;
  cursor: pointer;
  white-space: nowrap;
}

.overview-segment button span {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 10px;
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
  font-size: 11.5px;
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
  justify-content: space-between;
  gap: 12px;
  color: var(--lg-text-muted);
  font-size: 10.5px;
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
