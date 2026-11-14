<script setup lang="ts">
import { ref, watch } from 'vue';
import { LgInput } from '@lightguard/lightui';
import { useWriteFlow } from './useWriteFlow';
import WriteConfirmDialog from './WriteConfirmDialog.vue';

const props = defineProps<{
  open: boolean;
  serverId: string;
  serverName: string;
  path: string;
  currentMode: string;
}>();

const emit = defineEmits<{
  close: [];
  changed: [];
  needPermit: [];
  viewAudit: [];
}>();

const presets = ['644', '755', '600', '700'];
const mode = ref(props.currentMode);
const { preview, busy, failure, expired, remaining, runPreview, runExecute, reset } =
  useWriteFlow(() => props.serverId, 'chmod', () => props.path);

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    mode.value = props.currentMode;
    reset();
  },
);

async function onPreview() {
  await runPreview({ path: props.path, mode: mode.value.trim() });
}

async function onExecute() {
  if (!(await runExecute())) return;
  emit('changed');
  emit('close');
}
</script>

<template>
  <WriteConfirmDialog
    class="permission-dialog"
    :open="open"
    title="修改权限"
    :server-name="serverName"
    :target-label="path"
    :preview="preview"
    :failure="failure"
    :busy="busy"
    :expired="expired"
    :remaining="remaining"
    @preview="onPreview"
    @confirm="onExecute"
    @close="emit('close')"
    @need-permit="emit('needPermit')"
    @view-audit="emit('viewAudit')"
  >
    <label class="permission-dialog__field">
      <span>权限（八进制）</span>
      <LgInput
        v-model:value="mode"
        :input-props="{ 'aria-label': '权限', autocomplete: 'off', spellcheck: false }"
        :maxlength="4"
        @keydown.enter="onPreview"
      />
    </label>
    <div class="permission-dialog__presets">
      <button
        v-for="preset in presets"
        :key="preset"
        type="button"
        :data-preset="preset"
        :class="{ 'is-active': mode === preset }"
        @click="mode = preset"
      >
        {{ preset }}
      </button>
    </div>
    <p class="permission-dialog__note">只接受 000–777，不支持 setuid / setgid / sticky 特殊位。</p>
  </WriteConfirmDialog>
</template>

<style scoped>
.permission-dialog__field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: var(--lg-fs-sm);
}

.permission-dialog__presets {
  display: flex;
  gap: 8px;
}

.permission-dialog__presets button {
  padding: 4px 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  color: var(--lg-text);
  font: inherit;
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.permission-dialog__presets button.is-active {
  border-color: var(--lg-accent);
  color: var(--lg-accent);
}

.permission-dialog__note {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-xs, 12px);
}
</style>