<script setup lang="ts">
import { computed, inject, onMounted, ref, watch, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type DockerContainer, type DockerContainerDetail } from './api';
import type { ContainerMenuKind } from './actions';
import type { DesktopAppKey } from '../desktop/apps';
import { requestContainerTerminal } from '../terminal/bus';
import {
  filterContainers,
  mergeStats,
  readLayout,
  sortContainers,
  writeLayout,
  type ContainerLayout,
} from './containers';
import { shortText } from './format';
import { useDockerWrite } from './useDockerAction';
import ContainerCard from './ContainerCard.vue';
import ContainerTable from './ContainerTable.vue';
import ContainerDetail from './ContainerDetail.vue';
import DockerActionDialog from './DockerActionDialog.vue';

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

const action = useDockerWrite(props.server);
const {
  preview: actionPreview,
  busy: actionBusy,
  progress: actionProgress,
  error: actionError,
  done: actionDone,
  spent: actionSpent,
  ask: askAction,
  confirm: confirmAction,
  close: closeActionState,
} = action;
// 操作成功后重挂详情抽屉，强制读取最新状态。
const detailKey = ref(0);
// 容器终端标签属于桌面「终端」应用：用注入把「打开应用」交给外壳。
const openApp = inject<(key: DesktopAppKey) => void>('lg:open-app', () => undefined);

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
    void refreshStats();
  } catch (caught) {
    const sshError = toSshError(caught);
    error.value = sshError.message;
    errorCode.value = sshError.code;
    rows.value = [];
  } finally {
    loading.value = false;
  }
}

/** 列表先渲染，再合并一次 stats 采样，资源列随后补齐。 */
async function refreshStats(): Promise<void> {
  if (rows.value.length === 0) return;
  try {
    rows.value = mergeStats(rows.value, await dockerApi.stats(props.server.id));
  } catch {
    // stats 失败只影响资源列，保留已渲染的列表。
  }
}

function setLayout(next: ContainerLayout): void {
  layout.value = next;
  writeLayout(next);
}

async function runAction(row: DockerContainer, kind: ContainerMenuKind): Promise<void> {
  if (kind === 'container.terminal') {
    openContainerTerminal(row);
    return;
  }
  const ready = await askAction({ kind, target: row.id });
  if (!ready) {
    toast({ title: '无法执行', message: actionError.value, tone: 'error' });
  }
}

function openContainerTerminal(target: {
  id: string;
  name: string;
  shortId: string;
  state: string;
}): void {
  if (target.state !== 'running') {
    const paused = target.state === 'paused';
    toast({
      title: paused ? '容器已暂停' : '容器未运行',
      message: paused ? '恢复运行后才能打开终端。' : '启动容器后才能打开终端。',
      tone: 'warning',
    });
    return;
  }
  const name = target.name || target.shortId;
  requestContainerTerminal({
    serverId: props.server.id,
    containerId: target.id,
    name,
    shortId: target.shortId || target.id.slice(0, 12),
    title: props.server.name ? `${name} · ${props.server.name}` : name,
  });
  openApp('terminal');
}

function openDetailTerminal(detail: DockerContainerDetail): void {
  openContainerTerminal({
    id: detail.id,
    name: detail.name,
    shortId: detail.id.slice(0, 12),
    state: detail.state,
  });
}

function closeAction(): void {
  const finished = actionDone.value;
  closeActionState();
  if (finished?.result === 'success') {
    detailKey.value += 1;
    void load();
  }
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
        @action="runAction"
      />
    </div>

    <ContainerTable
      v-else
      :rows="visible"
      :active-id="selected?.id"
      @detail="selected = $event"
      @action="runAction"
    />

    <Teleport v-if="selected && dockerRoot" :to="dockerRoot">
      <ContainerDetail
        :key="`${selected.id}-${detailKey}`"
        :server="server"
        :container-id="selected.id"
        :container-name="selected.name || selected.shortId"
        @close="selected = null"
        @terminal="openDetailTerminal"
      />
    </Teleport>

    <Teleport v-if="actionPreview && dockerRoot" :to="dockerRoot">
      <DockerActionDialog
        :preview="actionPreview"
        :busy="actionBusy"
        :progress="actionProgress"
        :error="actionError"
        :done="actionDone"
        :spent="actionSpent"
        @confirm="confirmAction()"
        @close="closeAction"
      />
    </Teleport>
  </section>
</template>
