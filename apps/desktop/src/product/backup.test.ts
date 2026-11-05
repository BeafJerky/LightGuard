import { describe, expect, it } from 'vitest';
import { buildBackupPayload, validatePayload } from './backup';
import { DEFAULT_SETTINGS } from './store';

function validate(themes: Record<string, unknown>) {
  return validatePayload({
    appVersion: '0.15.3',
    scope: { settings: false, connections: false },
    settings: {},
    themes,
    connections: [],
  });
}

describe('backup themes', () => {
  it('keeps the WIN11 pair', () => {
    const result = validate({ console: 'win11', desktop: 'win11-dark' });
    expect(result.payload.themes).toEqual({ console: 'win11', desktop: 'win11-dark' });
    expect(result.warnings).toEqual([]);
  });
});

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

describe('backup chrome auto hide', () => {
  it('falls back to visible chrome when an older backup has no flags', () => {
    const settings = readSettings({});
    expect(settings.desktopBarAutoHide).toBe(false);
    expect(settings.desktopDockAutoHide).toBe(false);
  });

  it('keeps valid flags and rejects non-boolean values', () => {
    const valid = readSettings({ desktopBarAutoHide: true, desktopDockAutoHide: true });
    expect(valid.desktopBarAutoHide).toBe(true);
    expect(valid.desktopDockAutoHide).toBe(true);

    const invalid = readSettings({ desktopBarAutoHide: 'yes', desktopDockAutoHide: 1 });
    expect(invalid.desktopBarAutoHide).toBe(false);
    expect(invalid.desktopDockAutoHide).toBe(false);
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
