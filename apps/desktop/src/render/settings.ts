import { reactive, watch } from 'vue';
import { DEFAULT_RENDER_SETTINGS, normalizeRenderSettings } from '@lightguard/lightui';
import type { RenderSettings } from '@lightguard/lightui';
import {
  clearLegacyRenderKey,
  hasLegacyRender,
  isPersistenceActive,
  loadedStoredState,
  writePersistence,
  writePersistenceNow,
} from '../product/persistence';

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

function snapshot(): RenderSettings {
  return {
    global: renderSettings.global,
    tiers: { ...renderSettings.tiers },
    effects: { ...renderSettings.effects },
    a11yForceDom: renderSettings.a11yForceDom,
  };
}

watch(
  renderSettings,
  (value) => {
    if (isPersistenceActive()) {
      writePersistence({
        op: 'renderSet',
        settings: {
          global: value.global,
          tiers: { ...value.tiers },
          effects: { ...value.effects },
          a11yForceDom: value.a11yForceDom,
        },
      });
      return;
    }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
    } catch {
      // 存储不可用时只保留当前会话的设置，不影响界面。
    }
  },
  { deep: true },
);

export async function bootstrapRenderSettings() {
  const stored = loadedStoredState();
  if (!stored) return;
  if (stored.render) {
    Object.assign(renderSettings, normalizeRenderSettings(stored.render));
    clearLegacyRenderKey();
    return;
  }
  if (!isPersistenceActive()) return;
  // 默认渲染设置不落库，只有旧版确实写过偏好时才迁移。
  if (!hasLegacyRender()) return;
  try {
    await writePersistenceNow({ op: 'renderSet', settings: snapshot() });
    clearLegacyRenderKey();
  } catch {
    // 写库失败时保留 legacy 存储，下一次启动继续尝试迁移。
  }
}

export function resetRenderSettings() {
  Object.assign(renderSettings, normalizeRenderSettings(DEFAULT_RENDER_SETTINGS));
}
