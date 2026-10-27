<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { LgButton, LgIcon, LgInput, LgModal } from '@lightguard/lightui';
import type { Server } from './types';
import { backendAvailable, updateCredentials } from './store';
import type { BackendAuth } from './ssh';

const props = defineProps<{ server: Server | null }>();

const emit = defineEmits<{
  close: [];
  saved: [];
}>();

const auth = ref<BackendAuth>('password');
const secret = ref('');
const passphrase = ref('');
const showSecret = ref(false);
const busy = ref(false);
const error = ref('');

const isKey = computed(() => auth.value === 'key');

const canSave = computed(() => {
  if (isKey.value) return secret.value.includes('PRIVATE KEY');
  return secret.value.length > 0;
});

watch(
  () => props.server,
  (server) => {
    if (!server) return;
    auth.value = server.auth === 'key' ? 'key' : 'password';
    secret.value = '';
    passphrase.value = '';
    showSecret.value = false;
    error.value = '';
    busy.value = false;
  },
  { immediate: true },
);

async function onSave() {
  const server = props.server;
  if (!server || busy.value || !canSave.value) return;
  busy.value = true;
  error.value = '';
  const updated = await updateCredentials(server.id, auth.value, secret.value, passphrase.value);
  busy.value = false;
  secret.value = '';
  passphrase.value = '';
  if (!updated) {
    error.value = '服务器已不存在。';
    return;
  }
  if (updated.status === 'online' || updated.status === 'warning') {
    emit('saved');
    return;
  }
  error.value = updated.error
    ? `${updated.error.message}（${updated.error.code}）${updated.error.hint}`
    : '重新连接失败，请检查凭据后重试。';
}

function onShow(value: boolean) {
  if (!value && !busy.value) emit('close');
}

function onAuth(value: BackendAuth) {
  auth.value = value;
  secret.value = '';
  passphrase.value = '';
  error.value = '';
}
</script>

<template>
  <LgModal
    :show="Boolean(server)"
    preset="card"
    class="credentials"
    :bordered="false"
    :mask-closable="false"
    :close-on-esc="!busy"
    title="更新凭据"
    @update:show="onShow"
  >
    <div v-if="server" class="credentials__body">
      <p class="credentials__target">
        <LgIcon name="server" :size="15" />
        <span>{{ server.user }}@{{ server.ip }}:{{ server.port }}</span>
      </p>

      <div v-if="!backendAvailable" class="credentials__note is-warning">
        <LgIcon name="alert" :size="15" />
        <span>真实 SSH 仅桌面版可用。</span>
      </div>

      <div class="credentials__auth" role="group" aria-label="认证方式">
        <button
          v-for="option in [
            { key: 'key' as const, icon: 'key', label: 'SSH 私钥' },
            { key: 'password' as const, icon: 'lock', label: '密码' },
          ]"
          :key="option.key"
          type="button"
          class="credentials__auth-option"
          :class="{ 'is-active': auth === option.key }"
          :aria-pressed="auth === option.key"
          @click="onAuth(option.key)"
        >
          <LgIcon :name="option.icon" :size="16" />
          {{ option.label }}
        </button>
      </div>

      <label class="credentials__field">
        <span>{{ isKey ? '私钥内容' : '登录密码' }}</span>
        <div class="credentials__secret">
          <LgInput
            v-if="isKey"
            v-model:value="secret"
            type="textarea"
            :rows="4"
            :input-props="{ autocomplete: 'off', spellcheck: false }"
            placeholder="-----BEGIN OPENSSH PRIVATE KEY-----"
          />
          <LgInput
            v-else
            v-model:value="secret"
            :type="showSecret ? 'text' : 'password'"
            :input-props="{ autocomplete: 'new-password' }"
            placeholder="输入新的登录密码"
          />
          <button
            type="button"
            class="credentials__toggle"
            :aria-label="showSecret ? '隐藏内容' : '显示内容'"
            @click="showSecret = !showSecret"
          >
            <LgIcon :name="showSecret ? 'eye-off' : 'eye'" :size="15" />
          </button>
        </div>
      </label>

      <label v-if="isKey" class="credentials__field">
        <span>私钥口令</span>
        <LgInput
          v-model:value="passphrase"
          type="password"
          :input-props="{ autocomplete: 'new-password' }"
          placeholder="私钥未加密时留空"
        />
      </label>

      <div v-if="error" class="credentials__error">
        <LgIcon name="alert" :size="14" />
        <span>{{ error }}</span>
      </div>

      <div class="credentials__note">
        <LgIcon name="lock" :size="15" />
        <span>
          新凭据会加密写入系统凭据库并立即重连，不进入 localStorage、URL、日志或连接记录导出。
          旧凭据不会被读取或显示。
        </span>
      </div>
    </div>

    <template #footer>
      <div class="credentials__actions">
        <LgButton size="small" :disabled="busy" @click="emit('close')">取消</LgButton>
        <LgButton
          size="small"
          type="primary"
          :disabled="!canSave || !backendAvailable"
          :loading="busy"
          @click="onSave"
        >
          保存并重连
        </LgButton>
      </div>
    </template>
  </LgModal>
</template>

<style scoped>
.credentials {
  width: min(480px, calc(100vw - 28px));
}

.credentials__body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.credentials__target {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  padding: 8px 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text-secondary);
  font-family: var(--lg-font-mono);
  font-size: 11.5px;
  overflow-wrap: anywhere;
}

.credentials__auth {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.credentials__auth-option {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 9px 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text-muted);
  font-size: 12px;
  cursor: pointer;
}

.credentials__auth-option.is-active {
  border-color: color-mix(in srgb, var(--lg-primary) 42%, var(--lg-border));
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
  font-weight: 600;
}

.credentials__field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  color: var(--lg-text-secondary);
  font-size: 12px;
}

.credentials__secret {
  position: relative;
}

.credentials__toggle {
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

.credentials__toggle:hover {
  color: var(--lg-text);
}

.credentials__error {
  display: flex;
  align-items: flex-start;
  gap: 7px;
  padding: 9px 10px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
  font-size: 11.5px;
  line-height: 1.55;
}

.credentials__note {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 10px 11px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-info-soft);
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  line-height: 1.55;
}

.credentials__note.is-warning {
  background: var(--lg-warning-soft);
}

.credentials__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
