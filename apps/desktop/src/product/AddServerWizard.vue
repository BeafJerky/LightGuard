<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import {
  LgButton,
  LgCheckbox,
  LgIcon,
  LgInput,
  LgModal,
  LgSelect,
} from '@lightguard/lightui';
import type { AuthKind, ServerDraft } from './types';
import { addServer } from './store';
import { openServerDesktop } from './router';
import { toast, wizardOpen } from './ui';

type ProbeState = 'idle' | 'active' | 'ok' | 'fail';
type FailMode = 'none' | 'auth' | 'host';

interface Probe {
  label: string;
  state: ProbeState;
  ms: string;
}

const HOST_RE = /^[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?$/;
const GROUPS = ['生产', '预发', '边缘', '存储', 'CI', '未分组'].map((value) => ({
  label: value,
  value,
}));

const step = ref<1 | 2 | 3>(1);
const form = reactive({
  name: '',
  host: '',
  port: '22',
  user: 'root',
  group: '生产',
  role: '自定义接入',
  tags: '',
  note: '',
});
const auth = ref<AuthKind>('key');
const secret = ref('');
const passphrase = ref('');
const showSecret = ref(false);
const failMode = ref<FailMode>('none');
const busy = ref(false);
const result = ref<'ok' | 'fail' | null>(null);
const failure = ref<{ title: string; desc: string } | null>(null);
const createdId = ref('');
const duration = ref('');
const runToken = ref(0);

const errors = reactive<Record<string, string>>({});

const probeLabels = computed(() => [
  '解析主机地址',
  `建立 TCP 连接（${form.port || '22'} 端口）`,
  'SSH 握手 · 协商加密套件',
  '主机密钥校验 · 比对指纹',
  '身份认证 · 读取系统信息',
]);

const probes = reactive<Probe[]>(
  probeLabels.value.map((label) => ({ label, state: 'idle', ms: '' })),
);

const authLabel = computed(
  () => ({ key: 'SSH 私钥', password: '密码', agent: 'SSH Agent' })[auth.value],
);

const canSubmitStep2 = computed(() => {
  if (auth.value === 'agent') return true;
  if (auth.value === 'key') return secret.value.includes('PRIVATE KEY');
  return secret.value.length >= 6;
});

watch(
  () => probeLabels.value,
  (labels) => {
    labels.forEach((label, index) => {
      if (probes[index]) probes[index].label = label;
      else probes.push({ label, state: 'idle', ms: '' });
    });
  },
);

watch(wizardOpen, (open) => {
  if (open) reset();
  else {
    cancelRun();
    clearSecrets();
  }
});

onBeforeUnmount(() => cancelRun());

function reset() {
  cancelRun();
  step.value = 1;
  Object.assign(form, {
    name: '',
    host: '',
    port: '22',
    user: 'root',
    group: '生产',
    role: '自定义接入',
    tags: '',
    note: '',
  });
  auth.value = 'key';
  failMode.value = 'none';
  result.value = null;
  failure.value = null;
  createdId.value = '';
  duration.value = '';
  busy.value = false;
  showSecret.value = false;
  Object.keys(errors).forEach((key) => delete errors[key]);
  clearSecrets();
  resetProbes();
}

function clearSecrets() {
  secret.value = '';
  passphrase.value = '';
}

function resetProbes() {
  probes.splice(0, probes.length, ...probeLabels.value.map((label) => ({ label, state: 'idle' as ProbeState, ms: '' })));
}

function cancelRun() {
  runToken.value += 1;
  busy.value = false;
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function validateStep1() {
  Object.keys(errors).forEach((key) => delete errors[key]);
  if (!form.name.trim() || form.name.trim().length > 32) errors.name = '请输入 1–32 个字符的名称';
  if (!form.host.trim() || !HOST_RE.test(form.host.trim()) || form.host.trim().length > 253) {
    errors.host = '请输入有效的 IPv4 或域名';
  }
  const port = Number(form.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) errors.port = '端口需为 1–65535 的整数';
  if (!form.user.trim()) errors.user = '请输入登录用户';
  return Object.keys(errors).length === 0;
}

function validateStep2() {
  Object.keys(errors).forEach((key) => delete errors[key]);
  if (auth.value === 'key' && !secret.value.includes('PRIVATE KEY')) {
    errors.secret = '请粘贴 OpenSSH 私钥，或使用演示凭据';
  }
  if (auth.value === 'password' && secret.value.length < 6) {
    errors.secret = '密码至少需要 6 个字符，或使用演示凭据';
  }
  return Object.keys(errors).length === 0;
}

function useDemoCredentials() {
  if (auth.value === 'password') {
    secret.value = 'lightguard-demo';
    return;
  }
  secret.value =
    '-----BEGIN OPENSSH PRIVATE KEY-----\n' +
    'DEMO-ONLY-NOT-A-REAL-KEY-0000000000000000000000000000\n' +
    '-----END OPENSSH PRIVATE KEY-----';
}

function goNext() {
  if (step.value === 1) {
    if (!validateStep1()) return;
    step.value = 2;
    return;
  }
  if (step.value === 2) {
    if (!validateStep2()) return;
    step.value = 3;
    void connect();
  }
}

function goBack() {
  if (busy.value) return;
  step.value = step.value === 3 ? 2 : 1;
  result.value = null;
  failure.value = null;
  resetProbes();
}

function draft(): ServerDraft {
  return {
    name: form.name.trim(),
    ip: form.host.trim(),
    port: Number(form.port),
    user: form.user.trim(),
    auth: auth.value,
    group: form.group,
    tags: form.tags
      .split(/[,，\s]+/)
      .map((tag) => tag.trim())
      .filter(Boolean)
      .slice(0, 6),
    role: form.role.trim() || '自定义接入',
    note: form.note.trim(),
  };
}

async function connect() {
  if (busy.value) return;
  const token = ++runToken.value;
  busy.value = true;
  result.value = null;
  failure.value = null;
  createdId.value = '';
  duration.value = '';
  resetProbes();
  const started = performance.now();

  for (let index = 0; index < probes.length; index += 1) {
    if (token !== runToken.value) return;
    probes[index].state = 'active';
    await sleep(320 + Math.random() * 260);
    if (token !== runToken.value) return;

    const hostFailure = failMode.value === 'host' && index === 1;
    const authFailure = failMode.value === 'auth' && index === 4;
    if (hostFailure || authFailure) {
      probes[index].state = 'fail';
      probes[index].ms = `${Math.round(600 + Math.random() * 1800)}ms`;
      busy.value = false;
      result.value = 'fail';
      failure.value = hostFailure
        ? {
            title: '目标主机不可达 · HOST_UNREACHABLE',
            desc: 'TCP 连接在 5 秒内未完成。请确认地址、端口、安全组或 VPN / 跳板机状态，然后重试。',
          }
        : {
            title: '身份认证失败 · AUTH_FAILED',
            desc: '服务器拒绝了当前凭据。这是演示错误分支，关闭“模拟认证失败”后可重新连接。',
          };
      return;
    }

    probes[index].state = 'ok';
    probes[index].ms = `${Math.round(6 + Math.random() * 180)}ms`;
    await sleep(90);
  }

  if (token !== runToken.value) return;
  const server = addServer(draft());
  createdId.value = server.id;
  duration.value = `${((performance.now() - started) / 1000).toFixed(2)}s`;
  busy.value = false;
  result.value = 'ok';
  toast({ title: '服务器已添加', message: `${server.name} 已加入工作区`, tone: 'success' });
}

function finish() {
  if (!createdId.value) return;
  const id = createdId.value;
  wizardOpen.value = false;
  openServerDesktop(id);
}

function onShow(value: boolean) {
  if (!value) wizardOpen.value = false;
}
</script>

<template>
  <LgModal
    :show="wizardOpen"
    preset="card"
    class="wizard"
    :bordered="false"
    :mask-closable="false"
    :close-on-esc="!busy"
    :title="step === 3 && result === 'ok' ? '连接成功' : '添加服务器'"
    @update:show="onShow"
  >
    <div class="wizard__steps" aria-label="连接步骤">
      <div
        v-for="(label, index) in ['主机', '身份验证', '连接']"
        :key="label"
        class="wizard__step"
        :class="{ 'is-active': step === index + 1, 'is-done': step > index + 1 }"
        :aria-current="step === index + 1 ? 'step' : undefined"
      >
        <span class="wizard__step-bar"><i /></span>
        <span class="wizard__step-label">{{ index + 1 }}. {{ label }}</span>
      </div>
    </div>

    <section v-if="step === 1" class="wizard__body">
      <div class="wizard__grid">
        <label class="field span-2">
          <span class="field__label">显示名称 <em>*</em></span>
          <LgInput v-model:value="form.name" placeholder="例如 prod-web-07" :maxlength="32" :status="errors.name ? 'error' : undefined" />
          <span v-if="errors.name" class="field__error">{{ errors.name }}</span>
        </label>

        <label class="field span-2">
          <span class="field__label">主机地址 <em>*</em></span>
          <div class="field__row">
            <LgInput v-model:value="form.host" placeholder="192.168.1.10 或 server.example.com" :status="errors.host ? 'error' : undefined" />
            <LgInput v-model:value="form.port" class="field__port" placeholder="22" :status="errors.port ? 'error' : undefined" aria-label="SSH 端口" />
          </div>
          <span class="field__hint">支持 IPv4 与域名，端口默认 22</span>
          <span v-if="errors.host" class="field__error">{{ errors.host }}</span>
          <span v-if="errors.port" class="field__error">{{ errors.port }}</span>
        </label>

        <label class="field">
          <span class="field__label">登录用户 <em>*</em></span>
          <LgInput v-model:value="form.user" placeholder="root" :status="errors.user ? 'error' : undefined" />
          <span v-if="errors.user" class="field__error">{{ errors.user }}</span>
        </label>

        <label class="field">
          <span class="field__label">分组</span>
          <LgSelect v-model:value="form.group" :options="GROUPS" />
        </label>

        <label class="field">
          <span class="field__label">角色</span>
          <LgInput v-model:value="form.role" placeholder="自定义接入" />
        </label>

        <label class="field">
          <span class="field__label">标签</span>
          <LgInput v-model:value="form.tags" placeholder="以逗号分隔，如：生产, Web" />
        </label>

        <label class="field span-2">
          <span class="field__label">备注</span>
          <LgInput v-model:value="form.note" type="textarea" :rows="2" placeholder="可选，仅保存在本地配置中" />
        </label>
      </div>

      <div class="wizard__note">
        <LgIcon name="shield" :size="15" />
        <span>LightGuard 使用只读采集读取系统信息。演示环境不发起真实网络请求，所有数据仅保存在本浏览器。</span>
      </div>
    </section>

    <section v-else-if="step === 2" class="wizard__body">
      <div class="wizard__summary">
        <span><small>目标主机</small><strong>{{ form.user }}@{{ form.host }}:{{ form.port }}</strong></span>
        <span><small>显示名称</small><strong>{{ form.name }}</strong></span>
      </div>

      <div class="auth-toggle" role="group" aria-label="认证方式">
        <button
          v-for="option in [
            { key: 'key' as const, icon: 'key', title: 'SSH 私钥', desc: '推荐 · Ed25519 / RSA' },
            { key: 'password' as const, icon: 'lock', title: '密码', desc: '兼容旧服务器' },
            { key: 'agent' as const, icon: 'user', title: 'SSH Agent', desc: '使用本机代理' },
          ]"
          :key="option.key"
          type="button"
          class="auth-option"
          :class="{ 'is-active': auth === option.key }"
          :aria-pressed="auth === option.key"
          @click="auth = option.key; errors.secret = ''"
        >
          <LgIcon :name="option.icon" :size="17" />
          <span>
            <strong>{{ option.title }}</strong>
            <small>{{ option.desc }}</small>
          </span>
        </button>
      </div>

      <div v-if="auth !== 'agent'" class="field field--secret">
        <span class="field__label">
          {{ auth === 'key' ? '私钥内容' : '登录密码' }} <em>*</em>
        </span>
        <div class="secret">
          <LgInput
            v-if="auth === 'key'"
            v-model:value="secret"
            type="textarea"
            :rows="4"
            :input-props="{ autocomplete: 'off', spellcheck: false }"
            placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
            :status="errors.secret ? 'error' : undefined"
          />
          <LgInput
            v-else
            v-model:value="secret"
            :type="showSecret ? 'text' : 'password'"
            :input-props="{ autocomplete: 'new-password' }"
            placeholder="输入服务器登录密码"
            :status="errors.secret ? 'error' : undefined"
          />
          <button type="button" class="secret__toggle" :aria-label="showSecret ? '隐藏内容' : '显示内容'" @click="showSecret = !showSecret">
            <LgIcon :name="showSecret ? 'eye-off' : 'eye'" :size="15" />
          </button>
        </div>
        <span v-if="auth === 'key'" class="field__hint">私钥口令可选；凭据只在当前弹窗内存中存在，关闭即清除。</span>
        <span v-if="errors.secret" class="field__error">{{ errors.secret }}</span>
        <div class="field__tools">
          <LgButton size="tiny" @click="useDemoCredentials">使用演示凭据</LgButton>
          <span>请勿粘贴真实生产凭据。</span>
        </div>
      </div>

      <div v-else class="agent-note">
        <LgIcon name="key" :size="16" />
        <span>演示环境将直接通过本机 Agent 认证；真实接入由本地凭据库托管，不会进入浏览器存储。</span>
      </div>

      <div class="wizard__note">
        <LgIcon name="lock" :size="15" />
        <span>凭据不会写入 localStorage、URL、日志或连接记录；本页仅用于预览连接流程。</span>
      </div>

      <div class="wizard__options">
        <label class="field field--inline">
          <span class="field__label">演示错误分支</span>
          <LgSelect
            v-model:value="failMode"
            class="wizard__fail"
            :options="[
              { label: '不模拟失败', value: 'none' },
              { label: '模拟认证失败（AUTH_FAILED）', value: 'auth' },
              { label: '模拟主机不可达（HOST_UNREACHABLE）', value: 'host' },
            ]"
          />
        </label>
      </div>
    </section>

    <section v-else class="wizard__body">
      <div class="wizard__summary">
        <span><small>目标主机</small><strong>{{ form.user }}@{{ form.host }}:{{ form.port }}</strong></span>
        <span><small>认证方式</small><strong>{{ authLabel }}</strong></span>
        <span><small>显示名称</small><strong>{{ form.name }}</strong></span>
        <span><small>分组</small><strong>{{ form.group }}</strong></span>
      </div>

      <div class="probe" role="list" aria-label="连接进度">
        <div v-for="probe in probes" :key="probe.label" class="probe__row" :class="`is-${probe.state}`" role="listitem">
          <span class="probe__icon" aria-hidden="true">
            <LgIcon v-if="probe.state === 'ok'" name="check" :size="13" />
            <LgIcon v-else-if="probe.state === 'fail'" name="x" :size="13" />
            <span v-else-if="probe.state === 'active'" class="probe__spinner" />
          </span>
          <span class="probe__label">{{ probe.label }}</span>
          <span class="probe__ms">{{ probe.state === 'ok' || probe.state === 'fail' ? probe.ms : probe.state === 'active' ? '···' : '—' }}</span>
        </div>
      </div>

      <div v-if="result === 'ok'" class="result-card is-ok">
        <span class="result-card__icon"><LgIcon name="check" :size="17" /></span>
        <div>
          <strong>连接成功，已读取系统信息</strong>
          <p>{{ form.name }} · 只读采集完成。SSH 会话已加密，服务器桌面已准备好。</p>
          <div class="result-card__meta">
            <span>耗时 <strong>{{ duration || '—' }}</strong></span>
            <span>认证 <strong>{{ authLabel }}</strong></span>
          </div>
        </div>
      </div>

      <div v-else-if="result === 'fail' && failure" class="result-card is-fail">
        <span class="result-card__icon"><LgIcon name="alert" :size="17" /></span>
        <div>
          <strong>{{ failure.title }}</strong>
          <p>{{ failure.desc }}</p>
          <button type="button" class="result-card__link" @click="goBack">返回修改连接信息</button>
        </div>
      </div>
    </section>

    <template #footer>
      <div class="wizard__foot">
        <LgButton v-if="step < 3" size="small" @click="wizardOpen = false">取消</LgButton>
        <LgButton v-else-if="busy" size="small" @click="cancelRun">取消连接</LgButton>
        <LgButton v-else size="small" @click="goBack">返回修改</LgButton>

        <div class="wizard__foot-right">
          <LgButton v-if="step === 2" size="small" @click="goBack">上一步</LgButton>

          <LgButton v-if="step === 1" size="small" type="primary" @click="goNext">下一步</LgButton>
          <LgButton
            v-else-if="step === 2"
            size="small"
            type="primary"
            :disabled="!canSubmitStep2"
            @click="goNext"
          >
            <template #icon><LgIcon name="lock" :size="14" /></template>
            开始连接
          </LgButton>

          <LgButton v-else-if="busy" size="small" type="primary" loading disabled>连接中</LgButton>
          <template v-else-if="result === 'ok'">
            <LgButton size="small" @click="wizardOpen = false">留在控制台</LgButton>
            <LgButton size="small" type="primary" @click="finish">
              进入服务器桌面
              <template #icon><LgIcon name="arrow-right" :size="14" /></template>
            </LgButton>
          </template>
          <LgButton v-else size="small" type="primary" @click="connect">
            <template #icon><LgIcon name="refresh" :size="14" /></template>
            重新连接
          </LgButton>
        </div>
      </div>
    </template>
  </LgModal>
</template>

<style scoped>
.wizard {
  width: min(680px, calc(100vw - 28px));
}

.wizard__steps {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  margin-bottom: 18px;
}

.wizard__step {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.wizard__step-bar {
  height: 3px;
  overflow: hidden;
  border-radius: var(--lg-radius-full);
  background: var(--lg-hover);
}

.wizard__step-bar i {
  display: block;
  width: 0;
  height: 100%;
  background: var(--lg-primary);
  transition: width var(--lg-dur-slow) var(--lg-ease);
}

.wizard__step.is-active .wizard__step-bar i,
.wizard__step.is-done .wizard__step-bar i {
  width: 100%;
}

.wizard__step-label {
  color: var(--lg-text-muted);
  font-size: 11px;
}

.wizard__step.is-active .wizard__step-label {
  color: var(--lg-primary);
  font-weight: 600;
}

.wizard__step.is-done .wizard__step-label {
  color: var(--lg-text-secondary);
}

.wizard__body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.wizard__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.field.span-2 {
  grid-column: 1 / -1;
}

.field__label {
  color: var(--lg-text-secondary);
  font-size: 12px;
  font-weight: 500;
}

.field__label em {
  color: var(--lg-danger);
  font-style: normal;
}

.field__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 86px;
  gap: 8px;
}

.field__hint {
  color: var(--lg-text-muted);
  font-size: 11px;
}

.field__error {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--lg-danger);
  font-size: 11px;
}

.field__tools {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  color: var(--lg-text-muted);
  font-size: 11px;
}

.wizard__note {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 11px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-info-soft);
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  line-height: 1.55;
}

.wizard__summary {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.wizard__summary > span {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 8px 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
}

.wizard__summary small {
  color: var(--lg-text-muted);
  font-size: 10px;
}

.wizard__summary strong {
  color: var(--lg-text);
  font-family: var(--lg-font-mono);
  font-size: 11.5px;
  font-weight: 600;
  overflow-wrap: anywhere;
}

.auth-toggle {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.auth-option {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text-muted);
  text-align: left;
  cursor: pointer;
}

.auth-option > span {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.auth-option strong {
  color: var(--lg-text);
  font-size: 12px;
  font-weight: 600;
}

.auth-option small {
  font-size: 10px;
  line-height: 1.4;
}

.auth-option:hover {
  border-color: var(--lg-border-strong);
}

.auth-option.is-active {
  border-color: color-mix(in srgb, var(--lg-primary) 42%, var(--lg-border));
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.secret {
  position: relative;
}

.secret__toggle {
  position: absolute;
  top: 7px;
  right: 7px;
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border: 1px solid var(--lg-border);
  border-radius: 7px;
  background: var(--lg-surface);
  color: var(--lg-text-muted);
  cursor: pointer;
}

.secret__toggle:hover {
  color: var(--lg-text);
}

.agent-note {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 11px;
  border: 1px dashed var(--lg-border-strong);
  border-radius: var(--lg-radius-sm);
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  line-height: 1.55;
}

.wizard__options {
  padding-top: 2px;
}

.field--inline {
  flex-direction: row;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.wizard__fail {
  width: min(300px, 60vw);
}

.probe {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
}

.probe__row {
  display: grid;
  grid-template-columns: 22px minmax(0, 1fr) 64px;
  align-items: center;
  gap: 8px;
  padding: 6px 6px;
  border-radius: 6px;
  color: var(--lg-text-muted);
  font-size: 11.5px;
}

.probe__row.is-active {
  background: var(--lg-info-soft);
  color: var(--lg-text);
}

.probe__row.is-ok {
  color: var(--lg-text-secondary);
}

.probe__row.is-fail {
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.probe__icon {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  color: var(--lg-text-muted);
}

.probe__row.is-ok .probe__icon {
  background: var(--lg-success-soft);
  color: var(--lg-success);
}

.probe__row.is-fail .probe__icon {
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.probe__spinner {
  width: 11px;
  height: 11px;
  border: 2px solid var(--lg-info-soft);
  border-top-color: var(--lg-info);
  border-radius: 50%;
  animation: probe-spin 700ms linear infinite;
}

.probe__ms {
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: 10px;
  text-align: right;
}

.result-card {
  display: flex;
  gap: 10px;
  padding: 12px;
  border-radius: var(--lg-radius-sm);
  border: 1px solid var(--lg-border);
  background: var(--lg-bg-soft);
}

.result-card__icon {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 10px;
}

.result-card.is-ok {
  border-color: color-mix(in srgb, var(--lg-success) 34%, var(--lg-border));
  background: var(--lg-success-soft);
}

.result-card.is-ok .result-card__icon {
  background: var(--lg-success-soft);
  color: var(--lg-success);
}

.result-card.is-fail {
  border-color: color-mix(in srgb, var(--lg-danger) 34%, var(--lg-border));
  background: var(--lg-danger-soft);
}

.result-card.is-fail .result-card__icon {
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.result-card strong {
  color: var(--lg-text);
  font-size: 12.5px;
}

.result-card p {
  margin: 3px 0 0;
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  line-height: 1.55;
}

.result-card__meta {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  margin-top: 6px;
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.result-card__meta strong {
  color: var(--lg-text);
  font-family: var(--lg-font-mono);
  font-size: 11px;
}

.result-card__link {
  margin-top: 6px;
  padding: 0;
  border: 0;
  background: none;
  color: var(--lg-primary);
  font-size: 11px;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 3px;
}

.wizard__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  width: 100%;
}

.wizard__foot-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

@keyframes probe-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (max-width: 620px) {
  .wizard__grid,
  .wizard__summary,
  .auth-toggle {
    grid-template-columns: minmax(0, 1fr);
  }

  .field--inline {
    align-items: stretch;
    flex-direction: column;
  }

  .wizard__fail {
    width: 100%;
  }

  .wizard__foot {
    align-items: stretch;
    flex-direction: column-reverse;
  }

  .wizard__foot-right {
    justify-content: flex-end;
  }
}

@media (prefers-reduced-motion: reduce) {
  .probe__spinner {
    animation-duration: 2s;
  }
}
</style>
