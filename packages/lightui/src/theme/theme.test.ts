import { describe, expect, it } from 'vitest';
import { themes, themeOptions, resolveTheme } from './tokens';
import { toCssVars } from './css';
import { toNaiveOverrides } from './naive';
import type { ThemeName, ThemeTokens } from './types';

const themeEntries = Object.entries(themes) as Array<[ThemeName, ThemeTokens]>;

function walkStrings(value: unknown, path: string, visit: (path: string, value: string) => void) {
  if (typeof value === 'string') {
    visit(path, value);
    return;
  }
  if (value && typeof value === 'object') {
    for (const [key, child] of Object.entries(value)) walkStrings(child, `${path}.${key}`, visit);
  }
}

const colorKeys: Array<keyof ThemeTokens['color']> = [
  'bg',
  'bgSoft',
  'surface',
  'surface2',
  'surface3',
  'surfaceGlass',
  'border',
  'borderStrong',
  'borderWidth',
  'hover',
  'pressed',
  'text',
  'textSecondary',
  'textMuted',
  'primary',
  'primaryHover',
  'primaryPressed',
  'primarySoft',
  'onPrimary',
  'success',
  'successSoft',
  'accent',
  'accentSoft',
  'warning',
  'warningSoft',
  'danger',
  'dangerSoft',
  'info',
  'infoSoft',
  'focus',
  'chartGrid',
];

describe('theme tokens', () => {
  it('defines all 6 concrete themes plus system', () => {
    expect(Object.keys(themes)).toEqual(['dark', 'light', 'claude', 'pixel', 'nes', 'tech']);
    expect(themeOptions).toHaveLength(7);
    expect(themeOptions[0].id).toBe('system');
    expect(new Set(themeOptions.map((option) => option.id)).size).toBe(7);
  });

  it.each(themeEntries)('%s has every color token', (_name, theme) => {
    for (const key of colorKeys) {
      expect(theme.color[key], `color.${key}`).toBeTruthy();
    }
    expect(theme.radius.sm && theme.radius.md && theme.radius.lg && theme.radius.xl && theme.radius.full).toBeTruthy();
    expect(theme.font.sans && theme.font.mono && theme.font.display).toBeTruthy();
    expect(theme.motion.fast && theme.motion.base && theme.motion.slow && theme.motion.ease && theme.motion.spring).toBeTruthy();
    expect(theme.surface.desktopWallpaper && theme.surface.appBgImage).toBeTruthy();
    expect(theme.desktop.text && theme.desktop.textMuted).toBeTruthy();
    expect(theme.icon.stroke && theme.icon.linecap && theme.icon.linejoin).toBeTruthy();
  });

  it('resolves system to the default pack only', () => {
    expect(resolveTheme('system', true)).toBe(themes.dark);
    expect(resolveTheme('system', false)).toBe(themes.light);
    for (const id of ['dark', 'light', 'claude', 'pixel', 'nes', 'tech'] as const) {
      expect(resolveTheme(id, true)).toBe(themes[id]);
      expect(resolveTheme(id, false)).toBe(themes[id]);
    }
  });

  it('keeps pixel themes square and stepped', () => {
    for (const name of ['pixel', 'nes'] as const) {
      const theme = themes[name];
      expect(theme.radius.md).toBe('0');
      expect(theme.radius.full).toBe('0');
      expect(theme.motion.ease).toMatch(/^steps\(/);
      expect(theme.font.display).toBe('"LG Pixel", "Fusion Pixel 12px", "Zpix", monospace');
    }
  });
});

describe('toCssVars', () => {
  it.each(themeEntries)('%s emits prefixed, non-empty variables on a single source', (_name, theme) => {
    const vars = toCssVars(theme);
    for (const [key, value] of Object.entries(vars)) {
      expect(key.startsWith('--lg-'), key).toBe(true);
      expect(value, key).toBeTruthy();
    }
    expect(vars['--lg-bg']).toBe(theme.color.bg);
    expect(vars['--lg-primary']).toBe(theme.color.primary);
    expect(vars['--lg-text']).toBe(theme.color.text);
    expect(vars['--lg-border-width']).toBe(theme.color.borderWidth);
    expect(vars['--lg-radius-md']).toBe(theme.radius.md);
    expect(vars['--lg-font-display']).toBe(theme.font.display);
    expect(vars['--lg-dur-base']).toBe(theme.motion.base);
    expect(vars['--lg-ease']).toBe(theme.motion.ease);
    expect(vars['--lg-desktop-wallpaper']).toBe(theme.surface.desktopWallpaper);
    expect(vars['--lg-icon-stroke']).toBe(theme.icon.stroke);
  });

  it('maps the same token set for every theme', () => {
    const reference = Object.keys(toCssVars(themes.dark)).sort();
    for (const [, theme] of themeEntries) {
      expect(Object.keys(toCssVars(theme)).sort()).toEqual(reference);
    }
  });
});

describe('toNaiveOverrides', () => {
  it.each(themeEntries)('%s bridges common and component colors', (_name, theme) => {
    const overrides = toNaiveOverrides(theme);
    expect(overrides.common?.primaryColor).toBe(theme.color.primary);
    expect(overrides.common?.baseColor).toBe(theme.color.bg);
    expect(overrides.common?.textColor1).toBe(theme.color.text);
    expect(overrides.common?.borderColor).toBe(theme.color.border);
    expect(overrides.common?.fontFamily).toBe(theme.font.sans);
    expect(overrides.Button?.colorPrimary).toBe(theme.color.primary);
    expect(overrides.Input?.borderFocus).toBe(theme.color.primary);
    expect(overrides.Switch?.railColorActive).toBe(theme.color.primary);
    expect(overrides.Progress?.fillColor).toBe(theme.color.primary);
    expect(overrides.DataTable?.thColor).toBe(theme.color.surface2);
    expect(overrides.Popover?.color).toBe(theme.color.surface2);
    expect(overrides.Dropdown?.optionTextColorActive).toBe(theme.color.primary);
    expect(overrides.Message?.color).toBe(theme.color.surface2);
    expect(overrides.Modal?.color).toBe(theme.color.surface);
    expect(overrides.Dialog?.titleTextColor).toBe(theme.color.text);
    expect(overrides.Input?.boxShadowFocus).toContain(theme.color.primarySoft);
  });

  it('inverts tooltip surfaces for light and dark modes', () => {
    const darkTooltip = toNaiveOverrides(themes.dark).Tooltip;
    expect(darkTooltip?.color).toBe(themes.dark.color.surface3);
    expect(darkTooltip?.textColor).toBe(themes.dark.color.text);

    const lightTooltip = toNaiveOverrides(themes.light).Tooltip;
    expect(lightTooltip?.color).toBe(themes.light.color.text);
    expect(lightTooltip?.textColor).toBe(themes.light.color.surface);
  });

  it('zeroes Naive geometry for pixel packs', () => {
    for (const name of ['pixel', 'nes'] as const) {
      const overrides = toNaiveOverrides(themes[name]) as Record<string, any>;
      expect(overrides.common.borderRadius).toBe('0');
      expect(overrides.Button.borderRadiusMedium).toBe('0');
      expect(overrides.Input.borderRadius).toBe('0');
      expect(overrides.Select.peers.InternalSelection.borderRadius).toBe('0');
      expect(overrides.Select.peers.InternalSelectMenu.borderRadius).toBe('0');
      expect(overrides.Dropdown.borderRadius).toBe('0');
      expect(overrides.Popover.borderRadius).toBe('0');
      expect(overrides.Dialog.borderRadius).toBe('0');
    }
  });

  it.each(themeEntries)('%s emits only non-empty override values', (_name, theme) => {
    walkStrings(toNaiveOverrides(theme), 'overrides', (path, value) => {
      expect(value, path).toBeTruthy();
    });
  });
});
