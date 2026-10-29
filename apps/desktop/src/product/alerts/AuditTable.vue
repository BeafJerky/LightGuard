<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { LgIcon } from '@lightguard/lightui';
import { ssh, toSshError, type AuditEntry } from '../ssh';
import { guardState, rollbackAudit } from '../guard/store';
import { confirm, toast } from '../ui';
import {
  auditOpLabel,
  auditParamsSummary,
  auditResultLabel,
  canRollback,
  formatDurationMs,
} from './audit';

const PAGE = 200;

const rows = ref<AuditEntry[]>([]);
const loading = ref(false);
const error = ref('');
const failure = ref('');
const exhausted = ref(false);
const expanded = ref<number | null>(null);

function formatTime(ms: number): string {
  return new Date(ms).toLocaleString('zh-CN', { hour12: false });
}

async function load(reset = false): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  error.value = '';
  try {
    // 审计仓储按行返回、不做合并，本地行数就是后端游标。
    const page = await ssh.auditList(PAGE, reset ? 0 : rows.value.length);
    rows.value = reset ? page : [...rows.value, ...page];
    exhausted.value = page.length < PAGE;
  } catch (cause) {
    error.value = toSshError(cause).message;
    if (reset) exhausted.value = true;
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void load(true);
});

async function onRollback(row: AuditEntry): Promise<void> {
  const target = auditParamsSummary(row.params);
  const docker = row.op.startsWith('docker.container.');
  const ok = await confirm({
    title: docker ? '回退容器操作' : '回退新建文件夹',
    message: docker
      ? `将对 ${target} 执行相反操作（启动与停止互换）。回退会写入新的审计记录，且需要有效许可。`
      : `将删除 ${target}（仅空目录）。回退会写入新的审计记录，且需要有效许可。`,
    confirmLabel: '回退',
    tone: 'danger',
  });
  if (!ok) return;
  failure.value = '';
  if (await rollbackAudit(row.id)) {
    toast({
      title: '已回退',
      message: docker ? '已执行相反操作，审计已记录' : '空目录已删除，审计已记录',
      tone: 'success',
    });
    await load(true);
    return;
  }
  failure.value = guardState.errorHint || guardState.error || '请稍后重试';
  toast({
    title: '回退失败',
    message: failure.value,
    tone: 'error',
  });
}
</script>

<template>
  <section class="audit" aria-label="操作审计">
    <p v-if="error" class="audit__error" role="alert">{{ error }}</p>
    <p v-if="failure" class="audit__error" role="alert">回退失败：{{ failure }}</p>

    <div class="audit__head" aria-hidden="true">
      <span>时间</span>
      <span>服务器</span>
      <span>操作</span>
      <span>参数</span>
      <span>结果</span>
      <span>耗时</span>
      <span>许可来源</span>
    </div>

    <template v-for="row in rows" :key="row.id">
      <button
        type="button"
        class="audit__row"
        :class="`is-${row.result}`"
        :aria-expanded="expanded === row.id"
        @click="expanded = expanded === row.id ? null : row.id"
      >
        <span>{{ formatTime(row.atMs) }}</span>
        <span>{{ row.serverName }}</span>
        <span>{{ auditOpLabel(row.op) }}</span>
        <span class="audit__path">{{ auditParamsSummary(row.params) }}</span>
        <span>{{ auditResultLabel(row.result) }}</span>
        <span>{{ formatDurationMs(row.durationMs) }}</span>
        <span>{{ row.permitSource }}</span>
      </button>

      <div v-if="expanded === row.id" class="audit__detail">
        <dl>
          <div><dt>参数</dt><dd>{{ row.params }}</dd></div>
          <div><dt>结果</dt><dd>{{ auditResultLabel(row.result) }}</dd></div>
          <div><dt>耗时</dt><dd>{{ formatDurationMs(row.durationMs) }}</dd></div>
          <div><dt>许可来源</dt><dd>{{ row.permitSource }}</dd></div>
          <div v-if="row.rollbackOf !== null"><dt>回退自</dt><dd>#{{ row.rollbackOf }}</dd></div>
        </dl>
        <button
          v-if="canRollback(row, rows)"
          type="button"
          class="audit__rollback"
          @click.stop="onRollback(row)"
        >
          <LgIcon name="refresh" :size="12" />
          {{ row.op.startsWith('docker.container.') ? '回退（启动/停止互换）' : '回退（删除空目录）' }}
        </button>
      </div>
    </template>

    <p v-if="loading && !rows.length" class="audit__empty" role="status">正在加载审计记录…</p>
    <p v-else-if="!rows.length && !error" class="audit__empty">暂无操作审计记录</p>
    <button
      v-if="!exhausted && rows.length"
      type="button"
      class="audit__more"
      :disabled="loading"
      @click="load()"
    >
      {{ loading ? '加载中…' : '加载更多' }}
    </button>
  </section>
</template>

<style scoped>
.audit {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.audit__head,
.audit__row {
  display: grid;
  grid-template-columns: 150px 110px 130px minmax(0, 1fr) 76px 72px 92px;
  gap: 10px;
  align-items: center;
  text-align: left;
}

.audit__head {
  padding: 0 12px 6px;
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.audit__row {
  width: 100%;
  padding: 9px 12px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  cursor: pointer;
}

.audit__row:hover {
  border-color: var(--lg-border-strong);
}

.audit__row:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 1px;
}

.audit__row.is-failed,
.audit__row.is-unknown {
  border-color: color-mix(in srgb, var(--lg-warning) 40%, var(--lg-border));
}

.audit__path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--lg-font-mono, ui-monospace, monospace);
}

.audit__detail {
  margin: -2px 0 4px;
  padding: 10px 14px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface-2);
}

.audit__detail dl {
  display: grid;
  gap: 4px;
  margin: 0;
  font-size: 11.5px;
}

.audit__detail dl div {
  display: flex;
  gap: 8px;
}

.audit__detail dt {
  flex: 0 0 64px;
  color: var(--lg-text-muted);
}

.audit__detail dd {
  margin: 0;
  color: var(--lg-text);
  word-break: break-all;
}

.audit__rollback {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  margin-top: 10px;
  padding: 0 10px;
  border: var(--lg-border-width) solid var(--lg-warning);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-warning);
  font-size: 11px;
  cursor: pointer;
}

.audit__rollback:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 1px;
}

.audit__empty {
  margin: 24px 0;
  color: var(--lg-text-muted);
  font-size: 12px;
  text-align: center;
}

.audit__error {
  margin: 0 0 8px;
  color: var(--lg-danger);
  font-size: 11.5px;
}

.audit__more {
  align-self: center;
  height: 30px;
  margin-top: 8px;
  padding: 0 16px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-secondary);
  font-size: 11.5px;
  cursor: pointer;
}

.audit__more:disabled {
  opacity: 0.6;
  cursor: default;
}

@media (max-width: 1100px) {
  .audit__head {
    display: none;
  }

  .audit__row {
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 4px 10px;
  }

  .audit__row span:nth-child(4),
  .audit__row span:nth-child(6),
  .audit__row span:nth-child(7) {
    display: none;
  }
}
</style>
