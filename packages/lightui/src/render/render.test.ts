import { describe, expect, it } from 'vitest';
import { DEFAULT_CAPABILITIES } from './capabilities';
import { RENDER_TIER_META, resolveRenderBackend } from './resolve';
import { DEFAULT_RENDER_SETTINGS, normalizeRenderSettings } from './settings';
import { RENDER_TIERS } from './types';
import type { RenderBackend, RenderCapabilities, RenderPreference, RenderSettings, RenderTier } from './types';

const caps = (overrides: Partial<RenderCapabilities> = {}): RenderCapabilities => ({
  ...DEFAULT_CAPABILITIES,
  canvas2d: true,
  webgl2: true,
  ...overrides,
});

type SettingsOverride = Partial<Omit<RenderSettings, 'effects'>> & {
  effects?: Partial<RenderSettings['effects']>;
};

function settings(overrides: SettingsOverride = {}): RenderSettings {
  return {
    ...DEFAULT_RENDER_SETTINGS,
    ...overrides,
    effects: { ...DEFAULT_RENDER_SETTINGS.effects, ...overrides.effects },
    tiers: { ...DEFAULT_RENDER_SETTINGS.tiers, ...overrides.tiers },
  };
}

function resolve(
  tier: RenderTier,
  preference: RenderPreference = 'inherit',
  options: {
    settings?: SettingsOverride;
    capabilities?: Partial<RenderCapabilities>;
    count?: number;
  } = {},
): RenderBackend {
  return resolveRenderBackend({
    tier,
    preference,
    settings: settings(options.settings),
    capabilities: caps(options.capabilities),
    hint: { count: options.count },
  });
}

describe('render tier metadata', () => {
  it('declares every tier with a non-empty backend list', () => {
    for (const tier of RENDER_TIERS) {
      expect(RENDER_TIER_META[tier].id).toBe(tier);
      expect(RENDER_TIER_META[tier].backends.length).toBeGreaterThan(0);
    }
  });

  it('keeps text/IME layers on DOM by declaration', () => {
    expect(RENDER_TIER_META['console.charts'].backends).toEqual(['dom', 'canvas']);
    expect(RENDER_TIER_META['console.tables'].backends).toEqual(['dom', 'canvas']);
    expect(RENDER_TIER_META.terminal.backends).toEqual(['dom', 'webgl']);
  });
});

describe('auto resolution', () => {
  it('defaults every visual tier to Canvas or WebGL when available', () => {
    expect(resolve('app.background')).toBe('webgl');
    expect(resolve('app.background', 'inherit', { capabilities: { webgpu: true } })).toBe('webgpu');
    expect(resolve('desktop.effects')).toBe('dom');
    expect(resolve('desktop.icons')).toBe('canvas');
    expect(resolve('console.charts')).toBe('canvas');
    expect(resolve('console.tables')).toBe('canvas');
  });

  it('falls back from WebGPU to WebGL and Canvas', () => {
    expect(resolve('app.background', 'webgpu')).toBe('webgl');
    expect(resolve('app.background', 'webgpu', { capabilities: { webgl2: false } })).toBe('canvas');
    expect(
      resolve('app.background', 'webgpu', { capabilities: { webgl2: false, canvas2d: false } }),
    ).toBe('dom');
  });

  it('prefers WebGL for the terminal when available', () => {
    expect(resolve('terminal')).toBe('webgl');
    expect(resolve('terminal', 'inherit', { capabilities: { webgl2: false } })).toBe('dom');
  });
});

describe('explicit preferences', () => {
  it('honours forced canvas modes', () => {
    expect(resolve('app.background', 'canvas')).toBe('canvas');
    expect(resolve('desktop.icons', 'canvas')).toBe('canvas');
  });

  it('falls back from WebGL to Canvas for decorative layers', () => {
    expect(resolve('app.background', 'webgl', { capabilities: { webgl2: false } })).toBe('canvas');
    expect(resolve('app.background', 'webgl', { capabilities: { webgl2: false, canvas2d: false } })).toBe('dom');
  });

  it('never puts the terminal on canvas', () => {
    expect(resolve('terminal', 'canvas')).toBe('dom');
    expect(resolve('terminal', 'webgl', { capabilities: { webgl2: false } })).toBe('dom');
  });

  it('applies the global preference through inherit', () => {
    expect(resolve('desktop.icons', 'inherit', { settings: { global: 'canvas' } })).toBe('canvas');
    expect(resolve('terminal', 'inherit', { settings: { global: 'canvas' } })).toBe('dom');
  });

  it('lets a tier override the global preference', () => {
    expect(
      resolve('console.charts', 'inherit', {
        settings: { global: 'canvas', tiers: { 'console.charts': 'dom' } },
      }),
    ).toBe('dom');
  });
});

describe('effects and accessibility guards', () => {
  it('keeps effects off until explicitly enabled', () => {
    expect(resolve('desktop.effects')).toBe('dom');
    expect(
      resolve('desktop.effects', 'inherit', { settings: { effects: { enabled: true } } }),
    ).toBe('canvas');
  });

  it('disables effects under reduced motion', () => {
    expect(
      resolve('desktop.effects', 'inherit', {
        settings: { effects: { enabled: true } },
        capabilities: { reducedMotion: true },
      }),
    ).toBe('dom');
  });

  it('forces every tier to DOM in accessibility mode or forced colors', () => {
    for (const tier of RENDER_TIERS) {
      expect(resolve(tier, 'canvas', { settings: { a11yForceDom: true } })).toBe('dom');
      expect(resolve(tier, 'webgl', { capabilities: { forcedColors: true } })).toBe('dom');
    }
  });

  it('returns DOM when Canvas 2D is unavailable', () => {
    expect(resolve('console.charts', 'canvas', { capabilities: { canvas2d: false } })).toBe('dom');
  });
});

describe('settings normalization', () => {
  it('returns defaults for invalid input', () => {
    expect(normalizeRenderSettings(null)).toEqual(DEFAULT_RENDER_SETTINGS);
    expect(normalizeRenderSettings('nope')).toEqual(DEFAULT_RENDER_SETTINGS);
    expect(normalizeRenderSettings({ global: 'inherit' })).toEqual(DEFAULT_RENDER_SETTINGS);
  });

  it('keeps valid preferences and drops unknown tiers', () => {
    const normalized = normalizeRenderSettings({
      global: 'canvas',
      tiers: { 'console.charts': 'dom', 'desktop.icons': 'canvas', unknown: 'webgl', terminal: 'nope' },
      effects: { enabled: true, intensity: 'high', pauseWhenHidden: false },
      a11yForceDom: true,
    });
    expect(normalized.global).toBe('canvas');
    expect(normalized.tiers).toEqual({ 'console.charts': 'dom', 'desktop.icons': 'canvas' });
    expect(normalized.effects).toEqual({ enabled: true, intensity: 'high', pauseWhenHidden: false });
    expect(normalized.a11yForceDom).toBe(true);
  });

  it('defaults pauseWhenHidden to true and intensity to low', () => {
    const normalized = normalizeRenderSettings({ effects: { enabled: 'yes', intensity: 'huge' } });
    expect(normalized.effects).toEqual({ enabled: false, intensity: 'low', pauseWhenHidden: true });
  });
});
