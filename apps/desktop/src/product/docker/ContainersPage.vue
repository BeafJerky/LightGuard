<script setup lang="ts">
import { computed, inject, onMounted, ref, watch, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { dockerApi, type DockerContainer } from './api';
import {
  filterContainers,
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
const errorCode = ref('');
const keyword = ref('');
const layout = ref<ContainerLayout>(readLayout());
const selected = ref<DockerContainer | null>(null);
// 抽屉挂到 Docker 应用根节点，避免随 .lgdocker-main 滚动或被其布局包含改写包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const visible = computed(() => sortContainers(filterContainers(rows.value, keyword.value)));
const running = computed(() => rows.value.filter((row) => row.state === 'running').length);
const summary = computed(
  () => `共 ${rows.value.length} 个容器 · ${running.value} 个运行中`,
);
// Docker 本身不可用或当前用户无权限时给独立的空态，其余错误保留错误横幅。
const unavailable = computed(
  () => errorCode.value === 'DOCKER_UNAVAILABLE' || errorCode.value === 'DOCKER_PERMISSION',
);

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  errorCode.value = '';
  try {
    rows.value = await dockerApi.containers(props.server.id);
    // 刷新后按 id 重新绑定选中项：容器已消失则关闭抽屉，否则换成新行以保留最新名称。
    if (selected.value) {
      selected.value = rows.value.find((row) => row.id === selected.value?.id) ?? null;
    }
  } catch (caught) {
    const sshError = toSshError(caught);
    error.value = sshError.message;
    errorCode.value = sshError.code;
    rows.value = [];
  } finally {
    loading.value = false;
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

onMounted(load);
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
        <input v-model="keyword" type="search" placeholder="搜索名称、镜像或项目" aria-label="搜索容器" />
      </label>
      <div class="lgdocker-segment" role="group" aria-label="容器布局">
        <button
          type="button"
          :class="{ 'is-active': layout === 'card' }"
          :aria-pressed="layout === 'card'"
          aria-label="卡片视图"
          @click="setLayout('card')"
        >
          <LgIcon name="grid" :size="13" />
        </button>
        <button
          type="button"
          :class="{ 'is-active': layout === 'table' }"
          :aria-pressed="layout === 'table'"
          aria-label="表格视图"
          @click="setLayout('table')"
        >
          <LgIcon name="menu" :size="13" />
        </button>
      </div>
      <span class="lgdocker-count">{{ summary }}</span>
    </div>

    <div v-if="loading" class="lgdocker-empty">正在读取容器列表…</div>

    <div v-else-if="error && unavailable" class="lgdocker-empty">
      <strong>{{ errorCode === 'DOCKER_UNAVAILABLE' ? 'Docker 不可用' : '没有 Docker 权限' }}</strong>
      <span>{{ error }}</span>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>

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

    <Teleport v-if="selected && dockerRoot" :to="dockerRoot">
      <ContainerDetail
        :key="selected.id"
        :server="server"
        :container-id="selected.id"
        :container-name="selected.name || selected.shortId"
        @close="selected = null"
      />
    </Teleport>
  </section>
</template>
