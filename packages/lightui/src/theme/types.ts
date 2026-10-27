export type ThemeName = 'dark' | 'light' | 'claude' | 'pixel' | 'nes' | 'tech';
export type ThemeId = 'system' | ThemeName;
export type ThemeMode = 'dark' | 'light';
export type ThemePack = 'default' | 'claude' | 'pixel' | 'nes' | 'tech';
export type ThemeScope = 'console' | 'desktop';

export interface ThemeColors {
  bg: string;
  bgSoft: string;
  surface: string;
  surface2: string;
  surface3: string;
  surfaceGlass: string;
  border: string;
  borderStrong: string;
  borderWidth: string;
  hover: string;
  pressed: string;
  text: string;
  textSecondary: string;
  textMuted: string;
  primary: string;
  primaryHover: string;
  primaryPressed: string;
  primarySoft: string;
  onPrimary: string;
  success: string;
  successSoft: string;
  accent: string;
  accentSoft: string;
  warning: string;
  warningSoft: string;
  danger: string;
  dangerSoft: string;
  info: string;
  infoSoft: string;
  focus: string;
  chartGrid: string;
}

export interface ThemeSurface {
  panelBlur: string;
  texture: string;
  textureSize: string;
  textureOpacity: string;
  appBgImage: string;
  appGridImage: string;
  appGridSize: string;
  desktopWallpaper: string;
  desktopVignette: string;
}

export interface ThemeTokens {
  id: ThemeName;
  label: string;
  description: string;
  mode: ThemeMode;
  pack: ThemePack;
  color: ThemeColors;
  radius: {
    sm: string;
    md: string;
    lg: string;
    xl: string;
    full: string;
  };
  shadow: {
    sm: string;
    lg: string;
    hard: string;
    color: string;
  };
  font: {
    sans: string;
    mono: string;
    display: string;
  };
  motion: {
    fast: string;
    base: string;
    slow: string;
    ease: string;
    spring: string;
  };
  surface: ThemeSurface;
  desktop: {
    text: string;
    textMuted: string;
    textShadow: string;
  };
  icon: {
    stroke: string;
    linecap: string;
    linejoin: string;
  };
}

export interface ThemeScopeContext {
  theme: Readonly<ThemeTokens>;
  scope: ThemeScope;
  portalEl: HTMLElement | null;
}
