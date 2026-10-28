import type { PermitStatus } from '../ssh';
import { formatRemaining } from './store';

export const EXPIRING_WINDOW_MS = 60_000;

export type PermitState = 'locked' | 'unlocked' | 'expiring';
export type PermitTone = 'neutral' | 'warning' | 'danger';

export interface PermitVisual {
  state: PermitState;
  icon: 'lock' | 'unlock';
  label: string;
  remaining: string;
  tone: PermitTone;
}

export function permitVisual(status: PermitStatus, nowMs = Date.now()): PermitVisual {
  if (status.locked) {
    return { state: 'locked', icon: 'lock', label: '已锁定', remaining: '', tone: 'neutral' };
  }
  const remainingMs = Math.max(0, status.expiresAtMs - nowMs);
  const remaining = formatRemaining(status.expiresAtMs, nowMs);
  if (remainingMs < EXPIRING_WINDOW_MS) {
    return { state: 'expiring', icon: 'unlock', label: remaining, remaining, tone: 'danger' };
  }
  return { state: 'unlocked', icon: 'unlock', label: remaining, remaining, tone: 'warning' };
}

export function permitAria(status: PermitStatus, nowMs = Date.now()): string {
  const visual = permitVisual(status, nowMs);
  if (visual.state === 'locked') return '写操作已锁定，点击解锁';
  if (visual.state === 'expiring') {
    return `写操作许可即将到期，剩余 ${visual.remaining}，点击管理`;
  }
  return `写操作已解锁，剩余 ${visual.remaining}，点击管理`;
}
