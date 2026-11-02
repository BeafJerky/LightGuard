import { describe, expect, it } from 'vitest';
import { buildBackupPayload, validatePayload } from './backup';
import { DEFAULT_SETTINGS } from './store';

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

describe('backup autostart exclusion', () => {
  it('does not export the local autostart preference', () => {
    const payload = buildBackupPayload({
      settings: { ...DEFAULT_SETTINGS, autostart: true },
      themes: { console: 'dark', desktop: 'pixel' },
      servers: [],
      includeSettings: true,
      includeConnections: false,
    });
    expect('autostart' in payload.settings).toBe(false);
    expect(JSON.stringify(payload)).not.toContain('autostart');
  });

  it('does not invent autostart when an old backup omits it', () => {
    expect('autostart' in readSettings({})).toBe(false);
  });
});
