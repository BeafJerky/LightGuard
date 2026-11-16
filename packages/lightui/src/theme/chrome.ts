import type { ThemeChrome, ThemeTokens } from './types';

export const WINDOWS_CHROME: ThemeChrome = {
  windowControls: 'windows',
  menuBar: false,
  dock: 'windows',
  material: 'solid',
  metrics: {
    menuBarHeight: '0px',
    windowRadius: 'var(--lg-radius-md)',
    windowBarHeight: '36px',
    trafficLightSize: '12px',
    dockHeight: '50px',
    dockMagnification: '1',
  },
};

const MACOS_CHROME: ThemeChrome = {
  windowControls: 'macos',
  menuBar: true,
  dock: 'macos',
  material: 'sidebar-blur',
  metrics: {
    menuBarHeight: '28px',
    windowRadius: '12px',
    windowBarHeight: '40px',
    trafficLightSize: '12px',
    dockHeight: '64px',
    dockMagnification: '1.28',
  },
};

export function chromeOf(theme: ThemeTokens): ThemeChrome {
  return theme.chrome ?? (theme.pack === 'macos' ? MACOS_CHROME : WINDOWS_CHROME);
}