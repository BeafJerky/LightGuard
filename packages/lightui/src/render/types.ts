import type { ThemeTokens } from '../theme/types';

export type RenderBackend = 'dom' | 'canvas' | 'webgl';
export type RenderPreference = 'inherit' | 'auto' | RenderBackend;

export const RENDER_TIERS = [
  'app.background',
  'desktop.effects',
  'desktop.icons',
  'console.charts',
  'console.tables',
  'terminal',
] as const;

export type RenderTier = (typeof RENDER_TIERS)[number];

export type EffectIntensity = 'low' | 'medium' | 'high';

export interface RenderCapabilities {
  canvas2d: boolean;
  webgl2: boolean;
  webgpu: boolean;
  forcedColors: boolean;
  reducedMotion: boolean;
}

export interface RenderEffectsSettings {
  enabled: boolean;
  intensity: EffectIntensity;
  pauseWhenHidden: boolean;
}

export interface RenderSettings {
  global: Exclude<RenderPreference, 'inherit'>;
  tiers: Partial<Record<RenderTier, RenderPreference>>;
  effects: RenderEffectsSettings;
  a11yForceDom: boolean;
}

export interface RenderHint {
  count?: number;
  editable?: boolean;
}

export interface RenderTierMeta {
  id: RenderTier;
  label: string;
  description: string;
  backends: readonly RenderBackend[];
}

export interface RenderScopeContext {
  settings: Readonly<RenderSettings>;
  capabilities: Readonly<RenderCapabilities>;
  resolve: (tier: RenderTier, hint?: RenderHint) => RenderBackend;
}

export interface CanvasFrame {
  width: number;
  height: number;
  dpr: number;
  time: number;
  theme: Readonly<ThemeTokens>;
}
