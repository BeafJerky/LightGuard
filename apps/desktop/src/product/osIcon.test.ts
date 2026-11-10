import { describe, expect, it, vi } from 'vitest';
import { resolveIconModes, syncAppIcons } from './osIcon';

describe('resolveIconModes', () => {
  it('maps dark console themes to the dark window icon', () => {
    expect(resolveIconModes('dark', false)).toEqual({ window: 'dark', tray: 'light' });
    expect(resolveIconModes('pixel', false)).toEqual({ window: 'dark', tray: 'light' });
    expect(resolveIconModes('tech', false)).toEqual({ window: 'dark', tray: 'light' });
    expect(resolveIconModes('win11-dark', false)).toEqual({ window: 'dark', tray: 'light' });
  });

  it('maps light console themes to the light window icon', () => {
    expect(resolveIconModes('light', true)).toEqual({ window: 'light', tray: 'dark' });
    expect(resolveIconModes('claude', true)).toEqual({ window: 'light', tray: 'dark' });
    expect(resolveIconModes('nes', true)).toEqual({ window: 'light', tray: 'dark' });
    expect(resolveIconModes('win11', true)).toEqual({ window: 'light', tray: 'dark' });
  });

  it('resolves the system theme against the OS preference', () => {
    expect(resolveIconModes('system', true)).toEqual({ window: 'dark', tray: 'dark' });
    expect(resolveIconModes('system', false)).toEqual({ window: 'light', tray: 'light' });
  });

  it('keeps the tray on the OS preference when the console theme is fixed', () => {
    expect(resolveIconModes('nes', true)).toEqual({ window: 'light', tray: 'dark' });
    expect(resolveIconModes('pixel', false)).toEqual({ window: 'dark', tray: 'light' });
  });
});

describe('syncAppIcons', () => {
  it('pushes both modes through the bridge', async () => {
    const setIcons = vi.fn().mockResolvedValue(undefined);
    await syncAppIcons('dark', false, { setIcons });
    expect(setIcons).toHaveBeenCalledWith({ window: 'dark', tray: 'light' });
  });

  it('swallows bridge failures so theme switching still works', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    const setIcons = vi.fn().mockRejectedValue(new Error('boom'));
    await expect(syncAppIcons('light', true, { setIcons })).resolves.toBeUndefined();
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });
});
