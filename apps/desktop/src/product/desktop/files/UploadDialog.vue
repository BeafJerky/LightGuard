<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Channel } from '@tauri-apps/api/core';
import { ssh, type UploadSource, type WriteProgress } from '../../ssh';
import { useWriteFlow } from './useWriteFlow';
import WriteConfirmDialog from './WriteConfirmDialog.vue';

const props = defineProps<{
  open: boolean;
  serverId: string;
  serverName: string;
  dir: string;
  files: UploadSource[];
}>();

const emit = defineEmits<{
  close: [];
  uploaded: [count: number];
  needPermit: [];
  viewAudit: [];
}>();

const policies = [
  { value: 'skip', label: '跳过' },
  { value: 'overwrite', label: '覆盖' },
  { value: 'rename', label: '自动改名' },
] as const;
const policy = ref<'skip' | 'overwrite' | 'rename'>('skip');
const progress = ref<WriteProgress | null>(null);
const { preview, busy, failure, expired, remaining, runPreview, runExecute, reset } =
  useWriteFlow(() => props.serverId, 'upload', () => props.dir);

const planItems = computed(() => {
  const files = preview.value?.details.files;
  return Array.isArray(files) ? (files as Record<string, unknown>[]) : [];
});

const actionLabel = (action: unknown) => {
  if (action === 'overwrite') return '覆盖';
  if (action === 'rename') return '改名';
  if (action === 'skip') return '跳过';
  return '新建';
};

watch(
  () => props.open,
  (open) => {
    if (!open) return;
    policy.value = 'skip';
    progress.value = null;
    reset();
  },
);

async function onPreview() {
  await runPreview({
    dir: props.dir,
    conflictPolicy: policy.value,
    files: props.files.map((file) => ({ path: file.path, name: file.name, size: file.size })),
  });
}

async function onExecute() {
  if (!preview.value) return;
  // 测试/Web 环境没有 Tauri IPC；只有真实运行时才创建 Channel。
  const channel =
    typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
      ? new Channel<WriteProgress>()
      : undefined;
  if (channel) {
    channel.onmessage = (event) => {
      progress.value = event;
    };
  }
  const result = await runExecute(channel);
  if (!result) return;
  emit('uploaded', planItems.value.filter((item) => item.action !== 'skip').length);
  emit('close');
}

function cancelUpload() {
  const id = preview.value?.previewId;
  if (id) void ssh.sftpTransferCancel(id);
}
</script>

<template>
  <WriteConfirmDialog
    class="upload-dialog"
    :open="open"
    title="上传文件"
    :server-name="serverName"
    :target-label="dir"
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
    <div class="upload-dialog__policy" role="group" aria-label="冲突策略">
      <span>同名文件</span>
      <button
        v-for="item in policies"
        :key="item.value"
        type="button"
        :data-policy="item.value"
        :class="{ 'is-active': policy === item.value }"
        :disabled="Boolean(preview)"
        @click="policy = item.value"
      >
        {{ item.label }}
      </button>
    </div>

    <ul class="upload-dialog__files">
      <li v-for="file in files" :key="file.path">
        <span class="upload-dialog__name">{{ file.name }}</span>
        <span class="upload-dialog__meta">{{ file.size }} B</span>
        <span
          v-for="item in planItems.filter((entry) => entry.name === file.name)"
          :key="String(item.remotePath)"
          class="upload-dialog__action"
          :class="`is-${item.action}`"
        >
          {{ actionLabel(item.action) }}
        </span>
      </li>
    </ul>

    <p v-if="policy === 'overwrite'" class="upload-dialog__danger">
      覆盖会直接替换同名文件，原内容无法恢复，且该文件不提供回退。
    </p>

    <div v-if="progress" class="upload-dialog__progress" role="status">
      <strong>{{ progress.fileName }}</strong>
      <span>
        {{ progress.done }} 成功 · {{ progress.failed }} 失败 ·
        {{ progress.transferred }} B
        <template v-if="progress.total"> / {{ progress.total }} B</template>
      </span>
    </div>

    <button v-if="busy" type="button" class="upload-dialog__cancel" @click="cancelUpload">
      取消上传
    </button>
  </WriteConfirmDialog>
</template>

<style scoped>
.upload-dialog__policy {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--lg-fs-sm);
}

.upload-dialog__policy button {
  padding: 4px 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  color: var(--lg-text);
  font: inherit;
  font-size: var(--lg-fs-sm);
  cursor: pointer;
}

.upload-dialog__policy button.is-active {
  border-color: var(--lg-accent);
  color: var(--lg-accent);
}

.upload-dialog__files {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 180px;
  margin: 0;
  padding: 0;
  overflow: auto;
  list-style: none;
  font-size: var(--lg-fs-sm);
}

.upload-dialog__files li {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
  border-bottom: 1px solid var(--lg-border);
}

.upload-dialog__name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.upload-dialog__meta,
.upload-dialog__action {
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-xs, 12px);
}

.upload-dialog__action.is-overwrite {
  color: var(--lg-danger);
}

.upload-dialog__danger {
  margin: 0;
  color: var(--lg-danger);
  font-size: var(--lg-fs-sm);
}

.upload-dialog__progress {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  font-size: var(--lg-fs-sm);
}

.upload-dialog__cancel {
  align-self: flex-start;
  padding: 0;
  border: 0;
  background: none;
  color: var(--lg-danger);
  font: inherit;
  cursor: pointer;
  text-decoration: underline;
}
</style>