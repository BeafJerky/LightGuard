export type ContainerActionKind =
  | 'container.start'
  | 'container.stop'
  | 'container.restart'
  | 'container.remove';

export type ContainerMenuKind = ContainerActionKind | 'container.terminal';

export interface ActionOption {
  kind: ContainerMenuKind;
  label: string;
  icon: string;
  danger: boolean;
}

const ACTIVE_STATES = new Set(['running', 'paused', 'restarting']);

export function isActiveState(state: string): boolean {
  return ACTIVE_STATES.has(state);
}

/**
 * 行内菜单按状态展示：运行中可停止、重启、打开终端、删除（删除需先停止，后端兜底拒绝），
 * 已停止可启动或删除；正在删除的容器不给任何入口。
 */
export function containerActions(state: string): ActionOption[] {
  if (state === 'removing') return [];
  if (isActiveState(state)) {
    return [
      { kind: 'container.stop', label: '停止', icon: 'power', danger: false },
      { kind: 'container.restart', label: '重启', icon: 'refresh', danger: false },
      { kind: 'container.terminal', label: '终端', icon: 'terminal', danger: false },
      { kind: 'container.remove', label: '删除', icon: 'trash', danger: true },
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
