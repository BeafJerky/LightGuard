import { describe, expect, it } from 'vitest';
import { validatePayload } from './backup';

function readSettings(settings: Record<string, unknown>) {
  return validatePayload({
    appVersion: '0.12.0',
    scope: { settings: true, connections: false },
    settings,
    themes: { console: 'dark', desktop: 'pixel' },
    connections: [],
  }).payload.settings;
}

describe('backup dock mode', () => {
  it('falls back to a full-width dock when an older backup has no dock mode', () => {
    expect(readSettings({}).desktopDock).toBe('full');
  });

  it('keeps a valid floating dock mode', () => {
    expect(readSettings({ desktopDock: 'floating' }).desktopDock).toBe('floating');
  });

  it('falls back to a full-width dock for an unknown mode', () => {
    expect(readSettings({ desktopDock: 'compact' }).desktopDock).toBe('full');
  });
});
