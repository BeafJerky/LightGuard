// 预设的 id 就是持久化值：除 auto 外必须是完整 HTTPS 清单 URL，
// 这样 Rust 的 `validate_source` 不必再维护一套短 id → URL 的映射。
export const UPDATE_SOURCE_PRESETS = [
  { id: 'auto', label: '自动（官方 + 镜像）' },
  {
    id: 'https://raw.githubusercontent.com/BeafJerky/LightGuard/main/update/latest.json',
    label: 'GitHub 直连',
  },
  {
    id: 'https://gh-proxy.com/https://raw.githubusercontent.com/BeafJerky/LightGuard/main/update/latest.cn.json',
    label: 'gh-proxy 镜像',
  },
  {
    id: 'https://cdn.jsdelivr.net/gh/BeafJerky/LightGuard@main/update/latest.cn.json',
    label: 'jsDelivr 镜像',
  },
] as const;

function isPresetSource(value: string): boolean {
  return UPDATE_SOURCE_PRESETS.some((preset) => preset.id === value);
}

export function normalizeUpdateSource(value: unknown): string {
  if (typeof value !== 'string') return 'auto';
  const trimmed = value.trim();
  if (trimmed === 'auto') return 'auto';
  try {
    const url = new URL(trimmed);
    if (url.protocol !== 'https:') return 'auto';
    if (url.username || url.password) return 'auto';
    if (!url.hostname) return 'auto';
    return url.toString();
  } catch {
    return 'auto';
  }
}

/**
 * 把持久化的更新源映射回界面状态。
 * 先归一化：非法值会退回 `auto`，因此会被当成预设而不是自定义地址，
 * 不会把无效输入留在草稿里反复提交。
 */
export function initialSourceDraft(updateSource: string): {
  customMode: boolean;
  draft: string;
} {
  const normalized = normalizeUpdateSource(updateSource);
  if (isPresetSource(normalized)) return { customMode: false, draft: '' };
  return { customMode: true, draft: normalized };
}

/** 下拉框的选中项：自定义模式恒为 `custom`，否则就是命中的预设 id。 */
export function sourceSelectionState(
  updateSource: string,
  customMode: boolean,
): { selection: string; customMode: boolean; draft: string } {
  if (customMode) return { selection: 'custom', customMode: true, draft: '' };
  const fallback = initialSourceDraft(updateSource);
  if (fallback.customMode) return { selection: 'custom', customMode: true, draft: fallback.draft };
  return { selection: normalizeUpdateSource(updateSource), customMode: false, draft: '' };
}
