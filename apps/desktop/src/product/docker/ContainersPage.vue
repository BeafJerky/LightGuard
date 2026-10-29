<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { dockerApi, type DockerContainer } from './api';
import {
  filterContainers,
  mergeStats,
  readLayout,
  sortContainers,
  writeLayout,
  type ContainerLayout,
} from './containers';
import { shortText } from './format';
import ContainerCard from './ContainerCard.vue';
import ContainerTable from './ContainerTable.vue';
import ContainerDetail from './ContainerDetail.vue';

const props = defineProps<{ server: Server }>();

const rows = ref<DockerContainer[]>([]);
const loading = ref(true);
const error = ref('');
const keyword = ref('');
const layout = ref<ContainerLayout>(readLayout());
const selected = ref<DockerContainer | null>(null);

const visible = computed(() => sortContainers(filterContainers(rows.value, keyword.value)));
const running = computed(
  () => rows.value.filter((row) => ['running', 'restarting', 'paused'].includes(row.state)).length,
);
const summary = computed(
  () => `共 ${rows.value.length} 个容器 · ${running.value} 个运行中`,
);

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    rows.value = await dockerApi.containers(props.server.id);
  } catch (caught) {
    error.value = toSshError(caught).message;
    rows.value = [];
  } finally {
    loading.value = false;
  }
}

async function refreshStats(): Promise<void> {
  if (rows.value.length === 0) return;
  try {
    const samples = await dockerApi.stats(props.server.id);
    rows.value = mergeStats(rows.value, samples);
  } catch {
    // 采样失败不覆盖列表本身：手动刷新或下一次操作后重试。
  }
}

function setLayout(next: ContainerLayout): void {
  layout.value = next;
  writeLayout(next);
}

watch(
  () => props.server.id,
  () => {
    selected.value = null;
    void load();
  },
);

onMounted(async () => {
  await load();
  void refreshStats();
});
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>容器</h2>
      </div>
      <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
        <LgIcon name="refresh" :size="13" />
        刷新
      </button>
    </header>

    <div class="lgdocker-toolbar">
      <label class="lgdocker-search">
        <LgIcon name="search" :size="13" />
        <input v-model="keyword" type="search" placeholder="搜索名称、镜像或端口" aria-label="搜索容器" />
      </label>
      <div class="lgdocker-segment" role="group" aria-label="容器布局">
        <button
          type="button"
          :class="{ 'is-active': layout === 'card' }"
          aria-label="卡片视图"
          @click="setLayout('card')"
        >
          <LgIcon name="grid" :size="13" />
        </button>
        <button
          type="button"
          :class="{ 'is-active': layout === 'table' }"
          aria-label="表格视图"
          @click="setLayout('table')"
        >
          <LgIcon name="menu" :size="13" />
        </button>
      </div>
      <span class="lgdocker-count">{{ summary }}</span>
    </div>

    <div v-if="loading" class="lgdocker-empty">正在读取容器列表…</div>

    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="15" />
      <div><strong>无法读取容器</strong><span>{{ error }}</span></div>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>

    <div v-else-if="rows.length === 0" class="lgdocker-empty">
      <strong>还没有容器</strong>
      <span>在服务器上运行 docker run 后刷新。</span>
    </div>

    <div v-else-if="visible.length === 0" class="lgdocker-empty">
      <strong>没有匹配的容器</strong>
      <span>试试 {{ shortText(keyword, 24) }} 以外的关键词。</span>
    </div>

    <div v-else-if="layout === 'card'" class="lgdocker-cards">
      <ContainerCard
        v-for="row in visible"
        :key="row.id"
        :row="row"
        :active="selected?.id === row.id"
        @detail="selected = $event"
      />
    </div>

    <ContainerTable
      v-else
      :rows="visible"
      :active-id="selected?.id"
      @detail="selected = $event"
    />

    <ContainerDetail
      v-if="selected"
      :server="server"
      :container-id="selected.id"
      :container-name="selected.name || selected.shortId"
      @close="selected = null"
    />
  </section>
</template>
