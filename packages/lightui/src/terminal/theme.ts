import type { ITheme } from '@xterm/xterm';
import type { ThemeTokens } from '../theme/types';
import { mixColor, withAlpha } from '../render/paint/colors';

export function toXtermTheme(theme: ThemeTokens): ITheme {
  const { color, mode } = theme;
  const light = mode === 'light';
  const brighten = (value: string) => mixColor(value, '#ffffff', light ? 0.18 : 0.35);

  return {
    background: color.bg,
    foreground: color.text,
    cursor: color.primary,
    cursorAccent: color.bg,
    selectionBackground: withAlpha(color.primary, 0.32),
    selectionInactiveBackground: withAlpha(color.primary, 0.18),
    scrollbarSliderBackground: withAlpha(color.text, 0.16),
    scrollbarSliderHoverBackground: withAlpha(color.text, 0.3),
    scrollbarSliderActiveBackground: withAlpha(color.text, 0.42),
    black: light ? color.textSecondary : color.bgSoft,
    red: color.danger,
    green: color.success,
    yellow: color.warning,
    blue: color.info,
    magenta: color.accent,
    cyan: color.primary,
    white: color.textSecondary,
    brightBlack: color.textMuted,
    brightRed: brighten(color.danger),
    brightGreen: brighten(color.success),
    brightYellow: brighten(color.warning),
    brightBlue: brighten(color.info),
    brightMagenta: brighten(color.accent),
    brightCyan: brighten(color.primary),
    brightWhite: color.text,
  };
}
