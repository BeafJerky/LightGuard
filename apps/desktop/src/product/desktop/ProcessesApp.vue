<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgButton, LgIcon, LgInput, LgVirtualTable } from '@lightguard/lightui';
import { refreshServer } from '../store';
import { toast } from '../ui';
import { statusLabel } from './apps';
import type { Server } from '../types';

const props = defineProps<{ server: Server }>();

type SortKey = 'cpu' | 'mem' | 'pid';

const query = ref('');
const sort = ref<SortKey>('cpu');
const logQuery = ref('');
const refreshing = ref(false);

const sortOptions: Array<{ key: SortKey; label: string }> = [
  { key: 'cpu', label: 'CPU' },
  { key: 'mem', label: '内存' },
  { key: 'pid', label: 'PID' },
];

const processes = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  return props.server.processes
    .filter((proc) => {
      if (!keyword) return true;
      return `${proc.pid} ${proc.name} ${proc.user}`.toLowerCase().includes(keyword);
    })
    .slice()
    .sort((a, b) => {
      if (sort.value === 'pid') return a.pid - b.pid;
      if (sort.value === 'mem') return b.mem - a.mem;
      return b.cpu - a.cpu;
    });
});

const logColumns = [
  { key: 'time', title: '时间', width: 0.9 },
  { key: 'cpu', title: 'CPU', width: 0.7, align: 'right' as const },
  { key: 'mem', title: '内存', width: 0.7, align: 'right' as const },
  { key: 'down', title: '下行 MB/s', width: 0.9, align: 'right' as const },
  { key: 'up', title: '上行 MB/s', width: 0.9, align: 'right' as const },
  { key: 'processes', title: '进程数', width: 0.7, align: 'right' as const },
];

function clock(ts: number) {
  const date = new Date(ts);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

const logRows = computed<Array<Record<string, string | number>>>(() =>
  props.server.samples
    .slice()
    .reverse()
    .map((sample) => ({
      time: clock(sample.ts),
      cpu: `${sample.cpu.toFixed(1)}%`,
      mem: `${sample.mem.toFixed(1)}%`,
      down: sample.down.toFixed(2),
      up: sample.up.toFixed(2),
      processes: sample.processes,
    })),
);

const visibleLogs = computed(() => {
  const keyword = logQuery.value.trim().toLowerCase();
  if (!keyword) return logRows.value;
  return logRows.value.filter((row) =>
    Object.values(row).join(' ').toLowerCase().includes(keyword),
  );
});

async function refresh() {
  if (refreshing.value) return;
  refreshing.value = true;
  try {
    const updated = await refreshServer(props.server.id);
    if (updated) {
      toast({
        title: '进程快照已更新',
        message: `已采集 ${updated.processCount} 个进程`,
        tone: 'success',
        duration: 2200,
      });
    }
  } finally {
    refreshing.value = false;
  }
}
</script>

<template>
  <div class="lgapp">
    <div class="lgapp-head">
      <div>
        <h2>进程</h2>
        <p>{{ server.name }} · {{ processes.length }} 个进程 · {{ statusLabel(server.status) }}</p>
      </div>
      <LgButton size="small" :disabled="refreshing" @click="refresh">
        <template #icon><LgIcon name="refresh" :size="14" /></template>
        {{ refreshing ? '采样中…' : '重新采样' }}
      </LgButton>
    </div>

    <div class="lgapp-toolbar">
      <div class="lgapp-toolbar__search">
        <LgInput v-model:value="query" placeholder="搜索进程名、用户或 PID" clearable>
          <template #prefix><LgIcon name="search" :size="14" /></template>
        </LgInput>
      </div>
      <div class="lgapp-seg" role="group" aria-label="进程排序">
        <button
          v-for="option in sortOptions"
          :key="option.key"
          type="button"
          :class="{ 'is-active': sort === option.key }"
          :aria-pressed="sort === option.key"
          @click="sort = option.key"
        >
          {{ option.label }}排序
        </button>
      </div>
    </div>

    <div class="lgapp-table">
      <div class="lgapp-table__head">
        <span>PID</span><span>名称</span><span>用户</span><span>CPU</span><span>内存</span><span>状态</span>
      </div>
      <div v-if="processes.length === 0" class="lgapp-table__empty">没有匹配的进程</div>
      <div v-for="proc in processes" :key="proc.pid" class="lgapp-table__row">
        <span class="is-mono">{{ proc.pid }}</span>
        <span>{{ proc.name }}</span>
        <span>{{ proc.user }}</span>
        <span class="is-mono" :class="{ 'is-hot': proc.cpu > 20 }">{{ proc.cpu.toFixed(1) }}%</span>
        <span class="is-mono">{{ proc.mem.toFixed(1) }}%</span>
        <span>{{ proc.state }}</span>
      </div>
    </div>

    <div class="lgapp-section">
      <h3>采样历史 · {{ logRows.length }} 条</h3>
      <div class="lgapp-toolbar">
        <div class="lgapp-toolbar__search">
          <LgInput v-model:value="logQuery" placeholder="过滤采样历史" clearable>
            <template #prefix><LgIcon name="search" :size="14" /></template>
          </LgInput>
        </div>
        <span class="lgapp-count">{{ visibleLogs.length }} 条</span>
      </div>
      <div v-if="logRows.length === 0" class="lgapp-table__empty">
        暂无采样记录，保持连接后会自动累积
      </div>
      <LgVirtualTable
        v-else
        :columns="logColumns"
        :rows="visibleLogs"
        :height="236"
        :row-height="28"
        aria-label="采样历史"
      />
    </div>

    <p class="lgapp-note">
      <LgIcon name="info" :size="13" />
      进程列表与采样历史均来自只读采集脚本，不会结束或挂起任何进程；采样点保留最近 {{ server.samples.length }} 条。
    </p>
  </div>
</template>

<style scoped>
.lgapp-count {
  flex: 0 0 auto;
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
}
</style>
