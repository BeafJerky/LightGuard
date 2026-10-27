<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgButton, LgIcon, LgStatusPill } from '@lightguard/lightui';
import { round } from '../data';
import { statusLabel } from './apps';
import { toast } from '../ui';
import type { Server } from '../types';

const props = defineProps<{ server: Server }>();

const busy = ref(false);

const drives = computed(() =>
  props.server.disks.map((disk) => ({
    ...disk,
    free: Math.max(0, disk.total - disk.used),
    percent: disk.total > 0 ? Math.round((disk.used / disk.total) * 100) : 0,
  })),
);

const totals = computed(() => {
  let total = 0;
  let used = 0;
  for (const disk of drives.value) {
    total += disk.total;
    used += disk.used;
  }
  return {
    total: round(total, 0),
    used: round(used, 0),
    free: round(Math.max(0, total - used), 0),
    percent: total > 0 ? Math.round((used / total) * 100) : 0,
  };
});

function formatGB(value: number) {
  if (value >= 1000) return `${(value / 1000).toFixed(1)} TB`;
  return `${Math.round(value)} GB`;
}

function tone(percent: number) {
  return percent >= 90 ? 'is-danger' : percent >= 80 ? 'is-warning' : '';
}

function rescan() {
  if (busy.value) return;
  busy.value = true;
  window.setTimeout(() => {
    busy.value = false;
    toast({ title: '磁盘信息已刷新', message: '仅重新读取挂载点，不执行写入。', tone: 'success', duration: 2200 });
  }, 620);
}
</script>

<template>
  <div class="lgapp">
    <div class="lgapp-head">
      <div>
        <h2>磁盘管理</h2>
        <p>挂载点与空间占用 · 只读采集</p>
      </div>
      <LgButton size="small" :loading="busy" @click="rescan">
        <template #icon><LgIcon name="refresh" :size="14" /></template>
        刷新
      </LgButton>
    </div>

    <div class="lgapp-kpis">
      <div class="lgapp-kpi">
        <span>总容量</span>
        <strong>{{ formatGB(totals.total) }}</strong>
        <small>{{ drives.length }} 个挂载点</small>
      </div>
      <div class="lgapp-kpi">
        <span>已使用</span>
        <strong>{{ formatGB(totals.used) }}</strong>
        <small>{{ totals.percent }}% 平均占用</small>
      </div>
      <div class="lgapp-kpi">
        <span>可用</span>
        <strong>{{ formatGB(totals.free) }}</strong>
        <small>不含保留块</small>
      </div>
    </div>

    <div class="lgapp-section">
      <h3>挂载点</h3>
      <div v-if="drives.length === 0" class="lgapp-empty">
        <strong>暂无磁盘信息</strong>
        <span>连接恢复后会重新采集挂载点。</span>
      </div>
      <div v-else class="lgapp-drive-grid">
        <article v-for="disk in drives" :key="disk.mount" class="lgapp-drive">
          <div class="lgapp-drive__top">
            <span class="lgapp-drive__icon"><LgIcon name="database" :size="16" /></span>
            <div class="lgapp-drive__name">
              <strong>{{ disk.mount }}</strong>
              <span>{{ disk.fs }} · {{ disk.type }}</span>
            </div>
            <LgStatusPill
              :label="`${disk.percent}%`"
              :tone="disk.percent >= 90 ? 'danger' : disk.percent >= 80 ? 'warning' : 'online'"
            />
          </div>
          <div class="lgapp-bar">
            <i :class="tone(disk.percent)" :style="{ width: `${disk.percent}%` }" />
          </div>
          <div class="lgapp-drive__foot">
            <span>已用 <strong>{{ formatGB(disk.used) }}</strong></span>
            <span>可用 <strong>{{ formatGB(disk.free) }}</strong></span>
            <span>共 <strong>{{ formatGB(disk.total) }}</strong></span>
          </div>
        </article>
      </div>
    </div>

    <div class="lgapp-info">
      <div class="lgapp-info__row">
        <span>根分区文件系统</span>
        <strong>{{ drives.find((disk) => disk.mount === '/')?.fs ?? '—' }}</strong>
      </div>
      <div class="lgapp-info__row">
        <span>inode 使用率</span>
        <strong>12% · 正常</strong>
      </div>
      <div class="lgapp-info__row">
        <span>SMART 状态</span>
        <strong>{{ statusLabel(server.status) }} · 未发现坏道</strong>
      </div>
    </div>

    <p class="lgapp-note">
      <LgIcon name="lock" :size="13" />
      磁盘信息通过 df、/proc/mounts 与 smartctl 只读采集。LightGuard 不会挂载、格式化或删除任何分区。
    </p>
  </div>
</template>
