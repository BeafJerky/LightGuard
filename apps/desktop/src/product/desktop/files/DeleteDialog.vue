<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { useWriteFlow } from './useWriteFlow';
import WriteConfirmDialog from './WriteConfirmDialog.vue';

const props = defineProps<{
  open: boolean;
  serverId: string;
  serverName: string;
  path: string;
  kind: 'file' | 'dir';
}>();

const emit = defineEmits<{
  close: [];
  deleted: [];
  needPermit: [];
  viewAudit: [];
}>();

const ack = ref(false);
const { preview, busy, failure, expired, remaining, runPreview, runExecute, reset } =
  useWriteFlow(() => props.serverId, 'delete', () => props.path);

const details = computed(() => {
  const value = preview.value?.details ?? {};
  return {
    entries: Number(value.entries ?? 1),
    bytes: Number(value.bytes ?? 0),
    truncated: Boolean(value.truncated),
  };
});

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    ack.value = false;
    reset();
  },
);

async function onPreview() {
  await runPreview({ path: props.path });
}

async function onExecute() {
  if (!(await runExecute())) return;
  emit('deleted');
  emit('close');
}
</script>

<template>
  <WriteConfirmDialog
    class="delete-dialog"
    :open="open"
    title="删除"
    :server-name="serverName"
    :target-label="path"
    :preview="preview"
    :failure="failure"
    :busy="busy"
    :expired="expired"
    :remaining="remaining"
    :irreversible="true"
    :ack="ack"
    ack-label="我了解删除后无法恢复"
    @update:ack="ack = $event"
    @preview="onPreview"
    @confirm="onExecute"
    @close="emit('close')"
    @need-permit="emit('needPermit')"
    @view-audit="emit('viewAudit')"
  >
    <p v-if="preview && details.truncated" class="delete-dialog__note">
      目录内容超过统计上限，至少 {{ details.entries }} 项。
    </p>
    <p v-else-if="preview" class="delete-dialog__note">
      将删除 {{ details.entries }} 项，共 {{ details.bytes }} 字节。
    </p>
    <p v-else class="delete-dialog__note">
      删除后无法恢复；目录会递归删除，并保留符号链接本身而不跟随。
    </p>
  </WriteConfirmDialog>
</template>

<style scoped>
.delete-dialog__note {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  line-height: 1.5;
}
</style>