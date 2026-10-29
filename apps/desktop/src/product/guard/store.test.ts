import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { PermitEvent, WriteEvent, WritePreview } from '../ssh';

const mocks = vi.hoisted(() => ({
  permitStatus: vi.fn(),
  permitUnlock: vi.fn(),
  permitLock: vi.fn(),
  writePreview: vi.fn(),
  writeExecute: vi.fn(),
  writeRollback: vi.fn(),
  onPermit: vi.fn((_handler: (event: PermitEvent) => void) => () => {}),
  onWrite: vi.fn((_handler: (event: WriteEvent) => void) => () => {}),
}));

vi.mock('../ssh', () => ({
  ssh: mocks,
  toSshError: (error: unknown) =>
    error && typeof error === 'object'
      ? error
      : new Error('操作失败'),
}));

import {
  applyPermit,
  applyPermitEvent,
  applyWriteEvent,
  clearAll,
  clearServer,
  closePermitPanel,
  discardPreview,
  executePreview,
  formatRemaining,
  guardState,
  isWriteActive,
  lockPermit,
  openPermitPanel,
  permitOf,
  previewMkdir,
  refreshPermit,
  resetGuard,
  rollbackAudit,
  subscribeGuard,
  unlockPermit,
} from './store';

function preview(): WritePreview {
  return {
    previewId: 'p1',
    kind: 'mkdir',
    path: '/data/reports',
    operation: '新建文件夹',
    target: '/data/reports',
    impact: '将在 /data/reports 创建文件夹',
    rollback: '可在审计中回退',
    irreversible: false,
    expiresAtMs: 1_700_000_060_000,
  };
}

beforeEach(() => {
  resetGuard();
  vi.clearAllMocks();
});

describe('guard store', () => {
  it('defaults to a 15 minute locked permit', () => {
    const status = permitOf('srv_1');
    expect(status.locked).toBe(true);
    expect(status.minutes).toBe(15);
    expect(status.expiresAtMs).toBe(0);
    expect(status.writeActive).toBe(false);
  });

  it('applies permit events verbatim onto the stored status', () => {
    applyPermit({ serverId: 'srv_1', locked: false, minutes: 30, expiresAtMs: 5, writeActive: true });
    applyPermitEvent({ serverId: 'srv_1', locked: false, minutes: 30, expiresAtMs: 9, writeActive: false });
    expect(permitOf('srv_1').expiresAtMs).toBe(9);
    expect(permitOf('srv_1').writeActive).toBe(false);
    expect(isWriteActive('srv_1')).toBe(false);
  });

  it('derives write-active from the permit entry for both sources', () => {
    applyPermitEvent({ serverId: 'srv_1', locked: false, minutes: 15, expiresAtMs: 42, writeActive: true });
    expect(permitOf('srv_1').writeActive).toBe(true);
    expect(isWriteActive('srv_1')).toBe(true);

    applyWriteEvent({ serverId: 'srv_1', active: false });
    expect(permitOf('srv_1').writeActive).toBe(false);
    expect(isWriteActive('srv_1')).toBe(false);

    // 还没有许可条目时先记下写状态：占位条目必须是锁定的，不能被伪造成解锁。
    applyWriteEvent({ serverId: 'srv_2', active: true });
    expect(permitOf('srv_2').writeActive).toBe(true);
    expect(isWriteActive('srv_2')).toBe(true);
    expect(permitOf('srv_2').locked).toBe(true);
    expect(permitOf('srv_2').minutes).toBe(0);
    expect(permitOf('srv_2').expiresAtMs).toBe(0);

    // 之后到达的完整状态整条覆盖占位条目。
    applyPermit({ serverId: 'srv_2', locked: false, minutes: 5, expiresAtMs: 7, writeActive: false });
    expect(permitOf('srv_2').locked).toBe(false);
    expect(permitOf('srv_2').minutes).toBe(5);
    expect(isWriteActive('srv_2')).toBe(false);
  });

  it('formats countdowns without going negative', () => {
    expect(formatRemaining(0, 0)).toBe('00:00');
    expect(formatRemaining(90_000, 1_000)).toBe('01:29');
    expect(formatRemaining(15 * 60_000, 0)).toBe('15:00');
    expect(formatRemaining(1_000, 5_000)).toBe('00:00');
  });

  it('stores unlock results and failures', async () => {
    mocks.permitUnlock.mockResolvedValueOnce({
      serverId: 'srv_1',
      locked: false,
      minutes: 5,
      expiresAtMs: 1,
      writeActive: false,
    });
    expect(await unlockPermit('srv_1', 5)).toBe(true);
    expect(permitOf('srv_1').locked).toBe(false);
    expect(guardState.error).toBe('');

    mocks.permitUnlock.mockRejectedValueOnce(
      Object.assign(new Error('写操作正在进行'), { code: 'WRITE_BUSY', hint: '等待' }),
    );
    expect(await unlockPermit('srv_1', 5)).toBe(false);
    expect(guardState.error).toBe('写操作正在进行');
    expect(guardState.errorCode).toBe('WRITE_BUSY');
    expect(guardState.errorHint).toBe('等待');
    expect(guardState.busy).toBe(false);
  });

  it('tracks write activity and clears per-server state', () => {
    applyPermit({ serverId: 'srv_1', locked: false, minutes: 5, expiresAtMs: 1, writeActive: true });
    expect(isWriteActive('srv_1')).toBe(true);
    clearServer('srv_1');
    expect(guardState.permits.srv_1).toBeUndefined();
    expect(isWriteActive('srv_1')).toBe(false);
    expect(permitOf('srv_1').locked).toBe(true);
  });

  it('refreshes and locks permits through the backend', async () => {
    mocks.permitStatus.mockResolvedValueOnce({
      serverId: 'srv_1',
      locked: false,
      minutes: 30,
      expiresAtMs: 99,
      writeActive: false,
    });
    await refreshPermit('srv_1');
    expect(permitOf('srv_1').minutes).toBe(30);
    expect(permitOf('srv_1').expiresAtMs).toBe(99);

    mocks.permitLock.mockResolvedValueOnce({
      serverId: 'srv_1',
      locked: true,
      minutes: 0,
      expiresAtMs: 0,
      writeActive: false,
    });
    await lockPermit('srv_1');
    expect(permitOf('srv_1').locked).toBe(true);
    expect(permitOf('srv_1').minutes).toBe(0);
  });

  it('records refresh failures instead of throwing', async () => {
    mocks.permitStatus.mockRejectedValueOnce(
      Object.assign(new Error('解锁需要活跃会话'), { code: 'SESSION_NOT_FOUND', hint: '请先连接' }),
    );
    await expect(refreshPermit('srv_1')).resolves.toBeUndefined();
    expect(guardState.error).toBe('解锁需要活跃会话');
    expect(guardState.errorCode).toBe('SESSION_NOT_FOUND');
  });

  it('keeps one preview at a time and consumes it on execute', async () => {
    mocks.writePreview.mockResolvedValueOnce(preview());
    mocks.writeExecute.mockResolvedValueOnce({ auditId: 7, path: '/data/reports' });

    expect(await previewMkdir('srv_1', '/data', 'reports')).toEqual(preview());
    expect(guardState.previewServerId).toBe('srv_1');
    expect(guardState.preview?.previewId).toBe('p1');

    expect(await executePreview()).toEqual({ auditId: 7, path: '/data/reports' });
    expect(mocks.writeExecute).toHaveBeenCalledWith('p1');
    expect(guardState.preview).toBeNull();
    expect(guardState.previewServerId).toBe('');
  });

  it('surfaces preview and execute failures', async () => {
    mocks.writePreview.mockRejectedValueOnce(
      Object.assign(new Error('目标已存在：/data/x'), { code: 'PREVIEW_FAILED', hint: '换个名字' }),
    );
    expect(await previewMkdir('srv_1', '/data', 'x')).toBeNull();
    expect(guardState.error).toBe('目标已存在：/data/x');
    expect(guardState.errorCode).toBe('PREVIEW_FAILED');
    expect(guardState.busy).toBe(false);

    mocks.writePreview.mockResolvedValueOnce(preview());
    await previewMkdir('srv_1', '/data', 'reports');
    mocks.writeExecute.mockRejectedValueOnce(
      Object.assign(new Error('预览已过期'), { code: 'PREVIEW_EXPIRED', hint: '重新预览' }),
    );
    expect(await executePreview()).toBeNull();
    expect(guardState.errorCode).toBe('PREVIEW_EXPIRED');
    expect(guardState.errorHint).toBe('重新预览');
    expect(guardState.error).toBe('预览已过期');
    // 后端在 write_execute 里先 take_preview，失败的 id 已被消费，所以失败即清空预览，
    // 错误信息保留下来给 UI 展示，调用方直接重新预览即可。
    expect(guardState.preview).toBeNull();
    expect(guardState.previewServerId).toBe('');
    // discardPreview 只清预览，不清错误。
    discardPreview();
    expect(guardState.preview).toBeNull();
    expect(guardState.errorCode).toBe('PREVIEW_EXPIRED');
  });

  it('executes nothing without a preview', async () => {
    expect(await executePreview()).toBeNull();
    expect(mocks.writeExecute).not.toHaveBeenCalled();
  });

  it('reports rollback failures', async () => {
    mocks.writeRollback.mockResolvedValueOnce({ auditId: 8, path: '/data/reports' });
    expect(await rollbackAudit(7)).toBe(true);
    expect(mocks.writeRollback).toHaveBeenCalledWith(7);

    mocks.writeRollback.mockRejectedValueOnce(
      Object.assign(new Error('目标目录非空，无法回退'), { code: 'INVALID_INPUT', hint: '' }),
    );
    expect(await rollbackAudit(7)).toBe(false);
    expect(guardState.error).toBe('目标目录非空，无法回退');
  });

  it('opens and closes the permit panel and clears everything', () => {
    openPermitPanel();
    expect(guardState.permitPanelOpen).toBe(true);
    closePermitPanel();
    expect(guardState.permitPanelOpen).toBe(false);

    applyPermit({ serverId: 'srv_1', locked: false, minutes: 5, expiresAtMs: 1, writeActive: true });
    openPermitPanel();
    clearAll();
    expect(guardState.permits).toEqual({});
    expect(guardState.permitPanelOpen).toBe(false);
    expect(isWriteActive('srv_1')).toBe(false);
  });

  it('applies events from a single shared subscription', () => {
    const stopPermit = vi.fn();
    const stopWrite = vi.fn();
    mocks.onPermit.mockImplementationOnce((_handler) => stopPermit);
    mocks.onWrite.mockImplementationOnce((_handler) => stopWrite);

    const release = subscribeGuard();
    const releaseAgain = subscribeGuard();
    expect(mocks.onPermit).toHaveBeenCalledTimes(1);
    expect(mocks.onWrite).toHaveBeenCalledTimes(1);

    const onPermit = mocks.onPermit.mock.calls[0][0];
    onPermit({
      serverId: 'srv_1',
      locked: false,
      minutes: 15,
      expiresAtMs: 42,
      writeActive: false,
    });
    expect(permitOf('srv_1').locked).toBe(false);
    expect(permitOf('srv_1').expiresAtMs).toBe(42);

    const onWrite = mocks.onWrite.mock.calls[0][0];
    onWrite({ serverId: 'srv_1', active: true });
    expect(isWriteActive('srv_1')).toBe(true);
    expect(permitOf('srv_1').writeActive).toBe(true);
    onWrite({ serverId: 'srv_1', active: false });
    expect(isWriteActive('srv_1')).toBe(false);
    expect(permitOf('srv_1').writeActive).toBe(false);

    // 第一个订阅释放时监听仍然保留，重复释放不会误取消。
    release();
    release();
    expect(stopPermit).not.toHaveBeenCalled();
    releaseAgain();
    expect(stopPermit).toHaveBeenCalledTimes(1);
    expect(stopWrite).toHaveBeenCalledTimes(1);
  });
});
