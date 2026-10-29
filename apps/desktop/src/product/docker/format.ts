import type { DockerPort } from './api';

export type StateTone = 'online' | 'warning' | 'danger' | 'muted' | 'info';

export function stateLabel(state: string): string {
  const labels: Record<string, string> = {
    running: '运行中',
    exited: '已停止',
    created: '已创建',
    paused: '已暂停',
    restarting: '重启中',
    removing: '删除中',
    dead: '异常',
  };
  return labels[state] ?? (state || '未知');
}

export function stateTone(state: string): StateTone {
  if (state === 'running') return 'online';
  if (state === 'paused' || state === 'restarting' || state === 'created') return 'warning';
  if (state === 'dead') return 'danger';
  if (state === 'removing') return 'info';
  return 'muted';
}

export function percent(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)}%`;
}

export function dockerTime(value: string): string {
  const trimmed = value.trim();
  // Docker 对未设置的时间返回零值 0001-01-01T00:00:00Z，按空值展示。
  if (!trimmed || trimmed.startsWith('0001-01-01')) return '—';
  // Docker 的时间串自带时区；只截取日期与分钟，避免本地时区换算改变原值。
  const matched = /^(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2})/.exec(trimmed);
  return matched ? `${matched[1]} ${matched[2]}` : trimmed;
}

export function shortText(value: string, max = 48): string {
  const trimmed = value.trim();
  return trimmed.length > max ? `${trimmed.slice(0, max - 1)}…` : trimmed;
}

export function portLabel(port: DockerPort): string {
  const host = port.hostIp && port.hostIp !== '0.0.0.0' ? `${port.hostIp}:` : '';
  const suffix = port.protocol ? `/${port.protocol}` : '';
  return `${host}${port.hostPort || '—'} → ${port.containerPort}${suffix}`;
}
