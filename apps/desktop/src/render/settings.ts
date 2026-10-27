import { reactive, watch } from 'vue';
import { DEFAULT_RENDER_SETTINGS, normalizeRenderSettings } from '@lightguard/lightui';
import type { RenderSettings } from '@lightguard/lightui';

const STORAGE_KEY = 'lg.render.v1';

function load(): RenderSettings {
  try {
    if (typeof localStorage === 'undefined') return normalizeRenderSettings(DEFAULT_RENDER_SETTINGS);
    const raw = localStorage.getItem(STORAGE_KEY);
    return normalizeRenderSettings(raw ? JSON.parse(raw) : DEFAULT_RENDER_SETTINGS);
  } catch {
    return normalizeRenderSettings(DEFAULT_RENDER_SETTINGS);
  }
}

export const renderSettings = reactive<RenderSettings>(load());

watch(
  renderSettings,
  (value) => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
    } catch {
      // 存储不可用时只保留当前会话的设置，不影响界面。
    }
  },
  { deep: true },
);

export function resetRenderSettings() {
  Object.assign(renderSettings, normalizeRenderSettings(DEFAULT_RENDER_SETTINGS));
}
