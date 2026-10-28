import { describe, expect, it } from 'vitest';
import { useDesktopWindows } from './useDesktopWindows';

function setup(width = 1440, height = 900, inset = 0) {
  const bounds = { width, height };
  const topInset = { value: inset };
  const api = useDesktopWindows(
    () => bounds,
    () => topInset.value,
  );
  return { ...api, bounds, topInset };
}

describe('desktop window bounds fitting', () => {
  it('pulls windows back inside after the desktop shrinks', () => {
    const { open, windows, bounds, fitToBounds } = setup(1440, 900);
    open('about');
    const win = windows.value[0];
    win.x = 1000;
    win.y = 600;

    bounds.width = 900;
    bounds.height = 700;
    fitToBounds();

    expect(win.x).toBeLessThanOrEqual(900 - win.w);
    expect(win.y).toBeLessThanOrEqual(700 - 54 - win.h);
    expect(win.x).toBeGreaterThanOrEqual(0);
    expect(win.y).toBeGreaterThanOrEqual(0);
  });

  it('shrinks oversized windows so the title bar stays reachable', () => {
    const { open, windows, bounds, fitToBounds } = setup(1600, 1000);
    open('performance');
    const win = windows.value[0];
    win.w = 1400;
    win.h = 900;

    bounds.width = 800;
    bounds.height = 600;
    fitToBounds();

    expect(win.w).toBeLessThanOrEqual(800);
    expect(win.h).toBeLessThanOrEqual(600 - 54);
    expect(win.x + win.w).toBeLessThanOrEqual(800);
    expect(win.y).toBeLessThanOrEqual(600 - 54 - win.h);
  });

  it('respects the top inset when the bar becomes full width', () => {
    const { open, windows, topInset, fitToBounds } = setup(1440, 900);
    open('about');
    const win = windows.value[0];
    win.y = 0;

    topInset.value = 46;
    fitToBounds();

    expect(win.y).toBeGreaterThanOrEqual(46);
  });

  it('clamps the restore rect of maximized windows', () => {
    const { open, toggleMaximize, windows, bounds, fitToBounds } = setup(1600, 1000);
    open('about');
    const win = windows.value[0];
    win.x = 1200;
    win.y = 800;
    toggleMaximize('about');
    expect(win.maximized).toBe(true);

    bounds.width = 900;
    bounds.height = 700;
    fitToBounds();

    expect(win.restore).not.toBeNull();
    expect(win.restore!.x).toBeLessThanOrEqual(900 - win.restore!.w);
    expect(win.restore!.y).toBeLessThanOrEqual(700 - 54 - win.restore!.h);
  });

  it('ignores transient degenerate measurements', () => {
    const { open, windows, bounds, fitToBounds } = setup(1440, 900);
    open('about');
    const win = windows.value[0];
    const before = { x: win.x, y: win.y, w: win.w, h: win.h };

    bounds.width = 0;
    bounds.height = 0;
    fitToBounds();

    expect({ x: win.x, y: win.y, w: win.w, h: win.h }).toEqual(before);
  });
});
