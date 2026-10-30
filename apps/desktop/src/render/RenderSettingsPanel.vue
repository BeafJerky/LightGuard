<script setup lang="ts">
import { computed } from 'vue';
import {
  LgButton,
  LgIcon,
  LgSelect,
  LgSwitch,
  LgSurface,
  LgTag,
  RENDER_TIER_META,
  RENDER_TIERS,
  useRenderContext,
} from '@lightguard/lightui';
import type { RenderBackend, RenderPreference, RenderSettings, RenderTier } from '@lightguard/lightui';
import { renderSettings, resetRenderSettings } from './settings';

const props = withDefaults(defineProps<{ tableRows?: number }>(), { tableRows: 0 });

const render = useRenderContext();

const backendLabel: Record<RenderBackend, string> = {
  dom: 'DOM',
  canvas: 'Canvas',
  webgl: 'WebGL',
};

const globalOptions = [
  { label: '自动', value: 'auto' },
  { label: 'DOM', value: 'dom' },
  { label: 'Canvas', value: 'canvas' },
  { label: 'WebGL', value: 'webgl' },
];

const preferenceOptions = [
  { label: '继承全局', value: 'inherit' },
  { label: '自动', value: 'auto' },
  { label: 'DOM', value: 'dom' },
  { label: 'Canvas', value: 'canvas' },
  { label: 'WebGL', value: 'webgl' },
];

const intensityOptions = [
  { label: '低', value: 'low' },
  { label: '中', value: 'medium' },
  { label: '高', value: 'high' },
];

const capabilities = computed(() => render?.value.capabilities);

function optionsFor(tier: RenderTier) {
  return preferenceOptions.filter(
    (option) =>
      option.value === 'inherit' ||
      option.value === 'auto' ||
      RENDER_TIER_META[tier].backends.includes(option.value as RenderBackend),
  );
}

function hintFor(tier: RenderTier) {
  if (tier === 'console.charts') return { count: 16 };
  if (tier === 'console.tables') return { count: props.tableRows };
  return undefined;
}

function actual(tier: RenderTier) {
  const backend = render ? render.value.resolve(tier, hintFor(tier)) : 'dom';
  return backendLabel[backend];
}

function preferenceFor(tier: RenderTier) {
  return renderSettings.tiers[tier] ?? 'inherit';
}

function setGlobal(value: string) {
  renderSettings.global = value as RenderSettings['global'];
}

function setPreference(tier: RenderTier, value: string) {
  renderSettings.tiers[tier] = value as RenderPreference;
}

function setEffectsEnabled(value: boolean) {
  renderSettings.effects.enabled = value;
}

function setIntensity(value: string) {
  renderSettings.effects.intensity = value as RenderSettings['effects']['intensity'];
}

function setA11yForceDom(value: boolean) {
  renderSettings.a11yForceDom = value;
}
</script>

<template>
  <LgSurface tone="solid" padding="lg" class="panel render-panel" data-lg-render-panel>
    <div class="panel-head">
      <h2><LgIcon name="layers" :size="16" /> 渲染后端</h2>
      <LgTag size="small" :bordered="false">分层开关</LgTag>
    </div>

    <p class="panel-note">
      默认装饰层使用 DOM，终端在可用时优先 WebGL。切换只影响对应层级；能力不足、强制色彩、减少动效或无障碍模式会自动回退。
    </p>

    <div class="render-global">
      <label for="lg-render-global">全局默认</label>
      <LgSelect
        id="lg-render-global"
        size="small"
        :value="renderSettings.global"
        :options="globalOptions"
        @update:value="setGlobal"
      />
    </div>

    <div v-for="tier in RENDER_TIERS" :key="tier" class="render-row">
      <div class="render-row__meta">
        <strong>{{ RENDER_TIER_META[tier].label }}</strong>
        <span>{{ RENDER_TIER_META[tier].description }}</span>
      </div>
      <div class="render-row__control">
        <LgTag size="small" :bordered="false" type="info">{{ actual(tier) }}</LgTag>
        <LgSelect
          size="small"
          :value="preferenceFor(tier)"
          :options="optionsFor(tier)"
          :aria-label="`${RENDER_TIER_META[tier].label}渲染方式`"
          @update:value="(value: RenderPreference) => setPreference(tier, value)"
        />
      </div>
    </div>

    <div class="render-row render-row--effects">
      <div class="render-row__meta">
        <strong>特效开关</strong>
        <span>粒子、扫描线与网格装饰；减少动效时强制关闭</span>
      </div>
      <div class="render-row__control">
        <LgSwitch
          :value="renderSettings.effects.enabled"
          size="small"
          aria-label="桌面特效开关"
          @update:value="setEffectsEnabled"
        />
        <LgSelect
          size="small"
          :value="renderSettings.effects.intensity"
          :options="intensityOptions"
          :disabled="!renderSettings.effects.enabled"
          aria-label="桌面特效强度"
          @update:value="setIntensity"
        />
      </div>
    </div>

    <div class="render-row">
      <div class="render-row__meta">
        <strong>无障碍优先</strong>
        <span>强制所有层级使用 DOM，保留读屏、焦点与输入法</span>
      </div>
      <LgSwitch
        :value="renderSettings.a11yForceDom"
        size="small"
        aria-label="无障碍优先"
        @update:value="setA11yForceDom"
      />
    </div>

    <div v-if="capabilities" class="render-capabilities">
      <span>Canvas 2D：{{ capabilities.canvas2d ? '可用' : '不可用' }}</span>
      <span>WebGL2：{{ capabilities.webgl2 ? '可用' : '不可用' }}</span>
      <span>WebGPU：{{ capabilities.webgpu ? '可用' : '不可用' }}</span>
      <span>强制色彩：{{ capabilities.forcedColors ? '开' : '关' }}</span>
    </div>

    <div class="control-row">
      <LgButton size="small" @click="resetRenderSettings">恢复默认</LgButton>
    </div>
  </LgSurface>
</template>

<style scoped>
.render-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
  grid-column: 1 / -1;
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.panel-head h2 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0;
  font-size: var(--lg-fs-body);
  font-weight: 650;
  color: var(--lg-text);
}

.panel-head h2 :deep(.lg-icon) {
  color: var(--lg-primary);
}

.panel-note {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  line-height: 1.6;
}

.control-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-top: 4px;
}

.render-global {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 16px;
  padding-bottom: 4px;
}

.render-global label {
  font-size: var(--lg-fs-body);
  color: var(--lg-text-secondary);
  white-space: nowrap;
}

.render-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 16px;
  padding-top: 10px;
  border-top: 1px solid var(--lg-border);
}

.render-row__meta {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.render-row__meta strong {
  font-size: var(--lg-fs-body);
  font-weight: 600;
  color: var(--lg-text);
}

.render-row__meta span {
  font-size: var(--lg-fs-sm);
  color: var(--lg-text-muted);
}

.render-row__control {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.render-row__control :deep(.n-select) {
  width: 118px;
}

.render-capabilities {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  padding-top: 8px;
  font-size: var(--lg-fs-sm);
  color: var(--lg-text-muted);
}
</style>
