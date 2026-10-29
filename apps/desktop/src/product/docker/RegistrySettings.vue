<script setup lang="ts">
import { computed, inject, reactive, ref, watch, type Ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { Server } from '../types';
import { toSshError } from '../ssh';
import { toast } from '../ui';
import { dockerApi, type RegistryConfig } from './api';
import { buildDaemonPayload, formFromConfig, type ProxyKey, type SettingsForm } from './settings';
import { useDockerSystem } from './useDockerAction';
import DockerActionDialog from './DockerActionDialog.vue';
import SystemAuthDialog from './SystemAuthDialog.vue';

const props = defineProps<{ server: Server; open: boolean }>();
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
const form = reactive<SettingsForm>({
  mirrors: '',
  httpProxy: '',
  httpsProxy: '',
  noProxy: '',
  clear: [],
});

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

const proxyRows: Array<{ key: ProxyKey; label: string; placeholder: string }> = [
  { key: 'httpProxy', label: 'HTTP 代理', placeholder: 'http://proxy:3128' },
  { key: 'httpsProxy', label: 'HTTPS 代理', placeholder: 'http://proxy:3128' },
  { key: 'noProxy', label: 'NO_PROXY', placeholder: 'localhost,127.0.0.1,10.0.0.0/8' },
];

function reset(): void {
  config.value = null;
  loading.value = false;
  loadError.value = '';
  localError.value = '';
  Object.assign(form, { mirrors: '', httpProxy: '', httpsProxy: '', noProxy: '', clear: [] });
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
    Object.assign(form, formFromConfig(value));
  } catch (caught) {
    if (request !== seq) return;
    loadError.value = toSshError(caught).message;
  } finally {
    if (request === seq) loading.value = false;
  }
}

/** 只刷新配置状态、不改写表单：用户已输入的代理值要保留下来。 */
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

function isCleared(key: ProxyKey): boolean {
  return form.clear.includes(key);
}

function toggleClear(key: ProxyKey): void {
  if (form.clear.includes(key)) {
    form.clear = form.clear.filter((item) => item !== key);
    return;
  }
  form.clear = [...form.clear, key];
  form[key] = '';
}

function editProxy(key: ProxyKey): void {
  if (form.clear.includes(key)) form.clear = form.clear.filter((item) => item !== key);
}

function placeholderFor(key: ProxyKey, fallback: string): string {
  if (isCleared(key)) return '保存后清除';
  if (proxyConfigured.value && !form[key]) return '已配置，留空保持不变';
  return fallback;
}

async function save(): Promise<void> {
  if (!config.value || loading.value || systemBusy.value || asking.value) return;
  localError.value = '';
  const draft = buildDaemonPayload(form, { mirrorsKnown: readable.value });
  if (draft.errors.length > 0) {
    localError.value = draft.errors[0];
    return;
  }
  let ready = false;
  asking.value = true;
  try {
    ready = await askSystem({ kind: 'daemon.update', payload: { ...draft.payload } });
  } finally {
    asking.value = false;
  }
  if (!ready && !authVisible.value) {
    localError.value = systemError.value || '无法生成配置预览';
    // daemon.json 在别处被改坏时只刷新只读状态，保留用户已输入的表单。
    if (errorCode.value === 'DAEMON_INVALID') void reloadConfig();
  }
}

function finishSystem(): void {
  const finished = systemDone.value;
  closeSystemState();
  if (finished?.result === 'success') {
    toast({
      title: '已应用并重启 Docker',
      message: 'daemon.json 已更新，可在预览日志中确认结果。',
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
    <div v-if="open" class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="仓库设置">
      <div class="lgdocker-modal__mask" @click="requestClose" />
      <section class="lgdocker-modal__panel is-wide">
        <header class="lgdocker-modal__head">
          <div>
            <strong>仓库设置</strong>
            <span>/etc/docker/daemon.json · 需要系统授权</span>
          </div>
          <button
            type="button"
            class="lgdocker-btn is-small"
            aria-label="关闭仓库设置"
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

            <template v-else>
              <div v-if="!readable" class="lgdocker-settings__notice">
                <LgIcon name="key" :size="14" />
                <div>
                  <strong>需要系统授权后查看</strong>
                  <span>{{ config.note || '保存时会用 sudo 重新读取并合并，已有配置不会被覆盖。' }}</span>
                </div>
              </div>

              <label class="lgdocker-settings__block">
                <span>镜像加速源</span>
                <textarea
                  v-model="form.mirrors"
                  rows="3"
                  spellcheck="false"
                  :placeholder="readable ? 'https://docker.mirrors.ustc.edu.cn' : '已配置，留空保持不变'"
                  aria-label="镜像加速源"
                />
                <small>一行一个地址，仅支持 http:// 或 https://；留空表示不配置加速源。</small>
              </label>

              <fieldset class="lgdocker-settings__group" :disabled="!proxySupported">
                <legend>代理</legend>
                <p v-if="!proxySupported" class="lgdocker-muted">
                  当前 Docker 版本低于 23.0，不支持 daemon.json 中的 proxies 配置。
                </p>
                <div v-for="row in proxyRows" :key="row.key" class="lgdocker-settings__proxy">
                  <label class="lgdocker-field">
                    <span>{{ row.label }}</span>
                    <input
                      v-model="form[row.key]"
                      type="text"
                      spellcheck="false"
                      :disabled="isCleared(row.key)"
                      :placeholder="placeholderFor(row.key, row.placeholder)"
                      :aria-label="row.label"
                      @input="editProxy(row.key)"
                    />
                  </label>
                  <button
                    type="button"
                    class="lgdocker-btn is-small"
                    :aria-label="'清除 ' + row.label"
                    :disabled="!isCleared(row.key) && !proxyConfigured"
                    @click="toggleClear(row.key)"
                  >
                    {{ isCleared(row.key) ? '撤销' : '清除' }}
                  </button>
                </div>
              </fieldset>

              <p v-if="config.note && readable" class="lgdocker-muted">{{ config.note }}</p>
              <p v-if="localError || systemError" class="lgdocker-field-error" role="alert">
                {{ localError || systemError }}
              </p>
              <p class="lgdocker-muted">
                保存前会展示将要修改的键与脱敏后的值，并重启 Docker；写入或重启失败会自动恢复原配置。
              </p>
            </template>
          </template>
        </div>

        <footer class="lgdocker-modal__foot">
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
            {{ asking ? '正在生成预览…' : systemBusy ? '处理中…' : '保存并应用' }}
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
