import { mixColor } from '@lightguard/lightui';
import type { ThemeColors, ThemeMode, ThemeName, ThemeTokens } from '@lightguard/lightui';

export type AppIconKey =
  | 'about'
  | 'performance'
  | 'disks'
  | 'network'
  | 'processes'
  | 'connection'
  | 'files'
  | 'terminal'
  | 'docker';

export type AppIconScheme = 'a' | 'b';

export interface AppGlyphPart {
  d: string;
  mode?: 'fill' | 'stroke';
  strokeWidth?: number;
}

export interface AppIconSize {
  tile: number;
  glyph: number;
}

export interface SchemeBTile {
  start: string;
  end: string;
  border: string;
  ink: string;
}

export const APP_ICON_SIZES = {
  desktop: { tile: 46, glyph: 32 },
  dock: { tile: 34, glyph: 24 },
  taskbar: { tile: 30, glyph: 21 },
  start: { tile: 30, glyph: 20 },
  narrowTaskbar: { tile: 24, glyph: 17 },
} as const satisfies Record<string, AppIconSize>;

const SCHEME_BY_THEME: Record<ThemeName, AppIconScheme> = {
  dark: 'b',
  light: 'b',
  win11: 'b',
  'win11-dark': 'b',
  claude: 'a',
  pixel: 'a',
  nes: 'a',
  tech: 'a',
};

const TILE_COLOR_TOKEN: Record<string, keyof ThemeColors> = {
  'tile-about': 'primary',
  'tile-performance': 'accent',
  'tile-disks': 'info',
  'tile-network': 'warning',
  'tile-processes': 'danger',
  'tile-connection': 'success',
  'tile-files': 'accent',
  'tile-terminal': 'textSecondary',
  'tile-docker': 'info',
};

const DARK_INK = '#06101c';
const LIGHT_INK = '#f4f8fd';

export const APP_GLYPHS: Record<AppIconKey, AppGlyphPart[]> = {
  about: [
    {
      d: 'M12 2.4 4 5.6v5.2c0 5 3.4 9 8 10.8 4.6-1.8 8-5.8 8-10.8V5.6l-8-3.2Zm0 4.4a1.4 1.4 0 1 1 0 2.8 1.4 1.4 0 0 1 0-2.8Zm-1.15 4.3h2.3v6.3h-2.3v-6.3Z',
    },
  ],
  performance: [
    {
      d: 'M12 3.2a10.8 10.8 0 0 1 10.8 10.8H19a7 7 0 0 0-14 0H1.2A10.8 10.8 0 0 1 12 3.2Z',
    },
    { d: 'M12.6 13.4 18.9 6.2l1.5 1.3-5.7 6.8-2.1-.9Z' },
    { d: 'M14.4 14a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0Z' },
  ],
  disks: [
    {
      d: 'M5.4 3.2h13.2a2.4 2.4 0 0 1 2.4 2.4v3.6a2.4 2.4 0 0 1-2.4 2.4H5.4A2.4 2.4 0 0 1 3 9.2V5.6a2.4 2.4 0 0 1 2.4-2.4Zm1 3.2h3.2a.8.8 0 0 1 0 1.6H6.4a.8.8 0 0 1 0-1.6Zm9.4 0h1a.8.8 0 0 1 0 1.6h-1a.8.8 0 0 1 0-1.6ZM5.4 12.4h13.2a2.4 2.4 0 0 1 2.4 2.4v3.6a2.4 2.4 0 0 1-2.4 2.4H5.4A2.4 2.4 0 0 1 3 18.4v-3.6a2.4 2.4 0 0 1 2.4-2.4Zm1 3.2h3.2a.8.8 0 0 1 0 1.6H6.4a.8.8 0 0 1 0-1.6Zm9.4 0h1a.8.8 0 0 1 0 1.6h-1a.8.8 0 0 1 0-1.6Z',
    },
  ],
  network: [
    { d: 'M12 8.2v3.4M12 11.6 6.6 15.5M12 11.6l5.4 3.9', mode: 'stroke', strokeWidth: 2.4 },
    { d: 'M15.3 5.2a3.3 3.3 0 1 1-6.6 0 3.3 3.3 0 0 1 6.6 0Z' },
    { d: 'M8.5 18.8a3.3 3.3 0 1 1-6.6 0 3.3 3.3 0 0 1 6.6 0Z' },
    { d: 'M22.1 18.8a3.3 3.3 0 1 1-6.6 0 3.3 3.3 0 0 1 6.6 0Z' },
  ],
  processes: [
    {
      d: 'M8.2 4.6h7.6a3.6 3.6 0 0 1 3.6 3.6v7.6a3.6 3.6 0 0 1-3.6 3.6H8.2a3.6 3.6 0 0 1-3.6-3.6V8.2a3.6 3.6 0 0 1 3.6-3.6Zm1.6 4.2h4.4a1 1 0 0 1 1 1v4.4a1 1 0 0 1-1 1H9.8a1 1 0 0 1-1-1v-4.4a1 1 0 0 1 1-1Z',
    },
    {
      d: 'M9.2 1.8h1.8v3.2H9.2zM13 1.8h1.8v3.2H13zM9.2 19h1.8v3.2H9.2zM13 19h1.8v3.2H13zM1.8 9.2h3.2v1.8H1.8zM1.8 13h3.2v1.8H1.8zM19 9.2h3.2v1.8H19zM19 13h3.2v1.8H19z',
    },
  ],
  connection: [
    {
      d: 'M8 10.4a5 5 0 1 0 0 10 5 5 0 0 0 0-10Zm0 3.1a1.9 1.9 0 1 1 0 3.8 1.9 1.9 0 0 1 0-3.8Z',
    },
    { d: 'm11.6 12.4 8.8-8.8', mode: 'stroke', strokeWidth: 2.8 },
    { d: 'm15.4 8.6 2.6 2.6M18 6l2.6 2.6', mode: 'stroke', strokeWidth: 2.4 },
  ],
  files: [
    {
      d: 'M4.4 3.4h4.7c.8 0 1.5.3 2 .8l1.6 1.6h6.9a2.4 2.4 0 0 1 2.4 2.4v9.4a2.4 2.4 0 0 1-2.4 2.4H4.4A2.4 2.4 0 0 1 2 17.6V5.8a2.4 2.4 0 0 1 2.4-2.4Zm1 6.3h13.2a.8.8 0 0 1 0 1.6H5.4a.8.8 0 0 1 0-1.6Z',
    },
  ],
  terminal: [
    {
      d: 'M5.2 3.4h13.6a2.8 2.8 0 0 1 2.8 2.8v11.6a2.8 2.8 0 0 1-2.8 2.8H5.2a2.8 2.8 0 0 1-2.8-2.8V6.2a2.8 2.8 0 0 1 2.8-2.8Zm1.2 4.9 1.9-1.6 4.6 5.1-4.6 5.1-1.9-1.6 3.2-3.5-3.2-3.5Zm6.8 6.3h4.6a.9.9 0 0 1 0 1.8h-4.6a.9.9 0 0 1 0-1.8Z',
    },
  ],
  docker: [
    { d: 'M12 2.2 21 7.2l-9 5-9-5 9-5Z' },
    { d: 'M3 10.2 12 15.2l9-5v2.4l-9 5-9-5v-2.4Z' },
    { d: 'M3 15.2 12 20.2l9-5v2.4l-9 5-9-5v-2.4Z' },
  ],
};

export function appIconScheme(theme: ThemeName): AppIconScheme {
  return SCHEME_BY_THEME[theme];
}

export function appTileColor(theme: ThemeTokens, tile: string): string {
  const token = TILE_COLOR_TOKEN[tile] ?? 'primary';
  return theme.color[token];
}

function parseHex(color: string): [number, number, number] | null {
  const value = color.trim();
  if (!value.startsWith('#')) return null;
  const raw = value.slice(1);
  const full = raw.length === 3 ? raw.split('').map((char) => char + char).join('') : raw;
  if (full.length !== 6) return null;
  const int = Number.parseInt(full, 16);
  if (Number.isNaN(int)) return null;
  return [(int >> 16) & 255, (int >> 8) & 255, int & 255];
}

function luminance([r, g, b]: [number, number, number]): number {
  const channel = (value: number) => {
    const x = value / 255;
    return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

export function contrastRatio(a: string, b: string): number {
  const left = parseHex(a);
  const right = parseHex(b);
  if (!left || !right) return 1;
  const la = luminance(left);
  const lb = luminance(right);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

function schemeBInk(start: string, end: string, mode: ThemeMode): string {
  const dark = Math.min(contrastRatio(start, DARK_INK), contrastRatio(end, DARK_INK));
  const light = Math.min(contrastRatio(start, LIGHT_INK), contrastRatio(end, LIGHT_INK));
  if (dark === 1 && light === 1) return mode === 'dark' ? LIGHT_INK : DARK_INK;
  return dark >= light ? DARK_INK : LIGHT_INK;
}

export function schemeBTile(tileColor: string, theme: ThemeTokens): SchemeBTile {
  const start = mixColor(tileColor, theme.color.surface2, 0.3);
  const end = mixColor(tileColor, theme.color.surface, 0.4);
  return {
    start,
    end,
    border: mixColor(tileColor, theme.color.border, 0.54),
    ink: schemeBInk(start, end, theme.mode),
  };
}

const pathCache = new Map<string, Path2D>();

function glyphPath(d: string): Path2D {
  let path = pathCache.get(d);
  if (!path) {
    path = new Path2D(d);
    pathCache.set(d, path);
  }
  return path;
}

export function drawAppGlyph(
  ctx: CanvasRenderingContext2D,
  app: AppIconKey,
  cx: number,
  cy: number,
  size: number,
  color: string,
) {
  const parts = APP_GLYPHS[app] ?? APP_GLYPHS.about;
  const scale = size / 24;
  ctx.save();
  ctx.translate(cx - size / 2, cy - size / 2);
  ctx.scale(scale, scale);
  ctx.fillStyle = color;
  ctx.strokeStyle = color;
  for (const part of parts) {
    const path = glyphPath(part.d);
    if (part.mode === 'stroke') {
      ctx.lineWidth = part.strokeWidth ?? 2;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      ctx.stroke(path);
    } else {
      ctx.fill(path, 'evenodd');
    }
  }
  ctx.restore();
}
