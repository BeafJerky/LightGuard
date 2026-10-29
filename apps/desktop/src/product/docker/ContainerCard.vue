<script setup lang="ts">
import { LgIcon } from '@lightguard/lightui';
import type { DockerContainer } from './api';
import { containerActions, type ContainerActionKind } from './actions';
import { percent, shortText, stateLabel, stateTone } from './format';

const props = defineProps<{ row: DockerContainer; active?: boolean }>();
const emit = defineEmits<{
  detail: [row: DockerContainer];
  action: [row: DockerContainer, kind: ContainerActionKind];
}>();

function run(kind: ContainerActionKind): void {
  emit('action', props.row, kind);
}
</script>

<template>
  <article
    class="lgdocker-card"
    :class="{ 'is-active': active }"
    tabindex="0"
    @click="emit('detail', row)"
    @keydown.enter="emit('detail', row)"
  >
    <header class="lgdocker-card__head">
      <div>
        <strong>{{ row.name || row.shortId }}</strong>
        <span>{{ shortText(row.image, 36) }}</span>
        <em v-if="row.project">{{ row.project }}<template v-if="row.service">/{{ row.service }}</template></em>
      </div>
      <span class="lgdocker-state" :class="`is-${stateTone(row.state)}`">
        {{ stateLabel(row.state) }}
      </span>
    </header>
    <div class="lgdocker-card__metrics">
      <div><span>CPU</span><strong>{{ percent(row.cpuPercent) }}</strong></div>
      <div><span>内存</span><strong>{{ percent(row.memPercent) }}</strong></div>
      <div class="is-wide">
        <span>网络</span>
        <strong>{{ row.netRx || '—' }} ↓ / {{ row.netTx || '—' }} ↑</strong>
      </div>
    </div>
    <footer class="lgdocker-card__foot">
      <span>{{ row.ports || '无端口映射' }}</span>
      <div class="lgdocker-card__actions">
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
          <LgIcon :name="option.icon" :size="12" />
        </button>
        <button type="button" class="lgdocker-btn is-small" @click.stop="emit('detail', row)">
          详情
        </button>
      </div>
    </footer>
  </article>
</template>
