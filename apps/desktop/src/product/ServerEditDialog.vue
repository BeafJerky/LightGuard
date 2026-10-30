<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { LgButton, LgIcon, LgInput, LgModal, LgSelect } from '@lightguard/lightui';
import type { Server } from './types';
import { backendAvailable, editServer, state } from './store';

const props = defineProps<{ server: Server | null }>();

const emit = defineEmits<{
  close: [];
  saved: [server: Server];
  credentials: [server: Server];
}>();

const HOST_RE = /^[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?$/;
const GROUP_PRESETS = ['生产', '预发', '边缘', '存储', 'CI', '未分组'];

const form = reactive({
  name: '',
  host: '',
  port: '22',
  user: '',
  group: '未分组',
  role: '',
  tags: '',
  note: '',
});
const errors = reactive<Record<string, string>>({});
const busy = ref(false);
const error = ref('');
const reconnectFailed = ref(false);

const groupOptions = computed(() => {
  const names = new Set(GROUP_PRESETS);
  for (const server of state.servers) if (server.group) names.add(server.group);
  return [...names].map((value) => ({ label: value, value }));
});

const connectionChanged = computed(() => {
  const server = props.server;
  if (!server) return false;
  return (
    server.ip !== form.host.trim() ||
    server.port !== Number(form.port) ||
    server.user !== form.user.trim()
  );
});

const wasConnected = computed(
  () => props.server?.status === 'online' || props.server?.status === 'warning',
);

const reconnectHint = computed(() => {
  if (!connectionChanged.value) return '';
  if (props.server?.status === 'connecting') return '主机正在连接，请等待连接结束后再修改连接信息。';
  return wasConnected.value
    ? '保存后将使用本机已保存的凭据自动重连；若凭据不匹配，需要随后更新凭据。'
    : '连接信息将在下次连接时生效。';
});

watch(
  () => props.server,
  (server) => {
    if (!server) return;
    Object.assign(form, {
      name: server.name,
      host: server.ip,
      port: String(server.port),
      user: server.user,
      group: server.group || '未分组',
      role: server.role,
      tags: server.tags.join(', '),
      note: server.note,
    });
    Object.keys(errors).forEach((key) => delete errors[key]);
    error.value = '';
    reconnectFailed.value = false;
    busy.value = false;
  },
  { immediate: true },
);

function validate() {
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

async function onSave() {
  const server = props.server;
  if (!server || busy.value) return;
  if (!validate()) return;
  if (connectionChanged.value && server.status === 'connecting') {
    error.value = '主机正在连接，请等待连接结束后再修改连接信息。';
    return;
  }
  const patch = {
    name: form.name.trim(),
    ip: form.host.trim(),
    port: Number(form.port),
    user: form.user.trim(),
    group: form.group.trim() || '未分组',
    role: form.role.trim() || '通用服务器',
    tags: form.tags
      .split(/[,，\s]+/)
      .map((tag) => tag.trim())
      .filter(Boolean)
      .slice(0, 6),
    note: form.note.trim(),
  };
  const needsReconnect = connectionChanged.value && wasConnected.value;
  busy.value = true;
  error.value = '';
  reconnectFailed.value = false;
  const result = await editServer(server.id, patch);
  busy.value = false;
  if (!result) {
    error.value = '服务器已不存在。';
    return;
  }
  if (needsReconnect && !result.reconnected) {
    reconnectFailed.value = true;
    error.value = result.server.error
      ? `连接信息已保存，但自动重连失败：${result.server.error.message}（${result.server.error.code}）${result.server.error.hint}`
      : '连接信息已保存，但自动重连失败。请稍后重试或更新凭据。';
    return;
  }
  emit('saved', result.server);
}

function onShow(value: boolean) {
  if (!value && !busy.value) emit('close');
}

function openCredentials() {
  if (!props.server || busy.value) return;
  emit('credentials', props.server);
}
</script>

<template>
  <LgModal
    :show="Boolean(server)"
    preset="card"
    class="server-edit"
    :bordered="false"
    :mask-closable="false"
    :close-on-esc="!busy"
    title="编辑服务器"
    @update:show="onShow"
  >
    <div v-if="server" class="server-edit__body">
      <p class="server-edit__target">
        <LgIcon name="server" :size="15" />
        <span>{{ server.hostname || server.name }}</span>
        <code>{{ server.authLabel }}</code>
      </p>

      <div class="server-edit__grid">
        <label class="field span-2">
          <span class="field__label">显示名称 <em>*</em></span>
          <LgInput
            v-model:value="form.name"
            placeholder="例如 prod-web-07"
            :maxlength="32"
            :status="errors.name ? 'error' : undefined"
          />
          <span v-if="errors.name" class="field__error">{{ errors.name }}</span>
        </label>

        <label class="field span-2">
          <span class="field__label">主机地址 <em>*</em></span>
          <div class="field__row">
            <LgInput
              v-model:value="form.host"
              placeholder="192.168.1.10 或 server.example.com"
              :status="errors.host ? 'error' : undefined"
            />
            <LgInput
              v-model:value="form.port"
              class="field__port"
              placeholder="22"
              :status="errors.port ? 'error' : undefined"
              aria-label="SSH 端口"
            />
          </div>
          <span v-if="errors.host" class="field__error">{{ errors.host }}</span>
          <span v-if="errors.port" class="field__error">{{ errors.port }}</span>
        </label>

        <label class="field">
          <span class="field__label">登录用户 <em>*</em></span>
          <LgInput
            v-model:value="form.user"
            placeholder="root"
            :status="errors.user ? 'error' : undefined"
          />
          <span v-if="errors.user" class="field__error">{{ errors.user }}</span>
        </label>

        <label class="field">
          <span class="field__label">分组</span>
          <LgSelect v-model:value="form.group" :options="groupOptions" />
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
          <LgInput
            v-model:value="form.note"
            type="textarea"
            :rows="2"
            placeholder="可选，仅保存在本地配置中"
          />
        </label>
      </div>

      <div v-if="reconnectHint" class="server-edit__note is-info">
        <LgIcon name="refresh" :size="15" />
        <span>{{ reconnectHint }}</span>
      </div>

      <div v-if="!backendAvailable" class="server-edit__note is-warning">
        <LgIcon name="alert" :size="15" />
        <span>真实 SSH 仅桌面版可用；当前仍可修改本地保存的配置。</span>
      </div>

      <div v-if="error" class="server-edit__error">
        <LgIcon name="alert" :size="15" />
        <span>{{ error }}</span>
        <LgButton v-if="reconnectFailed" size="small" ghost type="warning" @click="openCredentials">
          更新凭据
        </LgButton>
      </div>

      <div class="server-edit__note">
        <LgIcon name="shield" :size="15" />
        <span>这里只修改本地连接配置；登录凭据仍由系统凭据库加密保存，不会显示在此对话框。</span>
      </div>
    </div>

    <template #footer>
      <div class="server-edit__actions">
        <LgButton size="small" :disabled="busy" @click="emit('close')">取消</LgButton>
        <LgButton size="small" type="primary" :loading="busy" @click="onSave">保存修改</LgButton>
      </div>
    </template>
  </LgModal>
</template>

<style scoped>
.server-edit {
  width: min(640px, calc(100vw - 28px));
}

.server-edit__body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.server-edit__target {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  padding: 8px 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  overflow-wrap: anywhere;
}

.server-edit__target code {
  margin-left: auto;
  color: var(--lg-text-muted);
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-label);
  white-space: nowrap;
}

.server-edit__grid {
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
  font-size: var(--lg-fs-sm);
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

.field__error {
  color: var(--lg-danger);
  font-size: var(--lg-fs-sm);
}

.server-edit__note,
.server-edit__error {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 9px 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-info-soft);
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  line-height: 1.55;
}

.server-edit__note.is-warning {
  border-color: color-mix(in srgb, var(--lg-warning) 34%, var(--lg-border));
  background: var(--lg-warning-soft);
}

.server-edit__error {
  align-items: center;
  border-color: color-mix(in srgb, var(--lg-danger) 34%, var(--lg-border));
  background: var(--lg-danger-soft);
  color: var(--lg-text-secondary);
}

.server-edit__error span {
  flex: 1;
  min-width: 0;
}

.server-edit__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 620px) {
  .server-edit__grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
