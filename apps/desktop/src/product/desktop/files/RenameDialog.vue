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
  currentName: string;
}>();

const emit = defineEmits<{
  close: [];
  renamed: [path: string];
  needPermit: [];
  viewAudit: [];
}>();

const name = ref(props.currentName);
const parent = props.path.slice(0, props.path.lastIndexOf('/')) || '/';
const expectedTarget = () =>
  parent === '/' ? `/${name.value.trim()}` : `${parent}/${name.value.trim()}`;

const { preview, busy, failure, expired, remaining, runPreview, runExecute, reset } =
  useWriteFlow(() => props.serverId, 'rename', expectedTarget);

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    name.value = props.currentName;
    reset();
  },
);

async function onPreview() {
  await runPreview({ path: props.path, newName: name.value.trim() });
}

async function onExecute() {
  const result = await runExecute();
  if (!result) return;
  emit('renamed', result.path);
  emit('close');
}
</script>

<template>
  <WriteConfirmDialog
    class="rename-dialog"
    :open="open"
    title="重命名"
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
    <label class="rename-dialog__field">
      <span>新名称</span>
      <LgInput
        v-model:value="name"
        :input-props="{ 'aria-label': '新名称', autocomplete: 'off', spellcheck: false }"
        :maxlength="255"
        @keydown.enter="onPreview"
      />
    </label>
    <p class="rename-dialog__note">名称不能包含 / \、NUL 或控制字符，且不能使用 . 或 ..。</p>
  </WriteConfirmDialog>
</template>

<style scoped>
.rename-dialog__field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: var(--lg-fs-sm);
}

.rename-dialog__note {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-xs, 12px);
}
</style>