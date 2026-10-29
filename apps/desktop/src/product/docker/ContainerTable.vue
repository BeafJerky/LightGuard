<script setup lang="ts">
import type { DockerContainer } from './api';
import { dockerTime, percent, shortText, stateLabel, stateTone } from './format';

defineProps<{ rows: DockerContainer[]; activeId?: string }>();
const emit = defineEmits<{ detail: [row: DockerContainer] }>();

function projectLabel(row: DockerContainer): string {
  if (!row.project) return '—';
  return row.service ? `${row.project}/${row.service}` : row.project;
}
</script>

<template>
  <div class="lgdocker-table" role="table" aria-label="容器列表">
    <div class="lgdocker-table__head" role="row">
      <span>状态</span>
      <span>名称</span>
      <span>镜像</span>
      <span>项目</span>
      <span>CPU</span>
      <span>内存</span>
      <span>网络 I/O</span>
      <span>创建时间</span>
      <span>操作</span>
    </div>
    <div
      v-for="row in rows"
      :key="row.id"
      class="lgdocker-table__row"
      :class="{ 'is-active': row.id === activeId }"
      role="row"
      @click="emit('detail', row)"
    >
      <span>
        <span class="lgdocker-state" :class="`is-${stateTone(row.state)}`">
          {{ stateLabel(row.state) }}
        </span>
      </span>
      <span class="is-strong">{{ row.name || row.shortId }}</span>
      <span :title="row.image">{{ shortText(row.image, 30) }}</span>
      <span :title="projectLabel(row)">{{ projectLabel(row) }}</span>
      <span class="is-num">{{ percent(row.cpuPercent) }}</span>
      <span class="is-num">{{ percent(row.memPercent) }}</span>
      <span class="is-num">{{ row.netRx || '—' }} ↓ / {{ row.netTx || '—' }} ↑</span>
      <span class="is-num">{{ dockerTime(row.createdAt) }}</span>
      <span>
        <button type="button" class="lgdocker-btn is-small" @click.stop="emit('detail', row)">
          详情
        </button>
      </span>
    </div>
  </div>
</template>
