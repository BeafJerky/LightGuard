import type { ServerStatus } from '../types';

export type DesktopAppKey =
  | 'about'
  | 'performance'
  | 'disks'
  | 'network'
  | 'processes'
  | 'connection'
  | 'terminal';

export interface DesktopAppMeta {
  key: DesktopAppKey;
  title: string;
  description: string;
  icon: string;
  tile: string;
  width: number;
  height: number;
}

export const DESKTOP_APPS: DesktopAppMeta[] = [
  {
    key: 'about',
    title: '系统信息',
    description: '硬件、系统与安全信息',
    icon: 'info',
    tile: 'tile-about',
    width: 780,
    height: 560,
  },
  {
    key: 'performance',
    title: '性能监视器',
    description: '实时 CPU、内存与网络',
    icon: 'activity',
    tile: 'tile-performance',
    width: 840,
    height: 580,
  },
  {
    key: 'disks',
    title: '磁盘管理',
    description: '挂载点与空间占用',
    icon: 'disk',
    tile: 'tile-disks',
    width: 820,
    height: 550,
  },
  {
    key: 'network',
    title: '网络',
    description: '网卡、地址与路由',
    icon: 'network',
    tile: 'tile-network',
    width: 790,
    height: 530,
  },
  {
    key: 'processes',
    title: '进程',
    description: '进程快照与采样日志',
    icon: 'cpu',
    tile: 'tile-processes',
    width: 900,
    height: 580,
  },
  {
    key: 'connection',
    title: '连接信息',
    description: '当前 SSH 会话详情',
    icon: 'key',
    tile: 'tile-connection',
    width: 720,
    height: 570,
  },
  {
    key: 'terminal',
    title: '终端',
    description: '完整交互 Shell',
    icon: 'terminal',
    tile: 'tile-terminal',
    width: 760,
    height: 500,
  },
];

export const DESKTOP_APP_KEYS = DESKTOP_APPS.map((app) => app.key);

export const desktopIconItems = DESKTOP_APPS.map((app) => ({
  name: app.icon,
  label: app.title,
}));

export function appMeta(key: DesktopAppKey): DesktopAppMeta {
  return DESKTOP_APPS.find((app) => app.key === key) ?? DESKTOP_APPS[0];
}

export function statusTone(status: ServerStatus): 'online' | 'warning' | 'danger' | 'muted' | 'info' {
  if (status === 'warning') return 'warning';
  if (status === 'error') return 'danger';
  if (status === 'offline') return 'muted';
  if (status === 'connecting') return 'info';
  return 'online';
}

export function statusLabel(status: ServerStatus): string {
  return {
    online: '在线',
    warning: '需关注',
    error: '连接失败',
    offline: '已离线',
    connecting: '连接中',
  }[status];
}
