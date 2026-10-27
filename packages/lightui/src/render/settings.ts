import { RENDER_TIERS } from './types';
import type {
  EffectIntensity,
  RenderBackend,
  RenderPreference,
  RenderSettings,
  RenderTier,
} from './types';

export const DEFAULT_RENDER_SETTINGS: RenderSettings = {
  global: 'auto',
  tiers: {},
  effects: {
    enabled: false,
    intensity: 'low',
    pauseWhenHidden: true,
  },
  a11yForceDom: false,
};

const BACKENDS: readonly RenderBackend[] = ['dom', 'canvas', 'webgl'];
const INTENSITIES: readonly EffectIntensity[] = ['low', 'medium', 'high'];

function isBackend(value: unknown): value is RenderBackend {
  return typeof value === 'string' && (BACKENDS as readonly string[]).includes(value);
}

function isPreference(value: unknown): value is RenderPreference {
  return value === 'inherit' || value === 'auto' || isBackend(value);
}

function isIntensity(value: unknown): value is EffectIntensity {
  return typeof value === 'string' && (INTENSITIES as readonly string[]).includes(value);
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === 'object' ? (value as Record<string, unknown>) : {};
}

export function normalizeRenderSettings(value: unknown): RenderSettings {
  const raw = record(value);
  const rawTiers = record(raw.tiers);
  const tiers: Partial<Record<RenderTier, RenderPreference>> = {};

  for (const tier of RENDER_TIERS) {
    if (isPreference(rawTiers[tier])) tiers[tier] = rawTiers[tier];
  }

  const rawEffects = record(raw.effects);

  return {
    global: raw.global === 'auto' || isBackend(raw.global) ? raw.global : 'auto',
    tiers,
    effects: {
      enabled: rawEffects.enabled === true,
      intensity: isIntensity(rawEffects.intensity) ? rawEffects.intensity : 'low',
      pauseWhenHidden: rawEffects.pauseWhenHidden !== false,
    },
    a11yForceDom: raw.a11yForceDom === true,
  };
}

export function isRenderSettingsEqual(a: RenderSettings, b: RenderSettings): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
