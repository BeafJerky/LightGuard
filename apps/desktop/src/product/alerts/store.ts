import { reactive } from 'vue';
import {
  ssh,
  toSshError,
  type AlertFilterName,
  type AlertItem,
  type LoginMonitor,
} from '../ssh';

export const PAGE_SIZE = 200;

export const alertsState = reactive({
  items: [] as AlertItem[],
  activeCount: 0,
  loginMonitor: [] as LoginMonitor[],
  filter: 'open' as AlertFilterName,
  offset: 0,
  exhausted: false,
  loading: false,
  error: '',
});

export function filterStatuses(filter: AlertFilterName): string[] {
  if (filter === 'open') return ['active'];
  if (filter === 'handled') return ['resolved', 'ignored'];
  return [];
}

export function validateIgnoreReason(reason: string): string | null {
  const trimmed = reason.trim();
  if (!trimmed) return '请填写忽略原因';
  if (Array.from(trimmed).length > 200) return '忽略原因不能超过 200 个字符';
  return null;
}

function mergeItems(existing: AlertItem[], incoming: AlertItem[]): AlertItem[] {
  const seen = new Set(existing.map((item) => item.id));
  const merged = [...existing];
  for (const alert of incoming) {
    if (seen.has(alert.id)) continue;
    seen.add(alert.id);
    merged.push(alert);
  }
  return merged;
}

async function fetchPage(filter: AlertFilterName, offset: number): Promise<void> {
  alertsState.loading = true;
  alertsState.error = '';
  try {
    const page = await ssh.alertsList(filter, PAGE_SIZE, offset);
    alertsState.filter = filter;
    // 翻页期间事件可能被重新排序，后端会重发已经取过的行：按 id 去重且保留先出现的那条。
    alertsState.items =
      offset === 0 ? mergeItems([], page.items) : mergeItems(alertsState.items, page.items);
    // offset 按后端实际返回的行数推进，去重不影响游标，否则重复页会让游标停住。
    alertsState.offset = offset + page.items.length;
    alertsState.activeCount = page.activeCount;
    if (page.loginMonitor.length) alertsState.loginMonitor = page.loginMonitor;
    alertsState.exhausted = page.items.length < PAGE_SIZE;
  } catch (error) {
    alertsState.error = toSshError(error).message;
  } finally {
    alertsState.loading = false;
  }
}

export async function loadAlerts(filter: AlertFilterName = alertsState.filter): Promise<void> {
  await fetchPage(filter, 0);
}

export async function loadMore(): Promise<void> {
  if (alertsState.exhausted || alertsState.loading) return;
  await fetchPage(alertsState.filter, alertsState.offset);
}

export async function ignoreAlert(alertId: number, reason: string, untilMs: number): Promise<boolean> {
  // 本地先校验：给不出原因就不必打扰后端，错误文案与后端保持同一套。
  const invalid = validateIgnoreReason(reason);
  if (invalid) {
    alertsState.error = invalid;
    return false;
  }
  try {
    await ssh.alertsIgnore(alertId, reason.trim(), untilMs);
    await loadAlerts();
    return true;
  } catch (error) {
    alertsState.error = toSshError(error).message;
    return false;
  }
}

export async function muteServer(
  serverId: string,
  ruleId: string | null,
  untilMs: number,
): Promise<boolean> {
  try {
    await ssh.alertsMute(serverId, ruleId, untilMs);
    await loadAlerts();
    return true;
  } catch (error) {
    alertsState.error = toSshError(error).message;
    return false;
  }
}

export function resetAlerts(): void {
  alertsState.items = [];
  alertsState.activeCount = 0;
  alertsState.loginMonitor = [];
  alertsState.filter = 'open';
  alertsState.offset = 0;
  alertsState.exhausted = false;
  alertsState.loading = false;
  alertsState.error = '';
}

// 与 guard store 同形：多个订阅共享一条后端监听，重复订阅不会重复应用，
// 最后一个订阅释放时才真正取消。
type ChangeListener = () => void;

const listeners = new Set<ChangeListener>();
let backendSubscription: (() => void) | null = null;
let subscribers = 0;

export function subscribeAlerts(onChange?: () => void): () => void {
  if (!backendSubscription) {
    backendSubscription = ssh.onAlertsChanged((event) => {
      alertsState.activeCount = event.activeCount;
      for (const listener of [...listeners]) listener();
    });
  }
  subscribers += 1;
  if (onChange) listeners.add(onChange);
  let released = false;
  return () => {
    if (released) return;
    released = true;
    if (onChange) listeners.delete(onChange);
    subscribers = Math.max(0, subscribers - 1);
    if (subscribers > 0) return;
    const stop = backendSubscription;
    backendSubscription = null;
    listeners.clear();
    stop?.();
  };
}
