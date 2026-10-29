import { describe, expect, it } from 'vitest';
import { dockerTime, percent, shortText, stateLabel, stateTone } from './format';

describe('docker format', () => {
  it('labels container states in Chinese', () => {
    expect(stateLabel('running')).toBe('运行中');
    expect(stateLabel('exited')).toBe('已停止');
    expect(stateLabel('paused')).toBe('已暂停');
    expect(stateLabel('weird')).toBe('weird');
  });

  it('maps states to tones', () => {
    expect(stateTone('running')).toBe('online');
    expect(stateTone('paused')).toBe('warning');
    expect(stateTone('dead')).toBe('danger');
    expect(stateTone('exited')).toBe('muted');
  });

  it('formats percentages and keeps unknown values visible', () => {
    expect(percent(0.42)).toBe('0.4%');
    expect(percent(12.6)).toBe('13%');
    expect(percent(null)).toBe('—');
    expect(percent(Number.NaN)).toBe('—');
  });

  it('trims docker timestamps and text', () => {
    expect(dockerTime('2026-10-29 10:20:31 +0800 CST')).toBe('2026-10-29 10:20');
    expect(dockerTime('not a date')).toBe('not a date');
    expect(dockerTime('')).toBe('—');
    expect(shortText('  nginx:1.27  ')).toBe('nginx:1.27');
    expect(shortText('a'.repeat(60), 10)).toBe('aaaaaaaaa…');
  });
});
