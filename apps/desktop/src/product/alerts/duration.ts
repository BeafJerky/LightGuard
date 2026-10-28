export type DurationOption = '30m' | '1h' | '4h' | 'today';

export const DURATION_OPTIONS: readonly { id: DurationOption; label: string }[] = [
  { id: '30m', label: '30 分钟' },
  { id: '1h', label: '1 小时' },
  { id: '4h', label: '4 小时' },
  { id: 'today', label: '今天' },
];

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

const FIXED_OFFSETS: Record<Exclude<DurationOption, 'today'>, number> = {
  '30m': 30 * MINUTE_MS,
  '1h': 60 * MINUTE_MS,
  '4h': 4 * 60 * MINUTE_MS,
};

/**
 * 时刻由调用方传入：本模块保持纯函数，测试与页面都能拿到确定结果。
 * `today` 取本地时间的下一个零点，跨月/跨年与夏令时交给 Date 归一化。
 */
export function durationToUntil(option: DurationOption, now: Date): number {
  if (option === 'today') {
    return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1).getTime();
  }
  return now.getTime() + FIXED_OFFSETS[option];
}

/**
 * 相对时间文案。时钟回拨或后端时间戳在未来时差值为负，一律钳到 0，
 * 不到一分钟也只显示“刚刚”，不会出现“-1 分钟前”。
 */
export function relativeText(atMs: number, nowMs: number): string {
  const elapsed = Number.isFinite(atMs) && Number.isFinite(nowMs) ? Math.max(0, nowMs - atMs) : 0;
  if (elapsed < MINUTE_MS) return '刚刚';
  if (elapsed < HOUR_MS) return `${Math.floor(elapsed / MINUTE_MS)} 分钟前`;
  if (elapsed < DAY_MS) return `${Math.floor(elapsed / HOUR_MS)} 小时前`;
  return `${Math.floor(elapsed / DAY_MS)} 天前`;
}
