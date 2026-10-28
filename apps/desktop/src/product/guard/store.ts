import { reactive } from 'vue';
import {
  ssh,
  toSshError,
  type PermitEvent,
  type PermitStatus,
  type WriteEvent,
  type WritePreview,
  type WriteResult,
} from '../ssh';

export const guardState = reactive({
  // 写锁状态只存在许可条目里（`permitOf(id).writeActive`），不再单独维护一份表，
  // 否则 `ssh://permit` 与 `guard://write` 两个来源会各写各的。
  permits: {} as Record<string, PermitStatus>,
  preview: null as WritePreview | null,
  previewServerId: '',
  permitPanelOpen: false,
  busy: false,
  error: '',
  errorHint: '',
  errorCode: '',
});

export function permitOf(serverId: string): PermitStatus {
  return (
    guardState.permits[serverId] ?? {
      serverId,
      locked: true,
      minutes: 15,
      expiresAtMs: 0,
      writeActive: false,
    }
  );
}

export function applyPermit(status: PermitStatus): void {
  guardState.permits[status.serverId] = status;
}

export function applyPermitEvent(event: PermitEvent): void {
  applyPermit({ ...permitOf(event.serverId), ...event });
}

/**
 * `guard://write` 的独立载荷：只翻转该 server 的 `writeActive`。
 * 还没有许可条目时补一个“锁定 + 无剩余时长”的占位条目（与后端 locked 状态同形），
 * 之后收到的完整 `PermitStatus` 会整条覆盖它。
 */
export function applyWriteEvent(event: WriteEvent): void {
  const permit = guardState.permits[event.serverId];
  if (permit) {
    permit.writeActive = event.active;
    return;
  }
  guardState.permits[event.serverId] = {
    serverId: event.serverId,
    locked: true,
    minutes: 0,
    expiresAtMs: 0,
    writeActive: event.active,
  };
}

function fail(error: unknown): void {
  const parsed = toSshError(error);
  guardState.error = parsed.message;
  guardState.errorHint = parsed.hint;
  guardState.errorCode = parsed.code;
}

function clearError(): void {
  guardState.error = '';
  guardState.errorHint = '';
  guardState.errorCode = '';
}

function clearPreview(): void {
  guardState.preview = null;
  guardState.previewServerId = '';
}

export function isWriteActive(serverId: string): boolean {
  return permitOf(serverId).writeActive === true;
}

export function formatRemaining(expiresAtMs: number, nowMs = Date.now()): string {
  const seconds = Math.max(0, Math.ceil((expiresAtMs - nowMs) / 1000));
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`;
}

export async function refreshPermit(serverId: string): Promise<void> {
  clearError();
  try {
    applyPermit(await ssh.permitStatus(serverId));
  } catch (error) {
    fail(error);
  }
}

export async function unlockPermit(serverId: string, minutes: number): Promise<boolean> {
  guardState.busy = true;
  clearError();
  try {
    applyPermit(await ssh.permitUnlock(serverId, minutes));
    return true;
  } catch (error) {
    fail(error);
    return false;
  } finally {
    guardState.busy = false;
  }
}

export async function lockPermit(serverId: string): Promise<void> {
  clearError();
  try {
    applyPermit(await ssh.permitLock(serverId));
  } catch (error) {
    fail(error);
  }
}

export async function previewMkdir(
  serverId: string,
  dir: string,
  name: string,
): Promise<WritePreview | null> {
  guardState.busy = true;
  clearError();
  try {
    const preview = await ssh.writePreview({ serverId, dir, name });
    guardState.preview = preview;
    guardState.previewServerId = serverId;
    return preview;
  } catch (error) {
    fail(error);
    return null;
  } finally {
    guardState.busy = false;
  }
}

export async function executePreview(): Promise<WriteResult | null> {
  const preview = guardState.preview;
  if (!preview) return null;
  guardState.busy = true;
  clearError();
  try {
    const result = await ssh.writeExecute(preview.previewId);
    clearPreview();
    return result;
  } catch (error) {
    // 后端在 `write_execute` 里先 `take_preview` 再校验，失败的 id 已被消费，
    // 留着它只会让重试撞 `PREVIEW_EXPIRED` 并渲染过期的影响面，所以失败即清空。
    clearPreview();
    fail(error);
    return null;
  } finally {
    guardState.busy = false;
  }
}

export function discardPreview(): void {
  clearPreview();
}

export async function rollbackAudit(auditId: number): Promise<boolean> {
  guardState.busy = true;
  clearError();
  try {
    await ssh.writeRollback(auditId);
    return true;
  } catch (error) {
    fail(error);
    return false;
  } finally {
    guardState.busy = false;
  }
}

export function openPermitPanel(): void {
  guardState.permitPanelOpen = true;
}

export function closePermitPanel(): void {
  guardState.permitPanelOpen = false;
}

export function clearServer(serverId: string): void {
  delete guardState.permits[serverId];
  if (guardState.previewServerId === serverId) clearPreview();
}

export function clearAll(): void {
  resetGuard();
}

export function resetGuard(): void {
  guardState.permits = {};
  clearPreview();
  guardState.permitPanelOpen = false;
  guardState.busy = false;
  clearError();
}

// 事件订阅按引用计数共享：多个组件各自 subscribe 也只注册一组后端监听，
// 重复订阅不会重复应用同一条事件，最后一个订阅释放时才真正取消。
let backendSubscription: (() => void) | null = null;
let subscribers = 0;

function openSubscription(): () => void {
  const offPermit = ssh.onPermit(applyPermitEvent);
  const offWrite = ssh.onWrite(applyWriteEvent);
  return () => {
    offPermit();
    offWrite();
  };
}

export function subscribeGuard(): () => void {
  if (!backendSubscription) backendSubscription = openSubscription();
  subscribers += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    subscribers = Math.max(0, subscribers - 1);
    if (subscribers > 0) return;
    const stop = backendSubscription;
    backendSubscription = null;
    stop?.();
  };
}
