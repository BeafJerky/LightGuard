<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgButton, LgEmpty, LgIcon, LgInput } from '@lightguard/lightui';
import { clearLogs, state } from './store';
import { confirm, toast } from './ui';
import type { LogEntry, LogStatus } from './types';

type FilterStatus = 'all' | LogStatus;

const query = ref('');
const filter = ref<FilterStatus>('all');
const expanded = ref<string[]>([]);

const filters: Array<{ key: FilterStatus; label: string }> = [
  { key: 'all', label: '全部' },
  { key: 'success', label: '成功' },
  { key: 'warning', label: '注意' },
  { key: 'failed', label: '失败' },
];

const counts = computed(() => {
  const base: Record<FilterStatus, number> = { all: state.logs.length, success: 0, warning: 0, failed: 0 };
  for (const log of state.logs) base[log.status] += 1;
  return base;
});

const visible = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  return state.logs.filter((log) => {
    if (filter.value !== 'all' && log.status !== filter.value) return false;
    if (!keyword) return true;
    return [log.serverName, log.ip, log.action, log.code, log.source, log.session, log.hint]
      .join(' ')
      .toLowerCase()
      .includes(keyword);
  });
});

const statusLabel: Record<LogStatus, string> = {
  success: '成功',
  warning: '注意',
  failed: '失败',
};

function formatTime(ts: number) {
  const date = new Date(ts);
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, '0');
  if (date.toDateString() === now.toDateString()) {
    return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
  }
  return `${date.getMonth() + 1}/${date.getDate()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function toggle(id: string) {
  expanded.value = expanded.value.includes(id)
    ? expanded.value.filter((item) => item !== id)
    : [...expanded.value, id];
}

function resetFilters() {
  query.value = '';
  filter.value = 'all';
}

function exportLogs() {
  const rows = visible.value.map((log: LogEntry) => ({
    id: log.id,
    time: new Date(log.ts).toISOString(),
    server: log.serverName,
    host: log.ip,
    port: log.port,
    action: log.action,
    status: log.status,
    duration: log.duration,
    source: log.source,
    resultCode: log.code,
    authMethod: log.auth,
    client: log.client,
    session: log.session,
    hostFingerprint: log.fingerprint,
    hint: log.hint,
  }));
  const payload = {
    format: 'lightguard.connection-log',
    version: 1,
    exportedAt: new Date().toISOString(),
    count: rows.length,
    note: '仅连接元数据，不包含口令、私钥或终端内容。',
    records: rows,
  };
  try {
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, '-');
    const link = document.createElement('a');
    link.href = url;
    link.download = `lightguard-connections-${stamp}.json`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    toast({
      title: '已导出连接记录',
      message: `${rows.length} 条元数据 · 不含凭据`,
      tone: 'success',
    });
  } catch {
    toast({ title: '导出失败', message: '当前浏览器不允许本地下载。', tone: 'error' });
  }
}

async function onClear() {
  if (!state.logs.length) {
    toast({ title: '暂无连接记录', tone: 'info' });
    return;
  }
  const ok = await confirm({
    title: '清空连接记录',
    message: `将删除本地保存的 ${state.logs.length} 条连接元数据，此操作不可撤销。配置、服务器与凭据不受影响。`,
    confirmLabel: '清空',
    tone: 'danger',
  });
  if (!ok) return;
  clearLogs();
  expanded.value = [];
  toast({ title: '连接记录已清空', tone: 'success' });
}
</script>

<template>
  <div class="connections">
    <section class="connections-intro">
      <div>
        <h2>审计元数据</h2>
        <p>记录连接、断开与错误事件。<strong>不保存密码，也不记录终端内容。</strong></p>
      </div>
      <div class="connections-intro__actions">
        <LgButton size="small" @click="exportLogs">
          <template #icon><LgIcon name="arrow-down" :size="14" /></template>
          导出 JSON
        </LgButton>
        <LgButton size="small" :disabled="state.logs.length === 0" @click="onClear">
          <template #icon><LgIcon name="trash" :size="14" /></template>
          清空记录
        </LgButton>
      </div>
    </section>

    <section class="connections-toolbar" aria-label="连接记录筛选">
      <div class="connections-search">
        <LgInput v-model:value="query" placeholder="搜索服务器、动作、错误码或会话 ID" clearable>
          <template #prefix><LgIcon name="search" :size="14" /></template>
        </LgInput>
      </div>
      <div class="connections-chips" role="group" aria-label="按结果筛选">
        <button
          v-for="item in filters"
          :key="item.key"
          type="button"
          class="chip"
          :class="{ 'is-active': filter === item.key }"
          :aria-pressed="filter === item.key"
          @click="filter = item.key"
        >
          {{ item.label }}<span>{{ counts[item.key] }}</span>
        </button>
      </div>
    </section>

    <div v-if="visible.length === 0" class="connections-empty">
      <LgEmpty description="没有匹配的连接记录">
        <LgButton v-if="query || filter !== 'all'" @click="resetFilters">清除筛选</LgButton>
      </LgEmpty>
    </div>

    <section v-else class="log-list" aria-label="连接记录列表">
      <div v-for="log in visible" :key="log.id" class="log-row" :class="{ 'is-expanded': expanded.includes(log.id) }">
        <button
          type="button"
          class="log-main"
          :aria-expanded="expanded.includes(log.id)"
          @click="toggle(log.id)"
        >
          <span class="log-time">{{ formatTime(log.ts) }}</span>
          <span class="log-server">
            <strong>{{ log.serverName }}</strong>
            <small>{{ log.ip }}:{{ log.port }}</small>
          </span>
          <span class="log-action">{{ log.action }}</span>
          <span class="log-duration">{{ log.duration }}</span>
          <span class="log-source">{{ log.source }}</span>
          <span class="log-badge" :data-tone="log.status">{{ statusLabel[log.status] }}</span>
          <LgIcon class="log-chevron" name="chevron-right" :size="13" />
        </button>

        <div v-if="expanded.includes(log.id)" class="log-detail">
          <div class="log-detail__grid">
            <div><span>会话 ID</span><strong class="mono">{{ log.session }}</strong></div>
            <div><span>目标地址</span><strong class="mono">{{ log.ip }}:{{ log.port }}</strong></div>
            <div><span>认证方式</span><strong>{{ log.auth }}</strong></div>
            <div><span>客户端</span><strong>{{ log.client }}</strong></div>
            <div><span>结果代码</span><strong class="mono">{{ log.code }}</strong></div>
            <div><span>主机指纹</span><strong class="mono">{{ log.fingerprint }}</strong></div>
          </div>
          <div v-if="log.hint" class="log-hint">
            <LgIcon name="alert" :size="14" />
            <span>{{ log.hint }}</span>
          </div>
        </div>
      </div>
    </section>

    <footer class="connections-foot">
      <LgIcon name="shield" :size="13" />
      会话 ID 与主机指纹可用于安全审计。日志默认保留在本地，最多 80 条；导出文件不含凭据。
    </footer>
  </div>
</template>

<style scoped>
.connections {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.connections-intro {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 12px 14px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.connections-intro h2 {
  margin: 0;
  font-size: var(--lg-fs-body);
  font-weight: 650;
}

.connections-intro p {
  margin: 2px 0 0;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
}

.connections-intro p strong {
  color: var(--lg-text-secondary);
  font-weight: 600;
}

.connections-intro__actions {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
}

.connections-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.connections-search {
  flex: 1 1 auto;
  max-width: 420px;
}

.connections-chips {
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  margin-left: auto;
}

.chip {
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

.chip span {
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}

.chip:hover {
  color: var(--lg-text);
}

.chip.is-active {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-text);
  font-weight: 600;
  box-shadow: var(--lg-shadow-sm);
}

.log-list {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
}

.log-row + .log-row {
  border-top: 1px solid var(--lg-border);
}

.log-main {
  display: grid;
  grid-template-columns: 78px minmax(140px, 1.3fr) minmax(120px, 1fr) 60px 88px 54px 16px;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 10px 12px;
  border: 0;
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  text-align: left;
  cursor: pointer;
}

.log-main:hover {
  background: var(--lg-hover);
}

.log-row.is-expanded .log-main {
  background: var(--lg-bg-soft);
}

.log-time {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}

.log-server {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.log-server strong {
  overflow: hidden;
  color: var(--lg-text);
  font-size: var(--lg-fs-sm);
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-server small {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}

.log-action {
  overflow: hidden;
  color: var(--lg-text-secondary);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-duration {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}

.log-source {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
}

.log-badge {
  justify-self: start;
  padding: 2px 7px;
  border-radius: var(--lg-radius-full);
  font-size: var(--lg-fs-label);
  font-weight: 600;
}

.log-badge[data-tone='success'] {
  background: var(--lg-success-soft);
  color: var(--lg-success);
}

.log-badge[data-tone='warning'] {
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.log-badge[data-tone='failed'] {
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.log-chevron {
  color: var(--lg-text-muted);
  transition: transform var(--lg-dur-fast) var(--lg-ease);
}

.log-row.is-expanded .log-chevron {
  transform: rotate(90deg);
}

.log-detail {
  padding: 0 12px 12px 12px;
  background: var(--lg-bg-soft);
}

.log-detail__grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  padding-top: 4px;
}

.log-detail__grid > div {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 9px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  min-width: 0;
}

.log-detail__grid span {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
}

.log-detail__grid strong {
  overflow-wrap: anywhere;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  font-weight: 500;
}

.mono {
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}

.log-hint {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-top: 8px;
  padding: 8px 10px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-warning-soft);
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  line-height: 1.55;
}

.log-hint svg {
  color: var(--lg-warning);
}

.connections-empty {
  padding: 46px 16px;
  border: 1px dashed var(--lg-border-strong);
  border-radius: var(--lg-radius-md);
}

.connections-foot {
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 2px;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
}

@media (max-width: 1080px) {
  .log-main {
    grid-template-columns: 72px minmax(130px, 1.2fr) minmax(110px, 1fr) 58px 54px 16px;
  }

  .log-source {
    display: none;
  }
}

@media (max-width: 860px) {
  .connections-intro {
    align-items: flex-start;
    flex-direction: column;
  }

  .connections-toolbar {
    align-items: stretch;
    flex-wrap: wrap;
  }

  .connections-search {
    flex-basis: 100%;
    max-width: none;
  }

  .connections-chips {
    margin-left: 0;
    overflow-x: auto;
  }

  .log-main {
    grid-template-columns: 62px minmax(0, 1fr) auto 16px;
    row-gap: 3px;
  }

  .log-time {
    grid-area: 1 / 1;
  }

  .log-server {
    grid-area: 1 / 2;
  }

  .log-badge {
    grid-area: 1 / 3;
  }

  .log-chevron {
    grid-area: 1 / 4;
  }

  .log-action {
    grid-area: 2 / 2 / 2 / 5;
    color: var(--lg-text-muted);
  }

  .log-duration {
    display: none;
  }

  .log-detail__grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

@media (max-width: 540px) {
  .log-detail__grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .connections-intro__actions {
    width: 100%;
  }

  .connections-intro__actions > * {
    flex: 1;
  }
}
</style>
