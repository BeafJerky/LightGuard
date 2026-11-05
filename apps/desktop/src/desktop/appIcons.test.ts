import { describe, expect, it } from 'vitest';
import { themes } from '@lightguard/lightui';
import { DESKTOP_APPS } from '../product/desktop/apps';
import {
  APP_GLYPHS,
  APP_ICON_SIZES,
  appIconScheme,
  appTileColor,
  contrastRatio,
  schemeBTile,
} from './appIcons';

const expectedScheme: Record<string, 'a' | 'b'> = {
  dark: 'b',
  light: 'b',
  win11: 'b',
  'win11-dark': 'b',
  claude: 'a',
  pixel: 'a',
  nes: 'a',
  tech: 'a',
};

describe('app icon glyphs', () => {
  it('provides filled glyph parts for every desktop app', () => {
    expect(Object.keys(APP_GLYPHS).sort()).toEqual(DESKTOP_APPS.map((app) => app.key).sort());
    for (const app of DESKTOP_APPS) {
      const parts = APP_GLYPHS[app.key];
      expect(parts.length, app.key).toBeGreaterThan(0);
      for (const part of parts) {
        expect(/^[Mm]/.test(part.d), app.key).toBe(true);
        if (part.mode === 'stroke') expect(part.strokeWidth).toBeGreaterThan(0);
      }
    }
  });
});

describe('app icon scheme', () => {
  it('maps every shipped theme to the approved scheme', () => {
    for (const [id, scheme] of Object.entries(expectedScheme)) {
      expect(appIconScheme(id as keyof typeof themes), id).toBe(scheme);
    }
    expect(Object.keys(expectedScheme).sort()).toEqual(Object.keys(themes).sort());
  });
});

describe('scheme B palette', () => {
  const vivid = Object.values(themes).filter((theme) => appIconScheme(theme.id) === 'b');

  it('keeps glyph ink at or above 3:1 against both gradient ends', () => {
    expect(vivid.length).toBe(4);
    for (const theme of vivid) {
      for (const app of DESKTOP_APPS) {
        const color = appTileColor(theme, app.tile);
        const tile = schemeBTile(color, theme);
        const start = contrastRatio(tile.start, tile.ink);
        const end = contrastRatio(tile.end, tile.ink);
        expect(start, `${theme.id}/${app.key}/start`).toBeGreaterThanOrEqual(3);
        expect(end, `${theme.id}/${app.key}/end`).toBeGreaterThanOrEqual(3);
      }
    }
  });

  it('derives a saturated tile and a visible border from the theme color', () => {
    const theme = themes.dark;
    const tile = schemeBTile(theme.color.primary, theme);
    expect(tile.start).not.toBe(theme.color.primary);
    expect(tile.end).not.toBe(theme.color.primary);
    expect(tile.border).not.toBe(theme.color.border);
    expect(['#06101c', '#f4f8fd']).toContain(tile.ink);
  });
});

describe('app icon size ladder', () => {
  it('locks the approved glyph-to-tile sizes', () => {
    expect(APP_ICON_SIZES.desktop).toEqual({ tile: 46, glyph: 32 });
    expect(APP_ICON_SIZES.dock).toEqual({ tile: 34, glyph: 24 });
    expect(APP_ICON_SIZES.taskbar).toEqual({ tile: 30, glyph: 21 });
    expect(APP_ICON_SIZES.start).toEqual({ tile: 30, glyph: 20 });
    expect(APP_ICON_SIZES.narrowTaskbar).toEqual({ tile: 24, glyph: 17 });
  });

  it('keeps the glyph optical ratio near 70% everywhere', () => {
    for (const size of Object.values(APP_ICON_SIZES)) {
      const ratio = size.glyph / size.tile;
      expect(ratio).toBeGreaterThanOrEqual(0.66);
      expect(ratio).toBeLessThanOrEqual(0.72);
    }
  });
});
