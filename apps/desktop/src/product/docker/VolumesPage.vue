<script setup lang="ts">
import { computed, inject, onMounted, ref, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { openPermitPanel, permitOf } from '../guard/store';
import { dockerApi, type DockerVolumeSummary } from './api';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';

const props = defineProps<{ server: Server }>();
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const rows = ref<DockerVolumeSummary[]>([]);
const loading = ref(true);
const error = ref('');
const name = ref('');

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
const summary = computed(() => `共 ${rows.value.length} 个数据卷`);

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    rows.value = await dockerApi.volumes(props.server.id);
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
    error.value = '请输入数据卷名';
    return;
  }
  await askAction({ kind: 'volume.create', target: value });
}

async function askRemove(row: DockerVolumeSummary): Promise<void> {
  if (locked.value) {
    openPermitPanel();
    return;
  }
  await askAction({ kind: 'volume.remove', target: row.name });
}

async function askPrune(): Promise<void> {
  if (locked.value) {
    openPermitPanel();
    return;
  }
  await askAction({ kind: 'volume.prune', target: '' });
}

async function confirm(): Promise<void> {
  const result = await confirmAction();
  if (result) {
    toast({ title: '数据卷操作完成', message: result.message, tone: 'success' });
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
        <h2>数据卷</h2>
        <p>{{ summary }}</p>
      </div>
      <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
        <LgIcon name="refresh" :size="13" />
        刷新
      </button>
    </header>

    <div class="lgdocker-toolbar">
      <label class="lgdocker-search">
        <LgIcon name="database" :size="13" />
        <input v-model="name" type="text" placeholder="新数据卷名称" aria-label="新数据卷名称" />
      </label>
      <button type="button" class="lgdocker-btn" @click="askCreate()">创建数据卷</button>
      <button type="button" class="lgdocker-btn" @click="askPrune()">清理未使用</button>
    </div>

    <div v-if="loading" class="lgdocker-empty">正在读取数据卷…</div>
    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="14" />
      <span>{{ error }}</span>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>
    <div v-else-if="!rows.length" class="lgdocker-empty">没有数据卷。</div>
    <div v-else class="lgdocker-reslist">
      <article v-for="row in rows" :key="row.name" class="lgdocker-resrow">
        <div class="lgdocker-resrow__main">
          <strong>{{ row.name }}</strong>
          <span>{{ row.driver }}</span>
          <span class="lgdocker-resrow__muted">
            {{ row.mountpoint }}
          </span>
          <span class="lgdocker-resrow__muted">
            {{
              !row.usageKnown
                ? '使用情况未检查（仅检查前 20 个卷）'
                : row.usedBy.length
                  ? `使用中：${row.usedBy.join('、')}`
                  : '无容器使用'
            }}
          </span>
        </div>
        <div class="lgdocker-resrow__actions">
          <button
            type="button"
            class="lgdocker-btn"
            :disabled="row.usedBy.length > 0"
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