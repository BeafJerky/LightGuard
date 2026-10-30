<script setup lang="ts">
import { LgIcon } from '@lightguard/lightui';
import type { DockerContainer } from './api';
import { containerActions, type ContainerMenuKind } from './actions';
import { percent, stateLabel, stateTone } from './format';

const props = defineProps<{ row: DockerContainer; active?: boolean }>();
const emit = defineEmits<{
  detail: [row: DockerContainer];
  action: [row: DockerContainer, kind: ContainerMenuKind];
}>();

function run(kind: ContainerMenuKind): void {
  emit('action', props.row, kind);
}

function projectLabel(row: DockerContainer): string {
  if (!row.project) return '';
  return row.service ? `${row.project}/${row.service}` : row.project;
}

function memoryLabel(row: DockerContainer): string {
  return row.memUsage || percent(row.memPercent);
}
</script>

<template>
  <article
    class="lgdocker-row"
    :class="{ 'is-active': active }"
    tabindex="0"
    :aria-label="`${row.name || row.shortId} ${stateLabel(row.state)}`"
    @click="emit('detail', row)"
    @keydown.enter="emit('detail', row)"
  >
    <span class="lgdocker-row__icon" aria-hidden="true">
      <LgIcon name="cartridge" :size="20" />
    </span>

    <div class="lgdocker-row__main">
      <div class="lgdocker-row__title">
        <strong>{{ row.name || row.shortId }}</strong>
        <span class="lgdocker-state" :class="`is-${stateTone(row.state)}`">
          {{ stateLabel(row.state) }}
        </span>
      </div>
      <div class="lgdocker-row__meta">
        <span v-if="row.project" class="lgdocker-chip">
          项目 <b>{{ projectLabel(row) }}</b>
        </span>
        <span>内存 <b class="is-mono">{{ memoryLabel(row) }}</b></span>
        <span>CPU <b class="is-mono">{{ percent(row.cpuPercent) }}</b></span>
        <span :title="`上行 ${row.netTx || '—'}`">
          <LgIcon name="arrow-up" :size="11" />
          <b class="is-mono">{{ row.netTx || '—' }}</b>
        </span>
        <span :title="`下行 ${row.netRx || '—'}`">
          <LgIcon name="arrow-down" :size="11" />
          <b class="is-mono">{{ row.netRx || '—' }}</b>
        </span>
      </div>
    </div>

    <div class="lgdocker-row__actions">
      <button
        v-for="option in containerActions(row.state)"
        :key="option.kind"
        type="button"
        class="lgdocker-iconbtn"
        :class="{ 'is-danger': option.danger }"
        :title="option.label"
        :aria-label="`${option.label} ${row.name || row.shortId}`"
        @click.stop="run(option.kind)"
      >
        <LgIcon :name="option.icon" :size="15" />
      </button>
      <button
        type="button"
        class="lgdocker-iconbtn lgdocker-row__enter"
        :aria-label="`查看 ${row.name || row.shortId} 详情`"
        title="查看详情"
        @click.stop="emit('detail', row)"
      >
        <LgIcon name="chevron-right" :size="16" />
      </button>
    </div>
  </article>
</template>
