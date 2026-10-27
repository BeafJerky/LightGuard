<script setup lang="ts">
import { ref, watch } from 'vue';
import { LgButton, LgIcon, LgModal, LgSelect } from '@lightguard/lightui';
import type { AuthKind, Server } from './types';
import { AUTH_LABELS } from './data';

const props = defineProps<{ server: Server | null }>();

const emit = defineEmits<{
  close: [];
  save: [auth: AuthKind];
}>();

const auth = ref<AuthKind>('key');

watch(
  () => props.server,
  (server) => {
    if (server) auth.value = server.auth;
  },
  { immediate: true },
);

const options = (Object.keys(AUTH_LABELS) as AuthKind[]).map((key) => ({
  label: AUTH_LABELS[key],
  value: key,
}));

function onShow(value: boolean) {
  if (!value) emit('close');
}
</script>

<template>
  <LgModal
    :show="Boolean(server)"
    preset="card"
    class="credentials"
    :bordered="false"
    :mask-closable="false"
    title="更新凭据"
    @update:show="onShow"
  >
    <div v-if="server" class="credentials__body">
      <p class="credentials__target">
        <LgIcon name="server" :size="15" />
        <span>{{ server.user }}@{{ server.ip }}:{{ server.port }}</span>
      </p>

      <label class="credentials__field">
        <span>认证方式</span>
        <LgSelect v-model:value="auth" :options="options" />
      </label>

      <div class="credentials__note">
        <LgIcon name="lock" :size="15" />
        <span>
          演示环境不接收凭据内容。真实接入会把新凭据写回系统凭据库，不进入 localStorage、URL、日志或连接记录导出。
        </span>
      </div>
    </div>

    <template #footer>
      <div class="credentials__actions">
        <LgButton size="small" @click="emit('close')">取消</LgButton>
        <LgButton size="small" type="primary" @click="emit('save', auth)">保存并重连</LgButton>
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

.credentials__field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  color: var(--lg-text-secondary);
  font-size: 12px;
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

.credentials__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
