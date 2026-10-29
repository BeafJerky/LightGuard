<script setup lang="ts">
import { nextTick, ref, watch } from 'vue';
import { LgIcon } from '@lightguard/lightui';

const props = defineProps<{
  open: boolean;
  message?: string;
  error?: string;
  busy?: boolean;
  failures?: number;
}>();
const emit = defineEmits<{ submit: [password: string]; cancel: [] }>();

const password = ref('');
const localError = ref('');
const field = ref<HTMLInputElement | null>(null);

watch(
  () => props.open,
  async (open) => {
    password.value = '';
    localError.value = '';
    if (open) {
      await nextTick();
      field.value?.focus();
    }
  },
);

function submit(): void {
  if (props.busy) return;
  if (password.value.length === 0) {
    localError.value = '请输入 sudo 密码';
    return;
  }
  localError.value = '';
  const value = password.value;
  // 提交后立刻清空输入框：失败时要求重新输入，避免密码停留在界面里。
  password.value = '';
  emit('submit', value);
}

function cancel(): void {
  password.value = '';
  emit('cancel');
}
</script>

<template>
  <div v-if="open" class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="系统授权">
    <div class="lgdocker-modal__mask" @click="cancel" />
    <form class="lgdocker-modal__panel" @submit.prevent="submit">
      <header class="lgdocker-modal__head">
        <div>
          <strong>需要系统授权</strong>
          <span>sudo 密码只用于本次操作</span>
        </div>
        <button type="button" class="lgdocker-btn is-small" aria-label="取消授权" @click="cancel">
          <LgIcon name="x" :size="13" />
        </button>
      </header>

      <div class="lgdocker-modal__body">
        <p class="lgdocker-muted">{{ message || '请输入服务器账号的 sudo 密码。' }}</p>
        <label class="lgdocker-field">
          <LgIcon name="key" :size="13" />
          <input
            ref="field"
            v-model="password"
            type="password"
            autocomplete="off"
            autocapitalize="off"
            spellcheck="false"
            placeholder="sudo 密码"
            aria-label="sudo 密码"
          />
        </label>
        <p v-if="localError || error" class="lgdocker-field-error" role="alert">
          {{ localError || error }}
        </p>
        <p class="lgdocker-muted">
          密码只保留在内存中并经 stdin 交给 sudo，不写入审计、日志或备份。
        </p>
        <p v-if="failures" class="lgdocker-field-error">
          已连续失败 {{ failures }}/3 次，达到 3 次将关闭授权框。
        </p>
      </div>

      <footer class="lgdocker-modal__foot">
        <button type="button" class="lgdocker-btn" :disabled="busy" @click="cancel">取消</button>
        <button type="submit" class="lgdocker-btn is-primary" :disabled="busy">
          {{ busy ? '授权中…' : '授权并继续' }}
        </button>
      </footer>
    </form>
  </div>
</template>
