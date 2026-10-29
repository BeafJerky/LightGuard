<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type DockerContainerDetail, type DockerStatsSample } from './api';
import { dockerTime, percent, portLabel, stateLabel, stateTone } from './format';

const props = defineProps<{ server: Server; containerId: string; containerName: string }>();
const emit = defineEmits<{ close: [] }>();

const detail = ref<DockerContainerDetail | null>(null);
const stats = ref<DockerStatsSample | null>(null);
const readings = ref(0);
const loading = ref(true);
const error = ref('');
let timer: number | null = null;

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
  try {
    const samples = await dockerApi.stats(props.server.id);
    stats.value = samples.find((sample) => sample.id === props.containerId) ?? null;
    if (stats.value) readings.value += 1;
  } catch {
    // 采样失败保留上一次读数。
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

onMounted(async () => {
  await load();
  void refreshStats();
  timer = window.setInterval(() => {
    void refreshStats();
  }, 2000);
});

onBeforeUnmount(() => {
  if (timer !== null) window.clearInterval(timer);
});
</script>

<template>
  <div class="lgdocker-drawer" role="dialog" aria-modal="true" :aria-label="`容器 ${containerName} 详情`">
    <div class="lgdocker-drawer__mask" @click="emit('close')" />
    <aside class="lgdocker-drawer__panel">
      <header class="lgdocker-drawer__head">
        <div>
          <strong>{{ containerName }}</strong>
          <span>{{ containerId.slice(0, 12) }}</span>
        </div>
        <button type="button" class="lgdocker-btn is-small" aria-label="关闭详情" @click="emit('close')">
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
                <dd class="is-mono" :title="detail.id">
                  {{ detail.id }}
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
                  · {{ network.ip || '—' }} · 网关 {{ network.gateway || '—' }}
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
