import type {
  RenderBackend,
  RenderCapabilities,
  RenderHint,
  RenderPreference,
  RenderSettings,
  RenderTier,
  RenderTierMeta,
} from './types';

export const RENDER_TIER_META: Record<RenderTier, RenderTierMeta> = {
  'app.background': {
    id: 'app.background',
    label: '应用背景',
    description: '应用与桌面壁纸的底层绘制',
    backends: ['dom', 'canvas', 'webgl'],
  },
  'desktop.effects': {
    id: 'desktop.effects',
    label: '桌面特效',
    description: '粒子、扫描线、网格等装饰层',
    backends: ['dom', 'canvas'],
  },
  'desktop.icons': {
    id: 'desktop.icons',
    label: '桌面图标 / Dock',
    description: '图标与 Dock 的绘制层，交互仍由 DOM 覆盖',
    backends: ['dom', 'canvas'],
  },
  'console.charts': {
    id: 'console.charts',
    label: '监控图表',
    description: 'Sparkline 等趋势图',
    backends: ['dom', 'canvas'],
  },
  'console.tables': {
    id: 'console.tables',
    label: '大表格 / 日志',
    description: '进程、连接与日志列表',
    backends: ['dom', 'canvas'],
  },
  terminal: {
    id: 'terminal',
    label: '终端',
    description: '终端渲染器，WebGL 优先',
    backends: ['dom', 'webgl'],
  },
};

const CHART_CANVAS_THRESHOLD = 120;
const TABLE_CANVAS_THRESHOLD = 2000;

function supports(tier: RenderTier, backend: RenderBackend): boolean {
  return RENDER_TIER_META[tier].backends.includes(backend);
}

function autoBackend(
  tier: RenderTier,
  capabilities: RenderCapabilities,
  hint?: RenderHint,
): RenderBackend {
  switch (tier) {
    case 'app.background':
      return 'dom';
    case 'desktop.effects':
      return capabilities.canvas2d ? 'canvas' : 'dom';
    case 'desktop.icons':
      return 'dom';
    case 'console.charts':
      return capabilities.canvas2d && (hint?.count ?? 0) >= CHART_CANVAS_THRESHOLD ? 'canvas' : 'dom';
    case 'console.tables':
      return capabilities.canvas2d && (hint?.count ?? 0) >= TABLE_CANVAS_THRESHOLD ? 'canvas' : 'dom';
    case 'terminal':
      return capabilities.webgl2 ? 'webgl' : 'dom';
  }
}

function coerce(tier: RenderTier, wanted: RenderBackend, capabilities: RenderCapabilities): RenderBackend {
  if (!supports(tier, wanted)) return 'dom';
  if (wanted === 'canvas' && !capabilities.canvas2d) return 'dom';
  if (wanted === 'webgl' && !capabilities.webgl2) {
    return tier === 'terminal' ? 'dom' : capabilities.canvas2d ? 'canvas' : 'dom';
  }
  return wanted;
}

export interface ResolveRenderOptions {
  tier: RenderTier;
  preference: RenderPreference;
  settings: RenderSettings;
  capabilities: RenderCapabilities;
  hint?: RenderHint;
}

export function resolveRenderBackend({
  tier,
  preference,
  settings,
  capabilities,
  hint,
}: ResolveRenderOptions): RenderBackend {
  if (settings.a11yForceDom || capabilities.forcedColors) return 'dom';

  if (tier === 'desktop.effects') {
    if (!settings.effects.enabled || capabilities.reducedMotion) return 'dom';
  }

  const tierPreference = settings.tiers[tier];
  const effective =
    preference === 'inherit'
      ? tierPreference && tierPreference !== 'inherit'
        ? tierPreference
        : settings.global
      : preference;
  const wanted = effective === 'auto' ? autoBackend(tier, capabilities, hint) : effective;

  return coerce(tier, wanted, capabilities);
}
