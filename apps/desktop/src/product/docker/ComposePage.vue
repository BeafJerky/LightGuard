<script setup lang="ts">
import { computed, inject, onMounted, ref, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { openPermitPanel, permitOf } from '../guard/store';
import { dockerApi, type ComposeProject } from './api';
import { useDockerWrite } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';

const props = defineProps<{ server: Server }>();
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const projects = ref<ComposeProject[]>([]);
const loading = ref(true);
const error = ref('');
const logsOpen = ref(false);
const logsTitle = ref('');
const logsText = ref('');
const logsLoading = ref(false);

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
const summary = computed(() => `共 ${projects.value.length} 个 Compose 项目`);

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    projects.value = await dockerApi.composeProjects(props.server.id);
  } catch (caught) {
    error.value = toSshError(caught).message;
    projects.value = [];
  } finally {
    loading.value = false;
  }
}

async function change(project: ComposeProject, actionKind: 'start' | 'stop'): Promise<void> {
  if (locked.value) {
    openPermitPanel();
    return;
  }
  await askAction({ kind: `compose.${actionKind}`, target: project.name });
}

async function confirm(): Promise<void> {
  const result = await confirmAction();
  if (result) {
    toast({ title: 'Compose 操作完成', message: result.message, tone: 'success' });
  }
}

async function close(): Promise<void> {
  const hadDone = Boolean(actionDone.value);
  closeActionState();
  if (hadDone) await load();
}

async function showLogs(project: ComposeProject): Promise<void> {
  logsOpen.value = true;
  logsTitle.value = project.name;
  logsText.value = '';
  logsLoading.value = true;
  try {
    logsText.value = await dockerApi.composeLogs(props.server.id, project.name, 200);
  } catch (caught) {
    logsText.value = toSshError(caught).message;
  } finally {
    logsLoading.value = false;
  }
}

onMounted(load);
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>Compose 项目</h2>
        <p>{{ summary }}</p>
      </div>
      <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
        <LgIcon name="refresh" :size="13" />
        刷新
      </button>
    </header>

    <div v-if="loading" class="lgdocker-empty">正在读取 Compose 项目…</div>
    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="14" />
      <span>{{ error }}</span>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>
    <div v-else-if="!projects.length" class="lgdocker-empty">
      当前服务器没有可用的 Compose 项目；需要 docker compose 插件与项目配置。
    </div>
    <div v-else class="lgdocker-reslist">
      <article v-for="project in projects" :key="project.name" class="lgdocker-resrow">
        <div class="lgdocker-resrow__main">
          <strong>{{ project.name }}</strong>
          <span>{{ project.status || '状态未知' }}</span>
          <span v-if="project.configFiles.length" class="lgdocker-resrow__muted">
            {{ project.configFiles.join(' · ') }}
          </span>
        </div>
        <div class="lgdocker-resrow__actions">
          <button type="button" class="lgdocker-btn" @click="showLogs(project)">日志</button>
          <button type="button" class="lgdocker-btn" @click="change(project, 'start')">启动</button>
          <button type="button" class="lgdocker-btn" @click="change(project, 'stop')">停止</button>
        </div>
      </article>
    </div>

    <div v-if="logsOpen" class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="Compose 日志">
      <div class="lgdocker-modal__mask" @click="logsOpen = false" />
      <section class="lgdocker-modal__panel lgdocker-logs">
        <header class="lgdocker-modal__head">
          <div>
            <strong>{{ logsTitle }}</strong>
            <span>最后 200 行</span>
          </div>
          <button type="button" class="lgdocker-btn" @click="logsOpen = false">关闭</button>
        </header>
        <pre class="lgdocker-logs__body">{{ logsLoading ? '正在读取日志…' : logsText || '没有日志输出。' }}</pre>
      </section>
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