import type { RenderCapabilities } from './types';

export const DEFAULT_CAPABILITIES: RenderCapabilities = {
  canvas2d: false,
  webgl2: false,
  webgpu: false,
  forcedColors: false,
  reducedMotion: false,
};

function media(query: string): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
  return window.matchMedia(query).matches;
}

function createCanvas(): HTMLCanvasElement | null {
  if (typeof document === 'undefined') return null;
  try {
    return document.createElement('canvas');
  } catch {
    return null;
  }
}

function supportsCanvas2d(): boolean {
  const canvas = createCanvas();
  if (!canvas) return false;
  try {
    return Boolean(canvas.getContext('2d'));
  } catch {
    return false;
  }
}

function supportsWebgl2(): boolean {
  const canvas = createCanvas();
  if (!canvas) return false;
  try {
    const gl = canvas.getContext('webgl2');
    if (!gl) return false;
    // 探测后主动释放，避免占用浏览器有限的 WebGL 上下文名额。
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    return true;
  } catch {
    return false;
  }
}

function supportsWebgpu(): boolean {
  return typeof navigator !== 'undefined' && 'gpu' in navigator;
}

export function detectCapabilities(): RenderCapabilities {
  return {
    canvas2d: supportsCanvas2d(),
    webgl2: supportsWebgl2(),
    webgpu: supportsWebgpu(),
    forcedColors: media('(forced-colors: active)'),
    reducedMotion: media('(prefers-reduced-motion: reduce)'),
  };
}

export function withCapabilities(
  base: RenderCapabilities,
  overrides?: Partial<RenderCapabilities>,
): RenderCapabilities {
  return overrides ? { ...base, ...overrides } : base;
}
