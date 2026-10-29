<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { LgButton, LgIcon, LgInput, LgModal } from '@lightguard/lightui';
import {
  discardPreview,
  executePreview,
  formatRemaining,
  guardState,
  isWriteActive,
  previewMkdir,
  refreshPermit,
} from '../../guard/store';
import { validateFolderName } from './folderName';
import { joinRemote } from './remotePath';

const props = defineProps<{
  open: boolean;
  serverId: string;
  // 弹层浮在整个桌面上：必须由对话框自己说明这是哪台主机，
  // 否则两窗口并排时用户无法从确认页判断文件夹建在哪里。
  serverName: string;
  dir: string;
}>();

const emit = defineEmits<{
  close: [];
  created: [name: string];
  needPermit: [];
  viewAudit: [];
}>();

type FailureKind = 'preview' | 'execute' | 'expired' | 'lost-audit' | 'busy' | 'permit';

interface Failure {
  kind: FailureKind;
  message: string;
  hint: string;
}

// 后端的两条「可能已经执行」路径（连不上时结果未知、执行成功但审计没落库）
// 只靠文案区分：它们都是 `WRITE_FAILED`，且都没有可回退的审计编号。
const MAY_HAVE_RUN = /可能已完成|可能已执行|没有审计编号/;

const name = ref('');
const error = ref('');
const running = ref(false);
const failure = ref<Failure | null>(null);
const nowMs = ref(Date.now());

// 预览不携带服务器身份，所以只认本对话框自己发起的那一条，
// 绝不把别的服务器（或别的目录）的预览拿来确认。
// 目标同样要逐字对上：将来出现第二种写操作时，kind/target 不匹配的预览不会被当成本次确认。
const expectedTarget = computed(() => joinRemote(props.dir, name.value.trim()));
const preview = computed(() => {
  const pending = guardState.preview;
  if (!pending) return null;
  if (guardState.previewServerId !== props.serverId) return null;
  if (pending.kind !== 'mkdir') return null;
  if (pending.target !== expectedTarget.value) return null;
  return pending;
});
const writeActive = computed(() => isWriteActive(props.serverId));
const expired = computed(() => Boolean(preview.value) && preview.value!.expiresAtMs <= nowMs.value);
const remaining = computed(() =>
  preview.value ? formatRemaining(preview.value.expiresAtMs, nowMs.value) : '',
);
const previewLabel = computed(() => (failure.value ? '重新预览' : '预览'));

const failureTitle = computed(() => {
  switch (failure.value?.kind) {
    case 'lost-audit':
      return '可能已完成，无法回退';
    case 'busy':
      return '写操作进行中';
    case 'permit':
      return '写许可已失效';
    case 'preview':
      return '预览失败';
    case 'expired':
      return '预览已过期';
    default:
      return '创建失败';
  }
});

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    name.value = '';
    error.value = '';
    failure.value = null;
    running.value = false;
    nowMs.value = Date.now();
    dropOwnPreview();
  },
);

// 预览与目录/服务器强绑定：切换目标后旧预览立即作废。
watch(
  () => [props.serverId, props.dir],
  () => {
    name.value = '';
    error.value = '';
    failure.value = null;
    dropOwnPreview();
  },
);

let ticker: number | undefined;

function stopTicker() {
  if (ticker === undefined) return;
  window.clearInterval(ticker);
  ticker = undefined;
}

// 倒计时只在有预览时跑；过期后按钮会变成「重新预览」，显示不能停在旧数字上。
watch(
  preview,
  (value) => {
    stopTicker();
    nowMs.value = Date.now();
    if (!value) return;
    ticker = window.setInterval(() => {
      nowMs.value = Date.now();
      if (!guardState.preview) stopTicker();
    }, 1000);
  },
  { immediate: true },
);

function dropOwnPreview() {
  if (guardState.previewServerId === props.serverId) discardPreview();
}

function close() {
  if (running.value) return;
  failure.value = null;
  dropOwnPreview();
  emit('close');
}

function backToEdit() {
  if (running.value) return;
  failure.value = null;
  error.value = '';
  dropOwnPreview();
}

function askPermit() {
  if (running.value) return;
  failure.value = null;
  dropOwnPreview();
  emit('needPermit');
  emit('close');
}

async function onPreview() {
  if (running.value || writeActive.value) return;
  const problem = validateFolderName(name.value);
  if (problem) {
    error.value = problem;
    return;
  }
  error.value = '';
  failure.value = null;
  running.value = true;
  const result = await previewMkdir(props.serverId, props.dir, name.value.trim());
  running.value = false;
  nowMs.value = Date.now();
  if (result) return;
  if (guardState.errorCode === 'PERMIT_REQUIRED') {
    askPermit();
    return;
  }
  // refreshPermit 会覆盖 guardState.error，先把本次错误留在本地。
  const message = guardState.error;
  const hint = guardState.errorHint;
  if (guardState.errorCode === 'WRITE_BUSY') await refreshPermit(props.serverId);
  failure.value = { kind: 'preview', message, hint };
}

async function failExecute() {
  const code = guardState.errorCode;
  const message = guardState.error;
  const hint = guardState.errorHint;
  // 许可到期或别的写操作占着这把锁：刷新许可状态，让用户看到「下一步该做什么」。
  if (code === 'WRITE_BUSY' || code === 'PERMIT_REQUIRED') {
    await refreshPermit(props.serverId);
  }
  const kind: FailureKind =
    code === 'PREVIEW_EXPIRED'
      ? 'expired'
      : code === 'WRITE_BUSY'
        ? 'busy'
        : code === 'PERMIT_REQUIRED'
          ? 'permit'
          : code === 'WRITE_FAILED' && MAY_HAVE_RUN.test(`${message}${hint}`)
            ? 'lost-audit'
            : 'execute';
  failure.value = { kind, message, hint };
}

async function onExecute() {
  // 倒计时每秒才跳一次，确认时可能已经越过 60 秒：先刷新本地时间再判断，避免拿过期预览去执行。
  nowMs.value = Date.now();
  if (!preview.value || running.value || writeActive.value || expired.value) return;
  const createdName = name.value.trim();
  running.value = true;
  const result = await executePreview();
  running.value = false;
  if (result) {
    failure.value = null;
    emit('created', createdName);
    emit('close');
    return;
  }
  // 失败的执行已经消费掉预览（后端先 take 再校验），这里只能回到「重新预览」。
  await failExecute();
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (!props.open || event.defaultPrevented || event.key !== 'Enter') return;
  if (!preview.value || running.value || expired.value || writeActive.value) return;
  // 按钮自己会响应 Enter；这里只补「焦点不在按钮上」的情况，避免执行两次。
  if (event.target instanceof Element && event.target.closest('button')) return;
  event.preventDefault();
  void onExecute();
}

function onShow(value: boolean) {
  if (!value) close();
}

window.addEventListener('keydown', onDocumentKeydown);

onBeforeUnmount(() => {
  stopTicker();
  window.removeEventListener('keydown', onDocumentKeydown);
  dropOwnPreview();
});
</script>

<template>
  <LgModal
    :show="open"
    preset="card"
    class="new-folder"
    :bordered="false"
    :mask-closable="!running"
    :close-on-esc="!running"
    title="新建文件夹"
    @update:show="onShow"
  >
    <div class="new-folder__body">
      <template v-if="!preview">
        <p class="new-folder__target">
          <LgIcon name="folder" :size="15" />
          <span>将在 {{ serverName }} 的 {{ dir }} 下创建</span>
        </p>
        <label class="new-folder__field">
          <span>文件夹名称</span>
          <LgInput
            v-model:value="name"
            :input-props="{ 'aria-label': '文件夹名称', autocomplete: 'off', spellcheck: false }"
            placeholder="例如 reports-2026"
            :maxlength="255"
            @keydown.enter="onPreview"
          />
        </label>
        <p v-if="error" class="new-folder__error" role="alert">{{ error }}</p>
        <p class="new-folder__note">名称不能包含 / \、NUL 或控制字符，且不能使用 . 或 ..；最长 255 字节。</p>
      </template>

      <template v-else>
        <dl class="new-folder__preview">
          <div class="new-folder__section">
            <dt>操作</dt>
            <dd>{{ preview.operation }}</dd>
          </div>
          <div class="new-folder__section">
            <dt>目标</dt>
            <dd>{{ serverName }} · {{ preview.target }}</dd>
          </div>
          <div class="new-folder__section">
            <dt>影响</dt>
            <dd>{{ preview.impact }}</dd>
          </div>
          <div class="new-folder__section">
            <dt>回退</dt>
            <dd>{{ preview.rollback }}</dd>
          </div>
        </dl>
        <p class="new-folder__note" :class="{ 'is-expired': expired }">
          {{
            expired
              ? '预览已过期，需要重新预览后才能执行。'
              : `预览剩余有效 ${remaining}，且只能执行一次。`
          }}
        </p>
      </template>

      <p v-if="writeActive" class="new-folder__note" role="status">
        该服务器有写操作进行中，完成后可继续。
      </p>

      <div v-if="failure" class="new-folder__failure" :class="`is-${failure.kind}`" role="alert">
        <strong>{{ failureTitle }}</strong>
        <span>{{ failure.message }}</span>
        <span v-if="failure.hint">{{ failure.hint }}</span>
        <div class="new-folder__failure-actions">
          <button
            v-if="failure.kind === 'permit'"
            type="button"
            data-action="need-permit"
            @click="askPermit"
          >
            去解锁
          </button>
          <button
            v-if="failure.kind !== 'expired'"
            type="button"
            data-action="view-audit"
            @click="emit('viewAudit')"
          >
            查看审计
          </button>
        </div>
      </div>
    </div>

    <template #footer>
      <div class="new-folder__actions">
        <LgButton size="small" :disabled="running" @click="close">取消</LgButton>
        <template v-if="!preview">
          <LgButton
            size="small"
            type="primary"
            data-action="preview"
            :loading="running"
            :disabled="running || writeActive"
            @click="onPreview"
          >
            {{ previewLabel }}
          </LgButton>
        </template>
        <template v-else-if="expired">
          <LgButton size="small" :disabled="running" @click="backToEdit">返回修改</LgButton>
          <LgButton
            size="small"
            type="primary"
            data-action="repreview"
            :loading="running"
            :disabled="running || writeActive"
            @click="onPreview"
          >
            重新预览
          </LgButton>
        </template>
        <template v-else>
          <LgButton size="small" :disabled="running" @click="backToEdit">返回修改</LgButton>
          <LgButton
            size="small"
            type="primary"
            data-action="confirm"
            :loading="running"
            :disabled="running || writeActive"
            @click="onExecute"
          >
            {{ running ? '正在创建…' : '确认创建' }}
          </LgButton>
        </template>
      </div>
    </template>
  </LgModal>
</template>

<style scoped>
.new-folder {
  width: min(560px, calc(100vw - 28px));
}

.new-folder__body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.new-folder__target {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: 12px;
  overflow-wrap: anywhere;
}

.new-folder__field {
  display: grid;
  gap: 6px;
  color: var(--lg-text-secondary);
  font-size: 12px;
}

.new-folder__error {
  margin: 0;
  color: var(--lg-danger);
  font-size: 11.5px;
}

.new-folder__note {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 11px;
  line-height: 1.55;
}

.new-folder__note.is-expired {
  color: var(--lg-warning);
}

.new-folder__preview {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
}

.new-folder__section {
  padding: 9px 12px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface-2);
}

.new-folder__section dt {
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.new-folder__section dd {
  margin: 4px 0 0;
  color: var(--lg-text);
  font-size: 12px;
  word-break: break-all;
}

.new-folder__failure {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border: var(--lg-border-width) solid color-mix(in srgb, var(--lg-danger) 36%, var(--lg-border));
  border-radius: var(--lg-radius-sm);
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
  font-size: 11.5px;
  line-height: 1.55;
}

/* 「可能已完成」既不是成功也不是干净的失败：用警告色，并把结论写在标题里。 */
.new-folder__failure.is-lost-audit,
.new-folder__failure.is-busy,
.new-folder__failure.is-permit,
.new-folder__failure.is-expired {
  border-color: color-mix(in srgb, var(--lg-warning) 36%, var(--lg-border));
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.new-folder__failure-actions {
  display: flex;
  gap: 8px;
}

.new-folder__failure button {
  height: 26px;
  margin-top: 4px;
  padding: 0 10px;
  border: var(--lg-border-width) solid currentColor;
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: inherit;
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.new-folder__failure button:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.new-folder__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
