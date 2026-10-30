import type {
  AuthKind,
  CloseBehavior,
  DesktopBarMode,
  DesktopDockMode,
  ServerStatus,
} from './types';

export const APP_VERSION = '0.13.0';
export const APP_BUILD = '2026.10.30';
export const HISTORY_POINTS = 120;
export const MAX_LOGS = 80;

export const STATUS_META: Record<ServerStatus, { label: string; tone: string }> = {
  online: { label: '在线', tone: 'success' },
  warning: { label: '需关注', tone: 'warning' },
  error: { label: '连接失败', tone: 'error' },
  offline: { label: '已离线', tone: 'default' },
  connecting: { label: '连接中', tone: 'info' },
};

export const AUTH_LABELS: Record<AuthKind, string> = {
  key: 'SSH 私钥',
  password: '密码',
  agent: 'SSH Agent',
};

export const isCloseBehavior = (value: unknown): value is CloseBehavior =>
  value === 'ask' || value === 'tray' || value === 'quit';

export const isDesktopBarMode = (value: unknown): value is DesktopBarMode =>
  value === 'floating' || value === 'full';

export const isDesktopDockMode = (value: unknown): value is DesktopDockMode =>
  value === 'floating' || value === 'full';

export const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
export const round = (value: number, digits = 0) => {
  const p = 10 ** digits;
  return Math.round(value * p) / p;
};
export const uid = (prefix = 'id') =>
  `${prefix}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 7)}`;
