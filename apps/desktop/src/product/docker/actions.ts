export type ContainerActionKind =
  | 'container.start'
  | 'container.stop'
  | 'container.restart'
  | 'container.remove';

export interface ActionOption {
  kind: ContainerActionKind;
  label: string;
  icon: string;
  danger: boolean;
}

const ACTIVE_STATES = new Set(['running', 'paused', 'restarting']);

function isActiveState(state: string): boolean {
  return ACTIVE_STATES.has(state);
}

/**
 * 行内菜单只暴露后端允许的操作：
 * 运行中的容器能重启/停止，停止中的容器能启动/删除；
 * 正在删除的容器不给任何入口，避免预览必然失败。
 */
export function containerActions(state: string): ActionOption[] {
  if (state === 'removing') return [];
  if (isActiveState(state)) {
    return [
      { kind: 'container.restart', label: '重启', icon: 'refresh', danger: false },
      { kind: 'container.stop', label: '停止', icon: 'power', danger: false },
    ];
  }
  return [
    { kind: 'container.start', label: '启动', icon: 'play', danger: false },
    { kind: 'container.remove', label: '删除', icon: 'trash', danger: true },
  ];
}

/** 预览倒计时按整秒向上取整；已过期或时间戳非法时为 0。 */
export function remainingSeconds(expiresAtMs: number, nowMs: number): number {
  if (!Number.isFinite(expiresAtMs)) return 0;
  return Math.max(0, Math.ceil((expiresAtMs - nowMs) / 1000));
}
