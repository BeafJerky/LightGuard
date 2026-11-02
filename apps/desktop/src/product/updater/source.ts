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
