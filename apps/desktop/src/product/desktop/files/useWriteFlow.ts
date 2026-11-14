import { computed, onBeforeUnmount, ref } from 'vue';
import type { Channel } from '@tauri-apps/api/core';
import {
  discardPreview,
  executePreview,
  guardState,
  previewWrite,
} from '../../guard/store';
import type { WriteKind, WritePreview, WriteProgress, WriteResult } from '../../ssh';

export type WriteFailureKind =
  | 'preview'
  | 'execute'
  | 'expired'
  | 'lost-audit'
  | 'busy'
  | 'permit';

export interface WriteFailure {
  kind: WriteFailureKind;
  message: string;
  hint: string;
}

const MAY_HAVE_RUN = /可能已完成|可能已执行|没有审计编号/;

function toFailure(
  code: string,
  message: string,
  hint: string,
  kind: 'preview' | 'execute',
): WriteFailure {
  if (code === 'PREVIEW_EXPIRED') return { kind: 'expired', message, hint };
  if (code === 'PERMIT_REQUIRED') return { kind: 'permit', message, hint };
  if (code === 'WRITE_BUSY') return { kind: 'busy', message, hint };
  if (kind === 'execute' && MAY_HAVE_RUN.test(`${message}${hint}`)) {
    return { kind: 'lost-audit', message, hint };
  }
  return { kind, message, hint };
}

/**
 * 三种写操作共用的预览状态机：只认本服务器、本 kind、本目标的预览，
 * 失败分类与许可/过期处理保持一致，避免每个对话框各写一份。
 */
export function useWriteFlow(
  serverId: () => string,
  kind: WriteKind,
  expectedTarget: () => string,
) {
  const failure = ref<WriteFailure | null>(null);
  const nowMs = ref(Date.now());
  const timer = window.setInterval(() => {
    nowMs.value = Date.now();
  }, 1000);
  onBeforeUnmount(() => window.clearInterval(timer));

  const busy = computed(() => guardState.busy);
  const preview = computed<WritePreview | null>(() => {
    const pending = guardState.preview;
    if (!pending || guardState.previewServerId !== serverId()) return null;
    if (pending.kind !== kind || pending.target !== expectedTarget()) return null;
    return pending;
  });
  const expired = computed(
    () => Boolean(preview.value) && preview.value!.expiresAtMs <= nowMs.value,
  );
  const remaining = computed(() => {
    if (!preview.value) return '';
    const left = Math.max(0, preview.value.expiresAtMs - nowMs.value);
    const seconds = Math.ceil(left / 1000);
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  });

  async function runPreview(payload: Record<string, unknown>): Promise<boolean> {
    failure.value = null;
    const result = await previewWrite(serverId(), kind, payload);
    if (result) return true;
    if (guardState.errorCode) {
      failure.value = toFailure(
        guardState.errorCode,
        guardState.error,
        guardState.errorHint,
        'preview',
      );
    }
    return false;
  }

  async function runExecute(onEvent?: Channel<WriteProgress>): Promise<WriteResult | null> {
    failure.value = null;
    const result = await executePreview(onEvent);
    if (result) return result;
    if (guardState.errorCode) {
      failure.value = toFailure(
        guardState.errorCode,
        guardState.error,
        guardState.errorHint,
        'execute',
      );
    }
    return null;
  }

  function reset(): void {
    discardPreview();
    failure.value = null;
  }

  return {
    preview,
    busy,
    failure,
    expired,
    remaining,
    nowMs,
    runPreview,
    runExecute,
    reset,
  };
}