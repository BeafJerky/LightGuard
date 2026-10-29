import { describe, expect, it } from 'vitest';
import { useDesktopWindows } from './useDesktopWindows';

function setup(width = 1440, height = 900, inset = 0, dockInset = 54) {
  const bounds = { width, height };
  const topInset = { value: inset };
  const bottomInset = { value: dockInset };
  const api = useDesktopWindows(
    () => bounds,
    () => topInset.value,
    () => bottomInset.value,
  );
  return { ...api, bounds, topInset, bottomInset };
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

  it('opens new windows inside the floating dock area', () => {
    const height = 420;
    const floating = setup(1440, height, 0, 6);
    floating.open('about');
    const floatWin = floating.windows.value[0];

    const full = setup(1440, height, 0, 54);
    full.open('about');
    const fullWin = full.windows.value[0];

    expect(floatWin.y + floatWin.h).toBeLessThanOrEqual(height - 12);
    expect(floatWin.y).toBeGreaterThan(fullWin.y);
  });

  it('re-fits windows when the dock switches between floating and full', () => {
    const { open, windows, fitToBounds, bottomInset } = setup(1440, 900);
    open('performance');
    const win = windows.value[0];
    win.y = 5000;
    win.h = 400;

    fitToBounds();
    expect(win.y).toBe(900 - 54 - 400);

    bottomInset.value = 6;
    fitToBounds();
    win.y = 5000;
    fitToBounds();
    expect(win.y).toBe(900 - 6 - 400);

    bottomInset.value = 54;
    fitToBounds();
    expect(win.y).toBe(900 - 54 - 400);
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
