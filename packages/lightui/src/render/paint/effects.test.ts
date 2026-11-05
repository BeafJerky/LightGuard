import { describe, expect, it } from 'vitest';
import { themes } from '../../theme/tokens';
import type { CanvasFrame } from '../types';
import { createDesktopEffectsDraw } from './effects';

function recordingContext() {
  const calls = { arc: 0, fillRect: 0 };
  const ctx = {
    save() {},
    restore() {},
    beginPath() {},
    arc() {
      calls.arc += 1;
    },
    fill() {},
    fillRect() {
      calls.fillRect += 1;
    },
    stroke() {},
    moveTo() {},
    lineTo() {},
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 1,
  };
  return { ctx, calls };
}

function frame(theme: CanvasFrame['theme']): CanvasFrame {
  return { width: 800, height: 600, dpr: 1, time: 1200, theme };
}

describe('desktop effects', () => {
  it('keeps WIN11 wallpapers static without drifting particles', () => {
    for (const id of ['win11', 'win11-dark'] as const) {
      const { ctx, calls } = recordingContext();
      createDesktopEffectsDraw('medium')(ctx as unknown as CanvasRenderingContext2D, frame(themes[id]));
      expect(calls.arc, id).toBe(0);
      expect(calls.fillRect, id).toBe(0);
    }
  });

  it('still animates particles for the default theme', () => {
    const { ctx, calls } = recordingContext();
    createDesktopEffectsDraw('medium')(ctx as unknown as CanvasRenderingContext2D, frame(themes.dark));
    expect(calls.arc).toBeGreaterThan(0);
  });
});
