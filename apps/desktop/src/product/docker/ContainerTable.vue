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
    <div role="rowgroup">
      <div class="lgdocker-table__head" role="row">
        <span role="columnheader">状态</span>
        <span role="columnheader">名称</span>
        <span role="columnheader">镜像</span>
        <span role="columnheader">项目</span>
        <span role="columnheader">CPU</span>
        <span role="columnheader">内存</span>
        <span role="columnheader">网络 I/O</span>
        <span role="columnheader">创建时间</span>
        <span role="columnheader">操作</span>
      </div>
    </div>
    <div role="rowgroup">
      <div
        v-for="row in rows"
        :key="row.id"
        class="lgdocker-table__row"
        :class="{ 'is-active': row.id === activeId }"
        role="row"
        tabindex="0"
        @click="emit('detail', row)"
        @keydown.enter="emit('detail', row)"
      >
        <span role="cell">
          <span class="lgdocker-state" :class="`is-${stateTone(row.state)}`">
            {{ stateLabel(row.state) }}
          </span>
        </span>
        <span role="cell" class="is-strong" :title="row.name || row.shortId">{{ row.name || row.shortId }}</span>
        <span role="cell" :title="row.image">{{ shortText(row.image, 30) }}</span>
        <span role="cell" :title="projectLabel(row)">{{ projectLabel(row) }}</span>
        <span role="cell" class="is-num">{{ percent(row.cpuPercent) }}</span>
        <span role="cell" class="is-num">{{ percent(row.memPercent) }}</span>
        <span role="cell" class="is-num" :title="`${row.netRx || '—'} ↓ / ${row.netTx || '—'} ↑`">
          {{ row.netRx || '—' }} ↓ / {{ row.netTx || '—' }} ↑
        </span>
        <span role="cell" class="is-num">{{ dockerTime(row.createdAt) }}</span>
        <span role="cell">
          <button type="button" class="lgdocker-btn is-small" @click.stop="emit('detail', row)">
            详情
          </button>
        </span>
      </div>
    </div>
  </div>
</template>
