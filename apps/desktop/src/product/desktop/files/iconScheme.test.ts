import { beforeEach, describe, expect, it } from 'vitest';
import {
  loadIconScheme,
  resolveIconScheme,
  saveIconScheme,
  type IconScheme,
} from './iconScheme';

describe('icon scheme', () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it('defaults to auto and persists an explicit choice', () => {
    expect(loadIconScheme()).toBe('auto');

    saveIconScheme('color');

    expect(loadIconScheme()).toBe('color');
    saveIconScheme('line');
    expect(loadIconScheme()).toBe('line');
  });

  it('falls back to auto for unknown stored values', () => {
    window.localStorage.setItem('lg.files.icons', 'neon');
    expect(loadIconScheme()).toBe('auto');
  });

  it('recommends colorful icons for the base and WIN11 themes', () => {
    for (const theme of ['light', 'dark', 'win11', 'win11-dark']) {
      expect(resolveIconScheme('auto', theme)).toBe('color');
    }
  });

  it('recommends line icons for stylized themes', () => {
    for (const theme of ['claude', 'pixel', 'nes', 'tech', 'unknown']) {
      expect(resolveIconScheme('auto', theme)).toBe('line');
    }
  });

  it('lets a manual choice override the theme recommendation', () => {
    const cases: [IconScheme, string, string][] = [
      ['line', 'win11', 'line'],
      ['line', 'win11-dark', 'line'],
      ['color', 'pixel', 'color'],
      ['color', 'claude', 'color'],
    ];
    for (const [setting, theme, expected] of cases) {
      expect(resolveIconScheme(setting, theme)).toBe(expected);
    }
  });
});
