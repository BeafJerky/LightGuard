<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgButton, LgIcon, LgInput, LgVirtualTable } from '@lightguard/lightui';
import { tickServer } from '../store';
import { toast } from '../ui';
import { statusLabel } from './apps';
import type { Server } from '../types';

const props = defineProps<{ server: Server }>();

type SortKey = 'cpu' | 'mem' | 'pid';

const query = ref('');
const sort = ref<SortKey>('cpu');
const logQuery = ref('');
const sampleSeed = ref(1);

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

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function hash(input: string) {
  let value = 2166136261;
  for (let i = 0; i < input.length; i += 1) {
    value ^= input.charCodeAt(i);
    value = Math.imul(value, 16777619);
  }
  return value >>> 0;
}

const logColumns = [
  { key: 'time', title: '时间', width: 0.9 },
  { key: 'level', title: '级别', width: 0.7 },
  { key: 'proc', title: '进程', width: 1.3 },
  { key: 'pid', title: 'PID', width: 0.6, align: 'right' as const },
  { key: 'message', title: '事件', width: 2.6 },
];

const logRows = computed<Array<Record<string, string | number>>>(() => {
  const random = mulberry32(hash(`${props.server.id}-${sampleSeed.value}`));
  const names = props.server.processes.length
    ? props.server.processes.map((proc) => ({ name: proc.name, pid: proc.pid }))
    : [{ name: 'sshd', pid: 742 }];
  const infoMessages = [
    '采样完成：读取 /proc/stat',
    'SSH 通道心跳正常',
    '只读命令执行完成',
    '系统信息缓存已刷新',
    '主机密钥校验通过',
    '会话仍处于活跃状态',
  ];
  const warnMessages = [
    '采样耗时高于阈值（320ms）',
    '主机密钥缓存即将过期',
    '网络抖动，自动重试采样',
    '内存压力升高，采样降频',
  ];
  const errorMessages = [
    '只读命令超时，已取消',
    'SSH 通道短暂阻塞',
    '磁盘采集返回部分结果',
  ];
  const base = Date.now();
  const rows: Array<Record<string, string | number>> = [];
  for (let index = 0; index < 2000; index += 1) {
    const source = names[Math.floor(random() * names.length)];
    const roll = random();
    const level = roll > 0.96 ? 'ERROR' : roll > 0.86 ? 'WARN' : 'INFO';
    const pool = level === 'ERROR' ? errorMessages : level === 'WARN' ? warnMessages : infoMessages;
    const message = pool[Math.floor(random() * pool.length)];
    const ts = new Date(base - index * 5000 - Math.floor(random() * 4000));
    const pad = (value: number) => String(value).padStart(2, '0');
    rows.push({
      time: `${pad(ts.getHours())}:${pad(ts.getMinutes())}:${pad(ts.getSeconds())}`,
      level,
      proc: source.name,
      pid: source.pid,
      message: `${message} · ${props.server.name}`,
    });
  }
  return rows;
});

const visibleLogs = computed(() => {
  const keyword = logQuery.value.trim().toLowerCase();
  if (!keyword) return logRows.value;
  return logRows.value.filter((row) =>
    `${row.time} ${row.level} ${row.proc} ${row.message}`.toLowerCase().includes(keyword),
  );
});

function refresh() {
  tickServer(props.server);
  sampleSeed.value += 1;
  toast({ title: '进程快照已更新', message: `重新采样 ${logRows.value.length} 行日志`, tone: 'success', duration: 2200 });
}
</script>

<template>
  <div class="lgapp">
    <div class="lgapp-head">
      <div>
        <h2>进程</h2>
        <p>{{ server.name }} · {{ processes.length }} 个进程 · {{ statusLabel(server.status) }}</p>
      </div>
      <LgButton size="small" @click="refresh">
        <template #icon><LgIcon name="refresh" :size="14" /></template>
        重新采样
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
      <h3>采样日志 · 2,000 行</h3>
      <div class="lgapp-toolbar">
        <div class="lgapp-toolbar__search">
          <LgInput v-model:value="logQuery" placeholder="过滤采样日志" clearable>
            <template #prefix><LgIcon name="search" :size="14" /></template>
          </LgInput>
        </div>
        <span class="lgapp-count">{{ visibleLogs.length }} 行</span>
      </div>
      <LgVirtualTable
        :columns="logColumns"
        :rows="visibleLogs"
        :height="236"
        :row-height="28"
        aria-label="进程采样日志"
      />
    </div>

    <p class="lgapp-note">
      <LgIcon name="info" :size="13" />
      采样日志由本地原型生成，用于验证 2,000 行虚拟滚动。进程列表来自只读的 ps 快照，不会结束或挂起任何进程。
    </p>
  </div>
</template>

<style scoped>
.lgapp-count {
  flex: 0 0 auto;
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 10px;
}
</style>
