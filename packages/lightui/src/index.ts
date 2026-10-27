export { default as LgConfigProvider } from './components/LgConfigProvider.vue';
export * from './components/wrappers';
export { default as LgIcon } from './components/LgIcon.vue';
export { default as LgIconSprite } from './components/LgIconSprite.vue';
export { default as LgSurface } from './components/LgSurface.vue';
export { default as LgKpi } from './components/LgKpi.vue';
export { default as LgStatusPill } from './components/LgStatusPill.vue';
export { default as LgSparkline } from './components/LgSparkline.vue';
export { default as LgVirtualTable } from './components/LgVirtualTable.vue';
export { default as LgTerminal } from './components/LgTerminal.vue';
export { default as LgMotion } from './components/LgMotion.vue';
export { default as LgRenderProvider } from './render/LgRenderProvider.vue';
export { default as LgCanvasSurface } from './render/CanvasSurface.vue';
export { default as LgAppBackground } from './render/LgAppBackground.vue';
export { default as LgDesktopEffects } from './render/LgDesktopEffects.vue';

export { RENDER_TIER_META, resolveRenderBackend } from './render/resolve';
export { DEFAULT_RENDER_SETTINGS, normalizeRenderSettings } from './render/settings';
export { DEFAULT_CAPABILITIES, detectCapabilities } from './render/capabilities';
export { LG_RENDER_KEY } from './render/context';
export { withAlpha, gridStep } from './render/paint/colors';
export { mixColor } from './render/paint/colors';
export { toXtermTheme } from './terminal/theme';
export { createSimulatedShell } from './terminal/simulator';
export { useRenderBackend, useRenderContext } from './render/useRenderBackend';
export type * from './render/types';
export { RENDER_TIERS } from './render/types';

export { themes, themeOptions, resolveTheme } from './theme/tokens';
export { toCssVars } from './theme/css';
export { toNaiveOverrides } from './theme/naive';
export { LG_SCOPE_KEY } from './theme/context';
export type * from './theme/types';

export {
  useDialog as useLgDialog,
  useMessage as useLgMessage,
  useNotification as useLgNotification,
} from 'naive-ui';
export type {
  DataTableColumns,
  DropdownOption,
  FormInst,
  FormRules,
  SelectOption,
} from 'naive-ui';
