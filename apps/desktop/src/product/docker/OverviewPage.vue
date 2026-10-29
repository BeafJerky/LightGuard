<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toast } from '../ui';
import { useDocker } from './useDocker';
import { useDockerSystem } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';
import SystemAuthDialog from './SystemAuthDialog.vue';

const props = defineProps<{ server: Server }>();

const { capabilities, overview, loading, error, installed, running, load } = useDocker(props.server);

const {
  preview: servicePreview,
  busy: serviceBusy,
  progress: serviceProgress,
  error: serviceError,
  done: serviceDone,
  spent: serviceSpent,
  authOpen: authVisible,
  authMessage,
  authError,
  authBusy,
  failures: authFailures,
  ask: askSystem,
  submit: submitPassword,
  confirm: confirmSystem,
  close: closeSystemState,
} = useDockerSystem(props.server);

// 弹窗挂到 Docker 应用根节点：.lgdocker-main 是查询容器，会改写绝对定位的包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const counts = computed(() => overview.value?.counts ?? null);
const info = computed(() => overview.value?.info ?? null);
const df = computed(() => overview.value?.df ?? null);
const mirrors = computed(() => info.value?.mirrors?.join('、') || '未配置');
const autoStart = computed(() => {
  const value = capabilities.value?.autoStart ?? '';
  if (value === 'enabled') return '已启用';
  if (value === 'disabled') return '已禁用';
  return value || '—';
});

async function toggleService(): Promise<void> {
  const ready = await askSystem({
    kind: running.value ? 'service.stop' : 'service.start',
    payload: {},
  });
  if (!ready && !authVisible.value) {
    toast({ title: '无法切换 Docker 服务', message: serviceError.value, tone: 'error' });
  }
}

function closeService(): void {
  const finished = serviceDone.value;
  closeSystemState();
  if (finished?.result === 'success') void load(false);
}

let lightTimer: number | null = null;
let fullTimer: number | null = null;

function stopTimers(): void {
  if (lightTimer !== null) window.clearInterval(lightTimer);
  if (fullTimer !== null) window.clearInterval(fullTimer);
  lightTimer = null;
  fullTimer = null;
}

function startTimers(): void {
  stopTimers();
  lightTimer = window.setInterval(() => void load(false, true), 10_000);
  fullTimer = window.setInterval(() => void load(true, true), 60_000);
}

function onVisibility(): void {
  if (document.visibilityState === 'visible') {
    void load(true, true);
    startTimers();
  } else {
    stopTimers();
  }
}

onMounted(() => {
  void load();
  startTimers();
  document.addEventListener('visibilitychange', onVisibility);
});

onBeforeUnmount(() => {
  stopTimers();
  document.removeEventListener('visibilitychange', onVisibility);
});
</script>

<template>
  <section class="lgdocker-page">
    <header class="lgdocker-pagehead">
      <div>
        <h2>概览</h2>
      </div>
      <div class="lgdocker-pagehead__actions">
        <button
          v-if="installed && !loading"
          type="button"
          class="lgdocker-btn"
          :class="{ 'is-danger': running }"
          :disabled="serviceBusy"
          @click="toggleService()"
        >
          <LgIcon :name="running ? 'power' : 'play'" :size="13" />
          {{ running ? '停止服务' : '启动服务' }}
        </button>
        <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
          <LgIcon name="refresh" :size="13" />
          刷新
        </button>
      </div>
    </header>

    <div v-if="loading" class="lgdocker-empty">正在读取 Docker 状态…</div>

    <div v-else-if="error" class="lgdocker-error" role="alert">
      <LgIcon name="alert" :size="15" />
      <div>
        <strong>无法读取 Docker</strong>
        <span>{{ error }}</span>
      </div>
      <button type="button" class="lgdocker-btn" @click="load()">重试</button>
    </div>

    <div v-else-if="!installed" class="lgdocker-empty">
      <strong>未安装 Docker</strong>
      <span>请在服务器上安装 Docker，或确认当前账号可以访问 docker 命令。</span>
    </div>

    <div v-else-if="!running" class="lgdocker-empty">
      <strong>Docker 服务未运行</strong>
      <span>{{ capabilities?.message || '启动 Docker 服务后可查看容器与镜像。' }}</span>
      <button type="button" class="lgdocker-btn" :disabled="serviceBusy" @click="toggleService()">
        <LgIcon name="play" :size="13" />
        启动 Docker 服务
      </button>
    </div>

    <template v-else>
      <div class="lgdocker-kpis">
        <article class="lgdocker-kpi">
          <span>运行中容器</span>
          <strong>{{ counts?.containersRunning ?? '—' }}</strong>
          <em>共 {{ counts?.containersTotal ?? '—' }} 个</em>
        </article>
        <article class="lgdocker-kpi">
          <span>本地镜像</span>
          <strong>{{ counts?.images ?? '—' }}</strong>
          <em>含未标记镜像</em>
        </article>
        <article class="lgdocker-kpi">
          <span>可回收空间</span>
          <strong>{{ df?.reclaimable ?? '—' }}</strong>
          <em>含悬空镜像与缓存</em>
        </article>
        <article class="lgdocker-kpi">
          <span>Docker 版本</span>
          <strong>{{ info?.version || capabilities?.version || '—' }}</strong>
          <em>API {{ info?.apiVersion || capabilities?.apiVersion || '—' }}</em>
        </article>
      </div>

      <div class="lgdocker-grid">
        <section class="lgdocker-panel">
          <h3>服务信息</h3>
          <dl class="lgdocker-kv">
            <div><dt>存储驱动</dt><dd>{{ info?.storageDriver || '—' }}</dd></div>
            <div><dt>数据目录</dt><dd class="is-mono">{{ info?.dataRoot || '—' }}</dd></div>
            <div><dt>开机自启</dt><dd>{{ autoStart }}</dd></div>
            <div><dt>加速源</dt><dd>{{ mirrors }}</dd></div>
            <div><dt>systemd</dt><dd>{{ capabilities?.systemd ? '可用' : '不可用' }}</dd></div>
          </dl>
        </section>

        <section class="lgdocker-panel">
          <h3>主机资源</h3>
          <dl class="lgdocker-kv">
            <div><dt>CPU</dt><dd>{{ Math.round(server.metrics.cpu) }}%</dd></div>
            <div><dt>内存</dt><dd>{{ Math.round(server.metrics.mem) }}%</dd></div>
            <div><dt>下行</dt><dd>{{ server.metrics.down.toFixed(1) }} MB/s</dd></div>
            <div><dt>上行</dt><dd>{{ server.metrics.up.toFixed(1) }} MB/s</dd></div>
          </dl>
        </section>

        <section class="lgdocker-panel">
          <h3>资源占用</h3>
          <dl class="lgdocker-kv">
            <div><dt>镜像</dt><dd>{{ df?.imagesSize ?? '—' }}</dd></div>
            <div><dt>容器</dt><dd>{{ df?.containersSize ?? '—' }}</dd></div>
            <div><dt>卷</dt><dd>{{ df?.volumesSize ?? '—' }}</dd></div>
            <div><dt>构建缓存</dt><dd>{{ df?.buildCacheSize ?? '—' }}</dd></div>
          </dl>
          <p v-if="!df" class="lgdocker-muted">磁盘占用暂不可用</p>
        </section>
      </div>
    </template>

    <Teleport v-if="dockerRoot" :to="dockerRoot">
      <DockerActionDialog
        v-if="servicePreview"
        :preview="servicePreview"
        :busy="serviceBusy"
        :progress="serviceProgress"
        :error="serviceError"
        :done="serviceDone"
        :spent="serviceSpent"
        @confirm="confirmSystem()"
        @close="closeService"
      />

      <SystemAuthDialog
        :open="authVisible"
        :message="authMessage"
        :error="authError"
        :busy="authBusy"
        :failures="authFailures"
        @submit="submitPassword"
        @cancel="closeService"
      />
    </Teleport>
  </section>
</template>
