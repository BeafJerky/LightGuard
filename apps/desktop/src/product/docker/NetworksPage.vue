<script setup lang="ts">
import { computed, inject, onMounted, ref, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { openPermitPanel, permitOf } from '../guard/store';
import { dockerApi, type DockerNetworkSummary } from './api';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';

const props = defineProps<{ server: Server }>();
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const rows = ref<DockerNetworkSummary[]>([]);
const loading = ref(true);
const error = ref('');
const name = ref('');
const driver = ref('bridge');
const drivers = ['bridge', 'host', 'overlay', 'macvlan', 'ipvlan', 'none'];

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

const locked = computed(() => permitOf(props.server.id).locked);
const summary = computed(() => `共 ${rows.value.length} 个网络`);
const builtin = (value: string) => ['bridge', 'host', 'none'].includes(value);

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    rows.value = await dockerApi.networks(props.server.id);
  } catch (caught) {
    error.value = toSshError(caught).message;
    rows.value = [];
  } finally {
    loading.value = false;
  }
}

async function askCreate(): Promise<void> {
  if (locked.value) {
    openPermitPanel();
    return;
  }
  const value = name.value.trim();
  if (!value) {
    error.value = '请输入网络名';
    return;
  }
  await askAction({ kind: 'network.create', target: value, payload: { driver: driver.value } });
}

async function askRemove(row: DockerNetworkSummary): Promise<void> {
  if (locked.value) {
    openPermitPanel();
    return;
  }
  await askAction({ kind: 'network.remove', target: row.name });
}

async function askPrune(): Promise<void> {
  if (locked.value) {
    openPermitPanel();
    return;
  }
  await askAction({ kind: 'network.prune', target: '' });
}

async function confirm(): Promise<void> {
  const result = await confirmAction();
  if (result) {
    toast({ title: '网络操作完成', message: result.message, tone: 'success' });
  }
}

async function close(): Promise<void> {
  const hadDone = Boolean(actionDone.value);
  closeActionState();
  if (hadDone) {
    name.value = '';
    await load();
  }
}

onMounted(load);
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>网络</h2>
        <p>{{ summary }}</p>
      </div>
      <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
        <LgIcon name="refresh" :size="13" />
        刷新
      </button>
    </header>

    <div class="lgdocker-toolbar">
      <label class="lgdocker-search">
        <LgIcon name="network" :size="13" />
        <input v-model="name" type="text" placeholder="新网络名称" aria-label="新网络名称" />
      </label>
      <select v-model="driver" class="lgdocker-select" aria-label="网络驱动">
        <option v-for="item in drivers" :key="item" :value="item">{{ item }}</option>
      </select>
      <button type="button" class="lgdocker-btn" @click="askCreate()">创建网络</button>
      <button type="button" class="lgdocker-btn" @click="askPrune()">清理未使用</button>
    </div>

    <div v-if="loading" class="lgdocker-empty">正在读取网络…</div>
    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="14" />
      <span>{{ error }}</span>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>
    <div v-else-if="!rows.length" class="lgdocker-empty">没有网络。</div>
    <div v-else class="lgdocker-reslist">
      <article v-for="row in rows" :key="row.id" class="lgdocker-resrow">
        <div class="lgdocker-resrow__main">
          <strong>{{ row.name }}</strong>
          <span>{{ row.driver }} · {{ row.scope }}<template v-if="row.internal"> · internal</template></span>
          <span class="lgdocker-resrow__muted">
            {{ row.containers.length ? `连接容器：${row.containers.join('、')}` : '无容器连接' }}
          </span>
        </div>
        <div class="lgdocker-resrow__actions">
          <button
            type="button"
            class="lgdocker-btn"
            :disabled="row.containers.length > 0 || builtin(row.name)"
            @click="askRemove(row)"
          >
            删除
          </button>
        </div>
      </article>
    </div>

    <Teleport v-if="actionPreview && dockerRoot" :to="dockerRoot">
      <DockerActionDialog
        :preview="actionPreview"
        :busy="actionBusy"
        :progress="actionProgress"
        :error="actionError"
        :done="actionDone"
        :spent="actionSpent"
        @confirm="confirm()"
        @close="close()"
      />
    </Teleport>
  </section>
</template>