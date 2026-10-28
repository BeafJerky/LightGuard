import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { AlertsChangedEvent } from '../ssh';

const mocks = vi.hoisted(() => ({
  alertsList: vi.fn(),
  alertsIgnore: vi.fn(),
  alertsMute: vi.fn(),
  onAlertsChanged: vi.fn((_handler: (event: AlertsChangedEvent) => void) => () => {}),
}));

vi.mock('../ssh', () => ({
  ssh: mocks,
  toSshError: (error: unknown) =>
    error && typeof error === 'object' ? error : new Error('操作失败'),
}));

import {
  PAGE_SIZE,
  alertsState,
  filterStatuses,
  ignoreAlert,
  loadAlerts,
  loadMore,
  muteServer,
  resetAlerts,
  subscribeAlerts,
  validateIgnoreReason,
} from './store';

function item(id: number) {
  return {
    id,
    serverId: 'srv_1',
    serverName: 'web-01',
    ruleId: 'cpu',
    severity: 'warning' as const,
    status: 'active' as const,
    title: `事件 ${id}`,
    detail: '',
    firstSeenAt: id,
    lastSeenAt: id,
    resolvedAt: null,
    ignoreReason: '',
    ignoredUntil: null,
  };
}

beforeEach(() => {
  resetAlerts();
  vi.clearAllMocks();
});

describe('alerts store', () => {
  it('maps filter names to statuses', () => {
    expect(filterStatuses('open')).toEqual(['active']);
    expect(filterStatuses('handled')).toEqual(['resolved', 'ignored']);
    expect(filterStatuses('all')).toEqual([]);
  });

  it('validates ignore reasons', () => {
    expect(validateIgnoreReason('  ')).not.toBeNull();
    expect(validateIgnoreReason('x'.repeat(201))).not.toBeNull();
    expect(validateIgnoreReason(' 计划维护 ')).toBeNull();
  });

  it('loads the first page and appends more', async () => {
    const first = Array.from({ length: 200 }, (_, index) => item(index));
    mocks.alertsList.mockResolvedValueOnce({
      items: first,
      activeCount: 3,
      loginMonitor: [{ serverId: 'srv_1', available: false, reason: '无日志' }],
    });
    await loadAlerts();
    expect(alertsState.items).toHaveLength(200);
    expect(alertsState.activeCount).toBe(3);
    expect(alertsState.loginMonitor[0].available).toBe(false);
    expect(alertsState.exhausted).toBe(false);

    mocks.alertsList.mockResolvedValueOnce({
      items: [item(200), item(201)],
      activeCount: 3,
      loginMonitor: [],
    });
    await loadMore();
    expect(alertsState.items).toHaveLength(202);
    expect(alertsState.exhausted).toBe(true);
    expect(mocks.alertsList).toHaveBeenLastCalledWith('open', 200, 200);
  });

  it('de-duplicates appended pages by id and keeps the first occurrence', async () => {
    mocks.alertsList.mockResolvedValueOnce({
      items: Array.from({ length: PAGE_SIZE }, (_, index) => item(index + 1)),
      activeCount: PAGE_SIZE,
      loginMonitor: [],
    });
    await loadAlerts();
    expect(alertsState.offset).toBe(PAGE_SIZE);

    mocks.alertsList.mockResolvedValueOnce({
      items: [{ ...item(PAGE_SIZE), title: '旧标题' }, item(PAGE_SIZE + 1)],
      activeCount: PAGE_SIZE,
      loginMonitor: [],
    });
    await loadMore();
    expect(alertsState.items).toHaveLength(PAGE_SIZE + 1);
    expect(alertsState.items.map((alert) => alert.id).at(-1)).toBe(PAGE_SIZE + 1);
    expect(alertsState.items[PAGE_SIZE - 1].title).toBe(`事件 ${String(PAGE_SIZE)}`);
    // 游标按后端返回的行数推进，去重不会让翻页停在同一页上。
    expect(alertsState.offset).toBe(PAGE_SIZE + 2);
  });

  it('rejects blank ignore reasons without calling the backend', async () => {
    expect(await ignoreAlert(5, '   ', 1_700_000_000_000)).toBe(false);
    expect(mocks.alertsIgnore).not.toHaveBeenCalled();
    expect(alertsState.error).toBe('请填写忽略原因');

    expect(await ignoreAlert(5, 'x'.repeat(201), 1_700_000_000_000)).toBe(false);
    expect(mocks.alertsIgnore).not.toHaveBeenCalled();
    expect(alertsState.error).toBe('忽略原因不能超过 200 个字符');
  });

  it('passes ignore and mute arguments through', async () => {
    mocks.alertsIgnore.mockResolvedValueOnce(undefined);
    mocks.alertsMute.mockResolvedValueOnce(undefined);
    await ignoreAlert(9, '维护', 1_700_000_000_000);
    await muteServer('srv_1', null, 1_700_000_000_000);
    expect(mocks.alertsIgnore).toHaveBeenCalledWith(9, '维护', 1_700_000_000_000);
    expect(mocks.alertsMute).toHaveBeenCalledWith('srv_1', null, 1_700_000_000_000);
  });

  it('resets paging when the filter changes', async () => {
    mocks.alertsList.mockResolvedValueOnce({
      items: Array.from({ length: PAGE_SIZE }, (_, index) => item(index)),
      activeCount: 2,
      loginMonitor: [],
    });
    await loadAlerts();
    expect(alertsState.offset).toBe(PAGE_SIZE);

    mocks.alertsList.mockResolvedValueOnce({
      items: [item(1)],
      activeCount: 0,
      loginMonitor: [],
    });
    await loadAlerts('handled');
    expect(mocks.alertsList).toHaveBeenLastCalledWith('handled', PAGE_SIZE, 0);
    expect(alertsState.filter).toBe('handled');
    expect(alertsState.items).toHaveLength(1);
    expect(alertsState.offset).toBe(1);
    expect(alertsState.exhausted).toBe(true);
    expect(alertsState.activeCount).toBe(0);
  });

  it('stops paging once a page is short or a load is in flight', async () => {
    mocks.alertsList.mockResolvedValueOnce({ items: [item(1)], activeCount: 1, loginMonitor: [] });
    await loadAlerts();
    expect(alertsState.exhausted).toBe(true);
    await loadMore();
    expect(mocks.alertsList).toHaveBeenCalledTimes(1);
  });

  it('keeps the ignore reason trimmed and refreshes after mutations', async () => {
    mocks.alertsIgnore.mockResolvedValueOnce(undefined);
    mocks.alertsList.mockResolvedValueOnce({ items: [], activeCount: 0, loginMonitor: [] });
    expect(await ignoreAlert(3, '  计划维护  ', 1_700_000_000_000)).toBe(true);
    expect(mocks.alertsIgnore).toHaveBeenCalledWith(3, '计划维护', 1_700_000_000_000);
    expect(mocks.alertsList).toHaveBeenCalledTimes(1);
    expect(alertsState.error).toBe('');
  });

  it('records failures from list, ignore and mute', async () => {
    mocks.alertsList.mockRejectedValueOnce(
      Object.assign(new Error('本地数据库不可用'), { code: 'INTERNAL', hint: '' }),
    );
    await loadAlerts();
    expect(alertsState.error).toBe('本地数据库不可用');
    expect(alertsState.loading).toBe(false);

    mocks.alertsIgnore.mockRejectedValueOnce(
      Object.assign(new Error('忽略原因需为 1–200 个字符'), { code: 'INVALID_INPUT', hint: '' }),
    );
    expect(await ignoreAlert(4, 'x', 1)).toBe(false);
    expect(alertsState.error).toBe('忽略原因需为 1–200 个字符');

    mocks.alertsMute.mockRejectedValueOnce(
      Object.assign(new Error('忽略/静音时长需在 1 分钟到 7 天之间'), {
        code: 'INVALID_INPUT',
        hint: '',
      }),
    );
    expect(await muteServer('srv_1', 'cpu', 1)).toBe(false);
    expect(alertsState.error).toBe('忽略/静音时长需在 1 分钟到 7 天之间');
  });

  it('applies change events from a single shared subscription', () => {
    const stop = vi.fn();
    mocks.onAlertsChanged.mockImplementationOnce((_handler) => stop);
    const seen: number[] = [];
    const release = subscribeAlerts(() => seen.push(alertsState.activeCount));
    const releaseAgain = subscribeAlerts(() => seen.push(alertsState.activeCount));
    const releaseSilent = subscribeAlerts();
    expect(mocks.onAlertsChanged).toHaveBeenCalledTimes(1);

    const onChanged = mocks.onAlertsChanged.mock.calls[0][0];
    onChanged({ activeCount: 5 });
    expect(alertsState.activeCount).toBe(5);
    expect(seen).toEqual([5, 5]);

    release();
    release();
    expect(stop).not.toHaveBeenCalled();
    releaseSilent();
    expect(stop).not.toHaveBeenCalled();
    releaseAgain();
    expect(stop).toHaveBeenCalledTimes(1);
  });
});
