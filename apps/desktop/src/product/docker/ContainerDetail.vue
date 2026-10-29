<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type DockerContainerDetail, type DockerStatsSample } from './api';
import { isActiveState } from './actions';
import { dockerTime, percent, portLabel, stateLabel, stateTone } from './format';

const props = defineProps<{ server: Server; containerId: string; containerName: string }>();
const emit = defineEmits<{ close: []; terminal: [detail: DockerContainerDetail] }>();

const detail = ref<DockerContainerDetail | null>(null);
const stats = ref<DockerStatsSample | null>(null);
const readings = ref(0);
const loading = ref(true);
const error = ref('');
const closeRef = ref<HTMLButtonElement | null>(null);
let timer: number | null = null;
let disposed = false;
let sampling = false;

const ports = computed(() => detail.value?.ports ?? []);
const mounts = computed(() => detail.value?.mounts ?? []);
const networks = computed(() => detail.value?.networks ?? []);
const env = computed(() => detail.value?.env ?? []);
// 单次 stats 无法给出速率，连续两次采样后再展示实时读数。
const live = computed(() => readings.value >= 2);

async function load(): Promise<void> {
  loading.value = true;
  error.value = '';
  try {
    detail.value = await dockerApi.containerDetail(props.server.id, props.containerId);
  } catch (caught) {
    error.value = toSshError(caught).message;
  } finally {
    loading.value = false;
  }
}

async function refreshStats(): Promise<void> {
  // 单次采样最长 8s，而轮询间隔只有 2s：上一次未结束时跳过本轮，避免请求堆积。
  if (sampling) return;
  sampling = true;
  try {
    const samples = await dockerApi.stats(props.server.id);
    if (disposed) return;
    stats.value = samples.find((sample) => sample.id === props.containerId) ?? null;
    if (stats.value) readings.value += 1;
  } catch {
    // 采样失败保留上一次读数。
  } finally {
    sampling = false;
  }
}

async function copyId(): Promise<void> {
  const value = detail.value?.id;
  if (!value) return;
  try {
    await navigator.clipboard.writeText(value);
    toast({ title: '已复制容器 ID', message: value, tone: 'success' });
  } catch {
    toast({ title: '复制失败', message: '请手动选择完整 ID 复制。', tone: 'warning' });
  }
}

function openTerminal(): void {
  if (detail.value) emit('terminal', detail.value);
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape') emit('close');
}

onMounted(async () => {
  document.addEventListener('keydown', onKeydown);
  await load();
  // 加载期间抽屉可能已关闭：此时不再注册轮询。
  if (disposed) return;
  await nextTick();
  closeRef.value?.focus();
  void refreshStats();
  timer = window.setInterval(() => {
    void refreshStats();
  }, 2000);
});

onBeforeUnmount(() => {
  disposed = true;
  document.removeEventListener('keydown', onKeydown);
  if (timer !== null) window.clearInterval(timer);
});
</script>

<template>
  <div
    class="lgdocker-drawer"
    role="dialog"
    :aria-label="`容器 ${containerName} 详情`"
    @keydown.esc="emit('close')"
  >
    <div class="lgdocker-drawer__mask" @click="emit('close')" />
    <aside class="lgdocker-drawer__panel">
      <header class="lgdocker-drawer__head">
        <div>
          <strong>{{ containerName }}</strong>
          <span>{{ containerId.slice(0, 12) }}</span>
        </div>
        <button
          v-if="detail && isActiveState(detail.state)"
          type="button"
          class="lgdocker-btn is-small"
          @click="openTerminal"
        >
          <LgIcon name="terminal" :size="12" />
          终端
        </button>
        <button
          ref="closeRef"
          type="button"
          class="lgdocker-btn is-small"
          aria-label="关闭详情"
          @click="emit('close')"
        >
          <LgIcon name="x" :size="13" />
        </button>
      </header>

      <div class="lgdocker-drawer__body">
        <div v-if="loading" class="lgdocker-empty">正在读取容器详情…</div>
        <div v-else-if="error" class="lgdocker-error" role="alert">
          <LgIcon name="alert" :size="15" />
          <div><strong>无法读取详情</strong><span>{{ error }}</span></div>
          <button type="button" class="lgdocker-btn" @click="load()">重试</button>
        </div>
        <template v-else-if="detail">
          <section class="lgdocker-panel">
            <h3>基本信息</h3>
            <dl class="lgdocker-kv">
              <div>
                <dt>全 ID</dt>
                <dd class="is-mono is-row" :title="detail.id">
                  <span class="is-ellipsis">{{ detail.id }}</span>
                  <button
                    type="button"
                    class="lgdocker-iconbtn"
                    title="复制完整 ID"
                    aria-label="复制完整 ID"
                    @click="copyId()"
                  >
                    <LgIcon name="copy" :size="11" />
                  </button>
                </dd>
              </div>
              <div><dt>名称</dt><dd>{{ detail.name || containerName }}</dd></div>
              <div>
                <dt>状态</dt>
                <dd>
                  <span class="lgdocker-state" :class="`is-${stateTone(detail.state)}`">
                    {{ stateLabel(detail.state) }}
                  </span>
                  <span v-if="detail.health" class="lgdocker-muted"> · {{ detail.health }}</span>
                </dd>
              </div>
              <div><dt>镜像</dt><dd :title="detail.image">{{ detail.image }}</dd></div>
              <div><dt>命令</dt><dd class="is-mono" :title="detail.command">{{ detail.command }}</dd></div>
              <div><dt>重启策略</dt><dd>{{ detail.restartPolicy || '—' }}</dd></div>
              <div><dt>创建时间</dt><dd>{{ dockerTime(detail.createdAt) }}</dd></div>
              <div><dt>启动时间</dt><dd>{{ dockerTime(detail.startedAt) }}</dd></div>
            </dl>
          </section>

          <section class="lgdocker-panel">
            <h3>实时资源</h3>
            <dl class="lgdocker-kv">
              <div><dt>CPU</dt><dd>{{ live ? percent(stats?.cpuPercent) : '—' }}</dd></div>
              <div>
                <dt>内存</dt>
                <dd>{{ live && stats ? `${stats.memUsage} / ${stats.memLimit}` : '—' }}</dd>
              </div>
              <div><dt>内存占比</dt><dd>{{ live ? percent(stats?.memPercent) : '—' }}</dd></div>
              <div><dt>网络</dt><dd>{{ live && stats ? `${stats.netRx} ↓ / ${stats.netTx} ↑` : '—' }}</dd></div>
            </dl>
          </section>

          <section class="lgdocker-panel">
            <h3>端口映射</h3>
            <p v-if="ports.length === 0" class="lgdocker-muted">未映射</p>
            <ul v-else class="lgdocker-list">
              <li v-for="port in ports" :key="`${port.hostIp}:${port.hostPort}-${port.containerPort}`">
                {{ portLabel(port) }}
              </li>
            </ul>
          </section>

          <section class="lgdocker-panel">
            <h3>挂载</h3>
            <p v-if="mounts.length === 0" class="lgdocker-muted">无挂载</p>
            <ul v-else class="lgdocker-list is-mono">
              <li v-for="mount in mounts" :key="mount.destination">
                {{ mount.source }} → {{ mount.destination }}
                <span v-if="mount.readOnly" class="lgdocker-muted">（只读）</span>
                <span v-else class="lgdocker-muted">（{{ mount.mode || 'rw' }}）</span>
              </li>
            </ul>
          </section>

          <section class="lgdocker-panel">
            <h3>网络</h3>
            <p v-if="networks.length === 0" class="lgdocker-muted">无网络信息</p>
            <ul v-else class="lgdocker-list">
              <li v-for="network in networks" :key="network.name">
                {{ network.name }}
                <span class="lgdocker-muted">
                  · {{ network.ip || '—' }} · MAC {{ network.mac || '—' }} · 网关
                  {{ network.gateway || '—' }}
                </span>
              </li>
            </ul>
          </section>

          <section class="lgdocker-panel">
            <h3>环境变量</h3>
            <p v-if="env.length === 0" class="lgdocker-muted">无环境变量</p>
            <ul v-else class="lgdocker-list is-mono">
              <li v-for="item in env" :key="item.key">{{ item.key }}={{ item.value }}</li>
            </ul>
            <p v-if="detail.envTruncated" class="lgdocker-muted">环境变量过多，只显示前 200 条。</p>
          </section>
        </template>
      </div>
    </aside>
  </div>
</template>
