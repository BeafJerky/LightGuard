<script setup lang="ts">
import { computed, inject, nextTick, reactive, ref, watch, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type RegistryConfig } from './api';
import {
  buildMirrorsPayload,
  buildProxyPayload,
  mirrorRowsFromConfig,
  proxyFormFromConfig,
  MAX_MIRRORS,
  type MirrorRow,
  type ProxyForm,
} from './settings';
import { useDockerSystem } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';
import SystemAuthDialog from './SystemAuthDialog.vue';

const props = defineProps<{ server: Server; open: boolean; mode: 'mirrors' | 'proxy' }>();
const emit = defineEmits<{ close: []; applied: [] }>();

// 弹窗挂到 Docker 应用根节点：.lgdocker-main 是查询容器，会改写绝对定位的包含块。
const dockerRoot = inject<Ref<HTMLElement | null>>('lg:docker-root', ref(null));

const config = ref<RegistryConfig | null>(null);
const loading = ref(false);
const loadError = ref('');
const localError = ref('');
// 预览请求在飞：此时保存/关闭都必须锁住，否则会叠加预览或关掉面板后仍弹出预览框。
const asking = ref(false);
// 读取请求序号：慢响应不能覆盖后发起的读取，也不能冲掉用户已经输入的内容。
let seq = 0;

const rows = ref<MirrorRow[]>([]);
const selected = ref(-1);
const dragFrom = ref(-1);
const table = ref<HTMLElement | null>(null);

const proxy = reactive<ProxyForm>({
  enabled: false,
  server: '',
  account: '',
  password: '',
  noProxy: '',
});

const title = computed(() => (props.mode === 'mirrors' ? '加速源设置' : '代理设置'));
const subtitle = computed(() =>
  props.mode === 'mirrors'
    ? '/etc/docker/daemon.json · 需要系统授权'
    : '访问 Docker 仓库时使用代理',
);

// 面板常驻：切换服务器后写操作必须落到当前服务器，因此传入实时读取 id 的视图。
const systemServer = {
  get id(): string {
    return props.server.id;
  },
};

const {
  preview: systemPreview,
  busy: systemBusy,
  progress: systemProgress,
  error: systemError,
  errorCode,
  done: systemDone,
  spent: systemSpent,
  authOpen: authVisible,
  authMessage,
  authError,
  authBusy,
  failures: authFailures,
  ask: askSystem,
  submit: submitPassword,
  confirm: confirmSystem,
  close: closeSystemState,
} = useDockerSystem(systemServer);

const invalid = computed(() => config.value?.invalid ?? false);
const readable = computed(() => config.value?.readable !== false && !invalid.value);
const proxySupported = computed(() => config.value?.proxySupported !== false);
const proxyConfigured = computed(() => config.value?.proxyConfigured ?? false);

const NOTES_KEY = 'lg.docker.mirrorNotes.v1';

function readNotes(serverId: string): Record<string, string> {
  try {
    const raw = window.localStorage.getItem(`${NOTES_KEY}:${serverId}`);
    const value = raw ? JSON.parse(raw) : null;
    if (!value || typeof value !== 'object') return {};
    const notes: Record<string, string> = {};
    for (const [url, note] of Object.entries(value)) {
      if (typeof note === 'string' && note.trim()) notes[url] = note;
    }
    return notes;
  } catch {
    return {};
  }
}

function writeNotes(serverId: string): void {
  try {
    const notes: Record<string, string> = {};
    for (const row of rows.value) {
      const url = row.url.trim();
      const note = row.note.trim();
      if (url && note) notes[url] = note;
    }
    window.localStorage.setItem(`${NOTES_KEY}:${serverId}`, JSON.stringify(notes));
  } catch {
    // 本机备注只是辅助信息，写不进去不影响 daemon.json。
  }
}

function reset(): void {
  config.value = null;
  loading.value = false;
  loadError.value = '';
  localError.value = '';
  rows.value = [];
  selected.value = -1;
  dragFrom.value = -1;
  Object.assign(proxy, { enabled: false, server: '', account: '', password: '', noProxy: '' });
  closeSystemState();
}

async function load(): Promise<void> {
  const request = ++seq;
  loading.value = true;
  loadError.value = '';
  localError.value = '';
  try {
    const value = await dockerApi.registryConfig(props.server.id);
    if (request !== seq) return;
    config.value = value;
    rows.value = mirrorRowsFromConfig(value, readNotes(props.server.id));
    Object.assign(proxy, proxyFormFromConfig(value));
  } catch (caught) {
    if (request !== seq) return;
    loadError.value = toSshError(caught).message;
  } finally {
    if (request === seq) loading.value = false;
  }
}

/** 只刷新配置状态、不改写表单：用户已输入的加速源与代理值要保留下来。 */
async function reloadConfig(): Promise<void> {
  const request = ++seq;
  try {
    const value = await dockerApi.registryConfig(props.server.id);
    if (request !== seq) return;
    config.value = value;
  } catch (caught) {
    if (request !== seq) return;
    loadError.value = toSshError(caught).message;
  }
}

watch(
  () => props.open,
  (open) => {
    reset();
    if (open) void load();
  },
  { immediate: true },
);

// 同一页面上切换服务器时面板不会重新挂载，需要丢弃旧服务器的表单与授权状态。
watch(
  () => props.server.id,
  () => {
    if (!props.open) return;
    reset();
    void load();
  },
);

function requestClose(): void {
  if (systemBusy.value || asking.value) return;
  emit('close');
}

function selectRow(index: number): void {
  selected.value = index;
}

function addRow(): void {
  rows.value.push({ url: '', note: '' });
  selected.value = rows.value.length - 1;
  void nextTick(() => {
    const inputs = table.value?.querySelectorAll<HTMLInputElement>('[data-mirror-url]');
    inputs?.[rows.value.length - 1]?.focus();
  });
}

function removeRow(): void {
  const index = selected.value;
  if (index < 0 || index >= rows.value.length) return;
  rows.value.splice(index, 1);
  selected.value = -1;
  dragFrom.value = -1;
}

function onDragStart(event: DragEvent, index: number): void {
  dragFrom.value = index;
  selected.value = index;
  event.dataTransfer?.setData('text/plain', String(index));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move';
}

function onDrop(index: number): void {
  const from = dragFrom.value;
  dragFrom.value = -1;
  if (from < 0 || from === index) return;
  const list = rows.value;
  const [moved] = list.splice(from, 1);
  if (!moved) return;
  list.splice(index, 0, moved);
  selected.value = index;
}

function draft() {
  if (props.mode === 'mirrors') {
    return buildMirrorsPayload(rows.value, { mirrorsKnown: readable.value });
  }
  return buildProxyPayload(proxy, {
    configured: proxyConfigured.value,
    currentNoProxy: config.value?.noProxy ?? '',
  });
}

async function save(): Promise<void> {
  if (!config.value || loading.value || systemBusy.value || asking.value) return;
  localError.value = '';
  const value = draft();
  if (value.errors.length > 0) {
    localError.value = value.errors[0];
    return;
  }
  let ready = false;
  asking.value = true;
  try {
    ready = await askSystem({ kind: 'daemon.update', payload: { ...value.payload } });
  } finally {
    asking.value = false;
  }
  if (!ready && !authVisible.value) {
    localError.value = systemError.value || '无法生成配置预览';
    // daemon.json 在别处被改坏时只刷新只读状态，保留用户已输入的列表与代理表单。
    if (errorCode.value === 'DAEMON_INVALID') void reloadConfig();
  }
}

function finishSystem(): void {
  const finished = systemDone.value;
  closeSystemState();
  if (finished?.result === 'success') {
    if (props.mode === 'mirrors') writeNotes(props.server.id);
    toast({
      title: '已应用并重启 Docker',
      message:
        props.mode === 'mirrors'
          ? '加速源已写入 daemon.json，可在预览日志中确认结果。'
          : '代理配置已写入 daemon.json，可在预览日志中确认结果。',
      tone: 'success',
    });
    // 父组件把 applied 映射为关闭面板：同一件事只发一次。
    emit('applied');
    return;
  }
  if (finished?.result === 'failed') {
    localError.value = finished.message || '配置应用失败，请检查 daemon.json 与 Docker 服务状态。';
  }
}
</script>

<template>
  <Teleport v-if="dockerRoot" :to="dockerRoot">
    <div v-if="open" class="lgdocker-modal" role="dialog" aria-modal="true" :aria-label="title">
      <div class="lgdocker-modal__mask" @click="requestClose" />
      <section class="lgdocker-modal__panel is-wide">
        <header class="lgdocker-modal__head">
          <div>
            <strong>{{ title }}</strong>
            <span>{{ subtitle }}</span>
          </div>
          <button
            type="button"
            class="lgdocker-btn is-small"
            :aria-label="`关闭${title}`"
            :disabled="systemBusy || asking"
            @click="requestClose"
          >
            <LgIcon name="x" :size="13" />
          </button>
        </header>

        <div class="lgdocker-modal__body">
          <div v-if="loading" class="lgdocker-empty">正在读取 daemon.json…</div>

          <div v-else-if="loadError" class="lgdocker-error" role="alert">
            <LgIcon name="alert" :size="15" />
            <div><strong>无法读取配置</strong><span>{{ loadError }}</span></div>
            <button type="button" class="lgdocker-btn" @click="load()">重试</button>
          </div>

          <template v-else-if="config">
            <div v-if="invalid" class="lgdocker-error" role="alert">
              <LgIcon name="alert" :size="15" />
              <div>
                <strong>daemon.json 无法解析，已禁止编辑</strong>
                <span>{{ config.note }}</span>
                <span>请先在服务器上修正 /etc/docker/daemon.json，再回来修改。</span>
              </div>
              <button type="button" class="lgdocker-btn" :disabled="loading" @click="load()">
                重新读取
              </button>
            </div>

            <template v-else-if="mode === 'mirrors'">
              <div v-if="!readable" class="lgdocker-settings__notice">
                <LgIcon name="key" :size="14" />
                <div>
                  <strong>需要系统授权后查看</strong>
                  <span>{{ config.note || '保存时会用 sudo 重新读取并合并，已有配置不会被覆盖。' }}</span>
                </div>
              </div>

              <div class="lgdocker-mirror__bar">
                <button type="button" class="lgdocker-btn is-small" @click="addRow">
                  <LgIcon name="plus" :size="13" />
                  添加
                </button>
                <button
                  type="button"
                  class="lgdocker-btn is-small"
                  :disabled="selected < 0"
                  @click="removeRow"
                >
                  <LgIcon name="trash" :size="13" />
                  删除
                </button>
              </div>

              <div ref="table" class="lgdocker-mirror">
                <div class="lgdocker-mirror__head" aria-hidden="true">
                  <span />
                  <span>URL</span>
                  <span>备注</span>
                </div>
                <div
                  v-for="(row, index) in rows"
                  :key="index"
                  class="lgdocker-mirror__row"
                  :class="{ 'is-selected': selected === index, 'is-dragging': dragFrom === index }"
                  draggable="true"
                  @click="selectRow(index)"
                  @dragstart="onDragStart($event, index)"
                  @dragover.prevent
                  @drop.prevent="onDrop(index)"
                  @dragend="dragFrom = -1"
                >
                  <span class="lgdocker-mirror__grip" aria-hidden="true">
                    <LgIcon name="menu" :size="13" />
                  </span>
                  <input
                    v-model="row.url"
                    type="text"
                    spellcheck="false"
                    placeholder="https://docker.mirrors.example.com"
                    aria-label="加速源地址"
                    data-mirror-url
                    @focus="selectRow(index)"
                  />
                  <input
                    v-model="row.note"
                    type="text"
                    placeholder="选填"
                    aria-label="加速源备注"
                    @focus="selectRow(index)"
                  />
                </div>
                <p v-if="rows.length === 0" class="lgdocker-mirror__empty">
                  还没有加速源，点击「添加」输入镜像加速地址。
                </p>
              </div>

              <p v-if="localError || systemError" class="lgdocker-field-error" role="alert">
                {{ localError || systemError }}
              </p>
              <p class="lgdocker-muted">
                最多 {{ MAX_MIRRORS }} 个；备注只保存在本机，不会写入 daemon.json。
              </p>
            </template>

            <template v-else>
              <div v-if="!proxySupported" class="lgdocker-settings__notice">
                <LgIcon name="alert" :size="14" />
                <div>
                  <strong>当前 Docker 不支持代理配置</strong>
                  <span>需要 Docker 23 及以上的 daemon.json proxies，升级后可在此启用。</span>
                </div>
              </div>

              <div class="lgdocker-proxy__toggle">
                <span>启动代理</span>
                <label class="lgdocker-radio">
                  <input
                    v-model="proxy.enabled"
                    type="radio"
                    :value="true"
                    aria-label="开启代理"
                    :disabled="!proxySupported"
                  />
                  <span>开启</span>
                </label>
                <label class="lgdocker-radio">
                  <input
                    v-model="proxy.enabled"
                    type="radio"
                    :value="false"
                    aria-label="关闭代理"
                    :disabled="!proxySupported"
                  />
                  <span>关闭</span>
                </label>
              </div>

              <label class="lgdocker-formfield">
                <span>服务器</span>
                <input
                  v-model="proxy.server"
                  type="text"
                  spellcheck="false"
                  placeholder="http://127.0.0.1:7890"
                  aria-label="代理服务器"
                  :disabled="!proxy.enabled || !proxySupported"
                />
              </label>

              <label class="lgdocker-formfield">
                <span>账号</span>
                <input
                  v-model="proxy.account"
                  type="text"
                  spellcheck="false"
                  autocomplete="off"
                  placeholder="选填"
                  aria-label="代理账号"
                  :disabled="!proxy.enabled || !proxySupported"
                />
              </label>

              <label class="lgdocker-formfield">
                <span>密码</span>
                <input
                  v-model="proxy.password"
                  type="password"
                  autocomplete="new-password"
                  placeholder="选填"
                  aria-label="代理密码"
                  :disabled="!proxy.enabled || !proxySupported"
                />
              </label>

              <label class="lgdocker-formfield">
                <span>不代理域名</span>
                <textarea
                  v-model="proxy.noProxy"
                  rows="3"
                  spellcheck="false"
                  placeholder="选填，每行一个"
                  aria-label="不代理域名"
                  :disabled="!proxy.enabled || !proxySupported"
                />
              </label>

              <p v-if="config.note && readable" class="lgdocker-muted">{{ config.note }}</p>
              <p v-if="proxyConfigured && !proxy.server" class="lgdocker-muted">
                服务器留空将保持现有代理地址，只更新不代理域名。
              </p>
              <p v-if="localError || systemError" class="lgdocker-field-error" role="alert">
                {{ localError || systemError }}
              </p>
              <p class="lgdocker-muted">
                账号密码会编码进代理地址；预览与审计只展示脱敏结果。
              </p>
            </template>
          </template>
        </div>

        <footer class="lgdocker-modal__foot">
          <p v-if="mode === 'mirrors' && config && !invalid" class="lgdocker-modal__hint">
            可拖拽调整顺序，提高加速源的优先级
          </p>
          <button
            type="button"
            class="lgdocker-btn"
            :disabled="systemBusy || asking"
            @click="requestClose"
          >
            取消
          </button>
          <button
            type="button"
            class="lgdocker-btn is-primary"
            :disabled="!config || invalid || loading || systemBusy || asking"
            @click="save()"
          >
            {{ asking ? '正在生成预览…' : systemBusy ? '处理中…' : '保存并重启 Docker' }}
          </button>
        </footer>
      </section>
    </div>

    <DockerActionDialog
      v-if="systemPreview"
      :preview="systemPreview"
      :busy="systemBusy"
      :progress="systemProgress"
      :error="systemError"
      :done="systemDone"
      :spent="systemSpent"
      @confirm="confirmSystem()"
      @close="finishSystem"
    />

    <SystemAuthDialog
      :open="authVisible"
      :message="authMessage"
      :error="authError"
      :busy="authBusy"
      :failures="authFailures"
      @submit="submitPassword"
      @cancel="closeSystemState"
    />
  </Teleport>
</template>
