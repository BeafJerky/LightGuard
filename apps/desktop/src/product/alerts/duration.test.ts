import { describe, expect, it } from 'vitest';
import { DURATION_OPTIONS, durationToUntil, relativeText } from './duration';

describe('alert duration', () => {
  const now = new Date(2026, 9, 28, 10, 0, 0);

  it('adds fixed offsets', () => {
    expect(durationToUntil('30m', now)).toBe(now.getTime() + 30 * 60_000);
    expect(durationToUntil('1h', now)).toBe(now.getTime() + 60 * 60_000);
    expect(durationToUntil('4h', now)).toBe(now.getTime() + 4 * 60 * 60_000);
  });

  it('ends today at local midnight', () => {
    const late = new Date(2026, 9, 28, 23, 50, 0);
    expect(durationToUntil('today', late)).toBe(new Date(2026, 9, 29, 0, 0, 0).getTime());
    expect(durationToUntil('today', now)).toBe(new Date(2026, 9, 29, 0, 0, 0).getTime());
  });

  it('describes relative time without negatives', () => {
    expect(relativeText(now.getTime(), now.getTime())).toBe('刚刚');
    expect(relativeText(now.getTime() - 6 * 60_000, now.getTime())).toBe('6 分钟前');
    expect(relativeText(now.getTime() - 3 * 60 * 60_000, now.getTime())).toBe('3 小时前');
    expect(relativeText(now.getTime() - 2 * 24 * 60 * 60_000, now.getTime())).toBe('2 天前');
    expect(relativeText(now.getTime() + 60_000, now.getTime())).toBe('刚刚');
  });

  it('keeps sub-minute differences on the floor and every option labelled', () => {
    expect(relativeText(now.getTime() - 59_000, now.getTime())).toBe('刚刚');
    expect(relativeText(now.getTime() - 90 * 60_000, now.getTime())).toBe('1 小时前');
    expect(relativeText(now.getTime() - 60 * 60_000, now.getTime())).toBe('1 小时前');
    expect(DURATION_OPTIONS.map((option) => option.id)).toEqual(['30m', '1h', '4h', 'today']);
  });
});
