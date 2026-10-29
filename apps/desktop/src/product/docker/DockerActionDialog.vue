<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import type { DockerPreview, DockerWriteResult } from './api';
import { remainingSeconds } from './actions';

const props = defineProps<{
  preview: DockerPreview;
  busy: boolean;
  progress: string[];
  error: string;
  done?: DockerWriteResult | null;
  spent?: boolean;
  cancellable?: boolean;
}>();
const emit = defineEmits<{ confirm: []; close: []; stop: [] }>();

const now = ref(Date.now());
let timer: number | null = null;

const left = computed(() => remainingSeconds(props.preview.expiresAtMs, now.value));
const finished = computed(() => Boolean(props.done) || Boolean(props.spent));
const expired = computed(() => !finished.value && left.value <= 0);
const succeeded = computed(() => props.done?.result === 'success');
const unknown = computed(() => props.done?.result === 'unknown');

onMounted(() => {
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
});

onBeforeUnmount(() => {
  if (timer !== null) window.clearInterval(timer);
});

function dismiss(): void {
  if (!props.busy) emit('close');
}

function primary(): void {
  if (finished.value) {
    emit('close');
    return;
  }
  if (props.busy || expired.value) return;
  emit('confirm');
}
</script>

<template>
  <div class="lgdocker-modal" role="dialog" aria-modal="true" aria-label="操作预览">
    <div class="lgdocker-modal__mask" @click="dismiss" />
    <section class="lgdocker-modal__panel">
      <header class="lgdocker-modal__head">
        <div>
          <strong>{{ preview.operation }}</strong>
          <span>{{ preview.kind }}</span>
        </div>
        <span v-if="!finished" class="lgdocker-countdown" :class="{ 'is-expired': expired }">
          <LgIcon name="clock" :size="12" />
          {{ expired ? '预览已过期' : `剩余 ${left} 秒` }}
        </span>
      </header>

      <div class="lgdocker-modal__body">
        <dl class="lgdocker-modal__facts">
          <div><dt>操作</dt><dd>{{ preview.operation }}</dd></div>
          <div><dt>目标</dt><dd>{{ preview.target }}</dd></div>
          <div><dt>影响</dt><dd>{{ preview.impact }}</dd></div>
          <div><dt>回退</dt><dd>{{ preview.rollback }}</dd></div>
        </dl>

        <ul v-if="preview.details.length > 0" class="lgdocker-list">
          <li v-for="(line, index) in preview.details" :key="index">{{ line }}</li>
        </ul>

        <p v-if="preview.irreversible" class="lgdocker-warn" role="note">
          <LgIcon name="alert" :size="13" />
          <span>此操作不可回退，请确认目标无误。</span>
        </p>

        <div
          v-if="busy && preview.kind === 'image.pull'"
          class="lgdocker-progressbar"
          role="progressbar"
          aria-label="拉取进度"
        >
          <span />
        </div>

        <div v-if="progress.length > 0" class="lgdocker-progress" role="log" aria-label="执行输出">
          <p v-for="(line, index) in progress" :key="index">{{ line }}</p>
        </div>

        <div v-if="error" class="lgdocker-error" role="alert">
          <LgIcon name="alert" :size="15" />
          <div><strong>执行提示</strong><span>{{ error }}</span></div>
        </div>

        <div v-else-if="unknown && done" class="lgdocker-warn" role="alert">
          <LgIcon name="alert" :size="13" />
          <span>结果未知，请刷新核对：{{ done.message }} · 审计 #{{ done.auditId }}</span>
        </div>

        <div v-else-if="succeeded && done" class="lgdocker-result" role="status">
          <LgIcon name="check" :size="14" />
          <span>{{ done.message }} · 审计 #{{ done.auditId }}</span>
        </div>

        <p v-if="expired" class="lgdocker-muted">预览已过期：关闭后重新发起操作会生成新的预览。</p>
      </div>

      <footer class="lgdocker-modal__foot">
        <button
          v-if="cancellable && busy"
          type="button"
          class="lgdocker-btn is-danger"
          @click="emit('stop')"
        >
          取消拉取
        </button>
        <button type="button" class="lgdocker-btn" :disabled="busy" @click="emit('close')">
          {{ finished ? '关闭' : '取消' }}
        </button>
        <button
          type="button"
          class="lgdocker-btn"
          :class="{ 'is-primary': !preview.irreversible, 'is-danger': preview.irreversible }"
          :disabled="busy || expired"
          @click="primary"
        >
          {{ busy ? '执行中…' : finished ? '完成' : '确认执行' }}
        </button>
      </footer>
    </section>
  </div>
</template>
