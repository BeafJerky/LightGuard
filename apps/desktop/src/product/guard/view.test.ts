import { describe, expect, it } from 'vitest';
import type { PermitStatus } from '../ssh';
import { permitAria, permitVisual } from './view';

function status(patch: Partial<PermitStatus> = {}): PermitStatus {
  return {
    serverId: 'srv_1',
    locked: true,
    minutes: 15,
    expiresAtMs: 0,
    writeActive: false,
    ...patch,
  };
}

describe('permit visual', () => {
  it('renders a neutral locked state', () => {
    expect(permitVisual(status(), 0)).toEqual({
      state: 'locked',
      icon: 'lock',
      label: '已锁定',
      remaining: '',
      tone: 'neutral',
    });
  });

  it('counts down locally while unlocked', () => {
    const visual = permitVisual(status({ locked: false, expiresAtMs: 15 * 60_000 }), 0);
    expect(visual.state).toBe('unlocked');
    expect(visual.label).toBe('15:00');
    expect(visual.tone).toBe('warning');
  });

  it('turns danger inside the last minute without dropping to negative', () => {
    expect(permitVisual(status({ locked: false, expiresAtMs: 59_999 }), 0)).toMatchObject({
      state: 'expiring',
      label: '01:00',
      tone: 'danger',
    });
    expect(permitVisual(status({ locked: false, expiresAtMs: 1 }), 5_000)).toMatchObject({
      state: 'expiring',
      remaining: '00:00',
    });
  });

  it('describes the trigger for screen readers', () => {
    expect(permitAria(status(), 0)).toBe('写操作已锁定，点击解锁');
    expect(permitAria(status({ locked: false, expiresAtMs: 90_000 }), 0)).toBe(
      '写操作已解锁，剩余 01:30，点击管理',
    );
    expect(permitAria(status({ locked: false, expiresAtMs: 30_000 }), 0)).toBe(
      '写操作许可即将到期，剩余 00:30，点击管理',
    );
  });
});
