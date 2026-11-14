<script setup lang="ts">
import { computed } from 'vue';
import { LgButton, LgModal } from '@lightguard/lightui';
import type { WritePreview } from '../../ssh';
import type { WriteFailure } from './useWriteFlow';

const props = defineProps<{
  open: boolean;
  title: string;
  serverName: string;
  targetLabel: string;
  preview: WritePreview | null;
  failure: WriteFailure | null;
  busy: boolean;
  expired: boolean;
  remaining: string;
  irreversible?: boolean;
  ack?: boolean;
  ackLabel?: string;
}>();

const emit = defineEmits<{
  'update:ack': [value: boolean];
  preview: [];
  confirm: [];
  close: [];
  needPermit: [];
  viewAudit: [];
}>();

const canConfirm = computed(
  () => Boolean(props.preview) && !props.busy && !props.expired && (!props.irreversible || props.ack),
);
const mayHaveRun = computed(() => props.failure?.kind === 'lost-audit');
</script>

<template>
  <LgModal
    :show="open"
    preset="card"
    class="write-confirm"
    :bordered="false"
    :mask-closable="!busy"
    :close-on-esc="!busy"
    :title="title"
    @update:show="(value: boolean) => { if (!value) emit('close'); }"
  >
    <div class="write-confirm__body">
      <p class="write-confirm__host">{{ serverName }} · {{ targetLabel }}</p>
      <slot />
      <dl v-if="preview" class="write-confirm__preview">
        <div>
          <dt>操作</dt>
          <dd>{{ preview.operation }}</dd>
        </div>
        <div>
          <dt>影响</dt>
          <dd>{{ preview.impact }}</dd>
        </div>
        <div>
          <dt>回退</dt>
          <dd>{{ preview.rollback }}</dd>
        </div>
      </dl>
      <p v-if="preview?.irreversible" class="write-confirm__danger" role="alert">
        此操作不可恢复，请确认目标无误。
      </p>
      <p v-if="preview" class="write-confirm__ttl" :class="{ 'is-expired': expired }">
        {{ expired ? '预览已过期，需要重新预览。' : `预览剩余有效 ${remaining}，且只能执行一次。` }}
      </p>
      <label v-if="irreversible" class="write-confirm__ack">
        <input
          type="checkbox"
          :checked="ack"
          :aria-label="ackLabel ?? '我了解此操作不可恢复'"
          @change="emit('update:ack', ($event.target as HTMLInputElement).checked)"
        />
        {{ ackLabel ?? '我了解此操作不可恢复' }}
      </label>
      <div v-if="failure" class="write-confirm__failure" role="alert">
        <strong>{{ mayHaveRun ? '可能已完成，无法回退' : preview ? '操作失败' : '预览失败' }}</strong>
        <p>{{ failure.message }}</p>
        <p v-if="failure.hint">{{ failure.hint }}</p>
        <button v-if="failure.kind === 'permit'" type="button" @click="emit('needPermit')">去解锁</button>
        <button v-if="mayHaveRun" type="button" @click="emit('viewAudit')">查看审计</button>
      </div>
    </div>
    <template #footer>
      <div class="write-confirm__actions">
        <LgButton size="small" data-action="cancel" :disabled="busy" @click="emit('close')">取消</LgButton>
        <LgButton
          v-if="!preview"
          size="small"
          type="primary"
          data-action="preview"
          :loading="busy"
          @click="emit('preview')"
        >
          {{ failure ? '重新预览' : '预览' }}
        </LgButton>
        <LgButton
          v-else-if="expired"
          size="small"
          type="primary"
          data-action="repreview"
          :loading="busy"
          :disabled="busy"
          @click="emit('preview')"
        >
          重新预览
        </LgButton>
        <LgButton
          v-else
          size="small"
          type="primary"
          data-action="confirm"
          :loading="busy"
          :disabled="!canConfirm"
          @click="emit('confirm')"
        >
          确认执行
        </LgButton>
      </div>
    </template>
  </LgModal>
</template>

<style scoped>
.write-confirm {
  width: min(560px, calc(100vw - 28px));
}

.write-confirm__body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.write-confirm__host {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  overflow-wrap: anywhere;
}

.write-confirm__preview {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 0;
}

.write-confirm__preview div {
  display: grid;
  grid-template-columns: 48px 1fr;
  gap: 10px;
}

.write-confirm__preview dt {
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
}

.write-confirm__preview dd {
  margin: 0;
  font-size: var(--lg-fs-sm);
  overflow-wrap: anywhere;
}

.write-confirm__danger {
  margin: 0;
  color: var(--lg-danger);
  font-size: var(--lg-fs-sm);
}

.write-confirm__ttl {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
}

.write-confirm__ttl.is-expired {
  color: var(--lg-danger);
}

.write-confirm__ack {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: var(--lg-fs-sm);
}

.write-confirm__failure {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-danger-soft);
  font-size: var(--lg-fs-sm);
}

.write-confirm__failure p {
  margin: 0;
}

.write-confirm__failure button {
  align-self: flex-start;
  padding: 0;
  border: 0;
  background: none;
  color: var(--lg-accent);
  font: inherit;
  cursor: pointer;
  text-decoration: underline;
}

.write-confirm__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>