<script setup lang="ts">
import { computed } from 'vue';
import {
  LgButton,
  LgIcon,
  LgSurface,
  LgSwitch,
  LgTag,
  resolveTheme,
  themeOptions,
} from '@lightguard/lightui';
import type { ThemeId, ThemeTokens } from '@lightguard/lightui';
import BackupPanel from './BackupPanel.vue';
import RenderSettingsPanel from '../render/RenderSettingsPanel.vue';
import { APP_BUILD, APP_VERSION } from './data';
import { resetDemo, restartLiveUpdates, state, updateSettings } from './store';
import { switchScopeTheme } from './theme';
import { confirm, toast } from './ui';
import type { ThemeChoices } from './types';

const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;

interface ThemeCard {
  id: ThemeId;
  label: string;
  description: string;
  pack: string;
  tokens: ThemeTokens;
}

const baseThemes = computed<ThemeCard[]>(() =>
  themeOptions
    .filter((option) => option.pack === 'default')
    .map((option) => ({ ...option, tokens: resolveTheme(option.id, prefersDark) })),
);

const packThemes = computed<ThemeCard[]>(() =>
  themeOptions
    .filter((option) => option.pack !== 'default')
    .map((option) => ({ ...option, tokens: resolveTheme(option.id, prefersDark) })),
);

const scopes: Array<{
  key: keyof ThemeChoices;
  title: string;
  description: string;
}> = [
  {
    key: 'console',
    title: '控制台主题',
    description: '总览、连接记录、设置、弹窗与添加服务器引导',
  },
  {
    key: 'desktop',
    title: '服务器桌面主题',
    description: '壁纸、连接条、任务栏、开始菜单与桌面窗口',
  },
];

function isActive(scope: keyof ThemeChoices, id: ThemeId) {
  return state.themes[scope] === id;
}

function pickTheme(scope: keyof ThemeChoices, id: ThemeId) {
  if (isActive(scope, id)) return;
  switchScopeTheme(scope, id);
}

function previewStyle(tokens: ThemeTokens, scope: keyof ThemeChoices) {
  return scope === 'console'
    ? {
        backgroundColor: tokens.color.bg,
        backgroundImage: tokens.surface.appBgImage,
      }
    : {
        backgroundColor: tokens.color.bg,
        backgroundImage: tokens.surface.desktopWallpaper,
      };
}

function setAutoRefresh(value: boolean) {
  updateSettings({ autoRefresh: value });
  restartLiveUpdates();
  toast({
    title: value ? '自动刷新已开启' : '自动刷新已暂停',
    message: value ? `每 ${state.settings.interval} 秒更新一次` : '指标将保持不变',
    tone: 'info',
  });
}

function setInterval(value: number) {
  updateSettings({ interval: value, autoRefresh: true });
  restartLiveUpdates();
  toast({ title: '刷新频率已更新', message: `每 ${value} 秒采集一次`, tone: 'success', duration: 2200 });
}

function setIdle(value: number) {
  updateSettings({ idleLock: value });
  toast({ title: '空闲锁定已更新', message: `${value} 分钟无操作后锁定`, tone: 'info', duration: 2200 });
}

function toggleSetting(key: 'hostKey' | 'clipboardClear' | 'notifyFailure' | 'notifyThreshold', value: boolean) {
  updateSettings({ [key]: value });
  const title = {
    hostKey: '主机密钥校验',
    clipboardClear: '敏感内容自动清除',
    notifyFailure: '连接失败提醒',
    notifyThreshold: '资源阈值提醒',
  }[key];
  toast({ title: `${title}${value ? '已开启' : '已关闭'}`, tone: value ? 'success' : 'info' });
}

function testNotify() {
  toast({
    title: '这是一条测试通知',
    message: '正式版本中，连接失败或资源持续偏高时才会提醒。',
    tone: 'info',
  });
}

async function onReset() {
  const ok = await confirm({
    title: '重置演示数据',
    message: '将恢复 6 台示例服务器、11 条连接记录与默认设置，当前修改会丢失。',
    confirmLabel: '重置',
    tone: 'danger',
  });
  if (!ok) return;
  resetDemo();
  restartLiveUpdates();
  toast({ title: '演示数据已重置', message: '服务器、记录与偏好已恢复默认', tone: 'success' });
}
</script>

<template>
  <div class="settings-page">
    <section class="settings-grid">
      <LgSurface tone="solid" padding="lg" class="settings-card settings-card--wide">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="sun" :size="17" /></span>
          <div class="settings-card__title">
            <h2>外观</h2>
            <p>控制台与服务器桌面各有一套独立主题，互不联动</p>
          </div>
          <LgTag size="small" :bordered="false" type="info">两套主题</LgTag>
        </header>

        <div class="theme-scopes">
          <div v-for="scope in scopes" :key="scope.key" class="theme-scope">
            <div class="theme-scope__head">
              <div>
                <strong>{{ scope.title }}</strong>
                <span>{{ scope.description }}</span>
              </div>
              <span class="theme-scope__badge">独立保存</span>
            </div>

            <div class="theme-gallery">
              <div class="theme-gallery__section">
                <span class="theme-gallery__label">基础主题</span>
                <div class="theme-grid" role="radiogroup" :aria-label="scope.title">
                  <label
                    v-for="option in baseThemes"
                    :key="option.id"
                    class="theme-card"
                    :class="{ 'is-active': isActive(scope.key, option.id) }"
                  >
                    <input
                      class="theme-card__radio"
                      type="radio"
                      :name="`theme-${scope.key}`"
                      :value="option.id"
                      :checked="isActive(scope.key, option.id)"
                      @change="pickTheme(scope.key, option.id)"
                    />
                    <span class="theme-preview" :style="previewStyle(option.tokens, scope.key)">
                      <template v-if="scope.key === 'console'">
                        <span
                          class="theme-preview__side"
                          :style="{ backgroundColor: option.tokens.color.surfaceGlass, borderColor: option.tokens.color.border }"
                        >
                          <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                        </span>
                        <span class="theme-preview__main">
                          <span
                            class="theme-preview__tile"
                            :style="{ backgroundColor: option.tokens.color.surface, borderColor: option.tokens.color.border }"
                          >
                            <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          </span>
                          <span
                            class="theme-preview__tile"
                            :style="{ backgroundColor: option.tokens.color.surface, borderColor: option.tokens.color.border }"
                          >
                            <i :style="{ backgroundColor: option.tokens.color.accent }" />
                          </span>
                          <span
                            class="theme-preview__row"
                            :style="{ backgroundColor: option.tokens.color.surface, borderColor: option.tokens.color.border }"
                          />
                        </span>
                      </template>
                      <template v-else>
                        <span
                          class="theme-preview__window"
                          :style="{ backgroundColor: option.tokens.color.surfaceGlass, borderColor: option.tokens.color.border }"
                        >
                          <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          <span class="theme-preview__lines">
                            <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                            <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                            <i :style="{ backgroundColor: option.tokens.color.accent }" />
                          </span>
                        </span>
                        <span
                          class="theme-preview__taskbar"
                          :style="{ backgroundColor: option.tokens.color.surfaceGlass, borderColor: option.tokens.color.border }"
                        >
                          <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                        </span>
                      </template>
                    </span>
                    <span class="theme-card__meta">
                      <strong>{{ option.label }}</strong>
                      <small>{{ option.description }}</small>
                    </span>
                    <span class="theme-card__check" aria-hidden="true">
                      <LgIcon name="check" :size="12" />
                    </span>
                  </label>
                </div>
              </div>

              <div class="theme-gallery__section">
                <span class="theme-gallery__label">风格主题</span>
                <div class="theme-grid" role="radiogroup" :aria-label="`${scope.title} · 风格主题`">
                  <label
                    v-for="option in packThemes"
                    :key="option.id"
                    class="theme-card"
                    :class="{ 'is-active': isActive(scope.key, option.id) }"
                  >
                    <input
                      class="theme-card__radio"
                      type="radio"
                      :name="`theme-${scope.key}`"
                      :value="option.id"
                      :checked="isActive(scope.key, option.id)"
                      @change="pickTheme(scope.key, option.id)"
                    />
                    <span class="theme-preview" :style="previewStyle(option.tokens, scope.key)">
                      <template v-if="scope.key === 'console'">
                        <span
                          class="theme-preview__side"
                          :style="{ backgroundColor: option.tokens.color.surfaceGlass, borderColor: option.tokens.color.border }"
                        >
                          <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                        </span>
                        <span class="theme-preview__main">
                          <span
                            class="theme-preview__tile"
                            :style="{ backgroundColor: option.tokens.color.surface, borderColor: option.tokens.color.border }"
                          >
                            <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          </span>
                          <span
                            class="theme-preview__tile"
                            :style="{ backgroundColor: option.tokens.color.surface, borderColor: option.tokens.color.border }"
                          >
                            <i :style="{ backgroundColor: option.tokens.color.accent }" />
                          </span>
                          <span
                            class="theme-preview__row"
                            :style="{ backgroundColor: option.tokens.color.surface, borderColor: option.tokens.color.border }"
                          />
                        </span>
                      </template>
                      <template v-else>
                        <span
                          class="theme-preview__window"
                          :style="{ backgroundColor: option.tokens.color.surfaceGlass, borderColor: option.tokens.color.border }"
                        >
                          <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          <span class="theme-preview__lines">
                            <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                            <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                            <i :style="{ backgroundColor: option.tokens.color.accent }" />
                          </span>
                        </span>
                        <span
                          class="theme-preview__taskbar"
                          :style="{ backgroundColor: option.tokens.color.surfaceGlass, borderColor: option.tokens.color.border }"
                        >
                          <i :style="{ backgroundColor: option.tokens.color.primary }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                          <i :style="{ backgroundColor: option.tokens.color.textMuted }" />
                        </span>
                      </template>
                    </span>
                    <span class="theme-card__meta">
                      <strong>{{ option.label }}</strong>
                      <small>{{ option.description }}</small>
                    </span>
                    <span class="theme-card__check" aria-hidden="true">
                      <LgIcon name="check" :size="12" />
                    </span>
                  </label>
                </div>
              </div>
            </div>
          </div>
        </div>

        <p class="settings-note">
          <LgIcon name="info" :size="13" />
          两套选择分别保存。切换控制台主题不会改变服务器桌面，反之亦然；进入对应界面时才会应用各自主题。
        </p>
      </LgSurface>

      <LgSurface tone="solid" padding="lg" class="settings-card">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="refresh" :size="17" /></span>
          <div class="settings-card__title">
            <h2>数据刷新</h2>
            <p>通过只读采集器拉取系统指标</p>
          </div>
        </header>

        <div class="setting-row">
          <div class="setting-text">
            <strong>自动刷新</strong>
            <span>保持总览与服务器桌面的数据新鲜度</span>
          </div>
          <LgSwitch
            :value="state.settings.autoRefresh"
            aria-label="自动刷新"
            @update:value="setAutoRefresh"
          />
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>刷新频率</strong>
            <span>当前每 {{ state.settings.interval }} 秒刷新一次</span>
          </div>
          <div class="settings-segment" role="group" aria-label="刷新频率">
            <button
              v-for="value in [3, 5, 10]"
              :key="value"
              type="button"
              :class="{ 'is-active': state.settings.interval === value }"
              :aria-pressed="state.settings.interval === value"
              @click="setInterval(value)"
            >
              {{ value }}s
            </button>
          </div>
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>采样窗口</strong>
            <span>性能监视器展示最近 60 秒的采样</span>
          </div>
          <LgTag size="small" :bordered="false" type="info">60 秒</LgTag>
        </div>
      </LgSurface>

      <LgSurface tone="solid" padding="lg" class="settings-card settings-card--wide">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="shield" :size="17" /></span>
          <div class="settings-card__title">
            <h2>连接与安全</h2>
            <p>SSH 直连、主机密钥校验与凭据保护</p>
          </div>
        </header>

        <div class="setting-row">
          <div class="setting-text">
            <strong>主机密钥校验（TOFU）</strong>
            <span>首次连接记录服务器指纹；指纹变化时阻断连接并要求人工确认。</span>
          </div>
          <LgSwitch
            :value="state.settings.hostKey"
            aria-label="主机密钥校验"
            @update:value="(value: boolean) => toggleSetting('hostKey', value)"
          />
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>凭据加密存储</strong>
            <span>桌面客户端使用系统凭据库托管密码与私钥；Web 演示环境不保存凭据内容。</span>
          </div>
          <LgTag size="small" :bordered="false" type="success">
            <template #icon><LgIcon name="lock" :size="12" /></template>
            AES-256-GCM
          </LgTag>
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>敏感内容自动清除</strong>
            <span>离开连接页或断开后，自动清除剪贴板中的密码与私钥内容。</span>
          </div>
          <LgSwitch
            :value="state.settings.clipboardClear"
            aria-label="敏感内容自动清除"
            @update:value="(value: boolean) => toggleSetting('clipboardClear', value)"
          />
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>空闲自动锁定</strong>
            <span>无操作一段时间后需要重新验证本地工作区。</span>
          </div>
          <div class="settings-segment" role="group" aria-label="空闲自动锁定">
            <button
              v-for="value in [15, 30, 60]"
              :key="value"
              type="button"
              :class="{ 'is-active': state.settings.idleLock === value }"
              :aria-pressed="state.settings.idleLock === value"
              @click="setIdle(value)"
            >
              {{ value }} 分钟
            </button>
          </div>
        </div>
      </LgSurface>

      <LgSurface tone="solid" padding="lg" class="settings-card">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="bell" :size="17" /></span>
          <div class="settings-card__title">
            <h2>通知</h2>
            <p>只在需要你介入时提醒</p>
          </div>
        </header>

        <div class="setting-row">
          <div class="setting-text">
            <strong>连接失败提醒</strong>
            <span>主机不可达或认证失败时提醒一次</span>
          </div>
          <LgSwitch
            :value="state.settings.notifyFailure"
            aria-label="连接失败提醒"
            @update:value="(value: boolean) => toggleSetting('notifyFailure', value)"
          />
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>资源阈值提醒</strong>
            <span>CPU / 内存 / 磁盘连续 5 分钟超过 80% 时提醒</span>
          </div>
          <LgSwitch
            :value="state.settings.notifyThreshold"
            aria-label="资源阈值提醒"
            @update:value="(value: boolean) => toggleSetting('notifyThreshold', value)"
          />
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>测试通知</strong>
            <span>发送一条本地示例通知，检查提醒样式</span>
          </div>
          <LgButton size="small" @click="testNotify">发送测试</LgButton>
        </div>
      </LgSurface>

      <LgSurface tone="solid" padding="lg" class="settings-card settings-card--wide">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="database" :size="17" /></span>
          <div class="settings-card__title">
            <h2>配置备份与恢复</h2>
            <p>导出设置与 SSH 连接元数据；凭据永不明文写入文件</p>
          </div>
          <LgTag size="small" :bordered="false" type="info">.lgbackup</LgTag>
        </header>
        <BackupPanel />
      </LgSurface>

      <LgSurface tone="solid" padding="lg" class="settings-card">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="info" :size="17" /></span>
          <div class="settings-card__title">
            <h2>关于 LightGuard</h2>
            <p>轻守卫 · 面向小团队的 SSH 服务器守护台</p>
          </div>
        </header>

        <dl class="about-list">
          <div><dt>版本</dt><dd>v{{ APP_VERSION }}</dd></div>
          <div><dt>构建</dt><dd>{{ APP_BUILD }}</dd></div>
          <div><dt>形态</dt><dd>Web 控制台 + 桌面客户端</dd></div>
          <div><dt>数据</dt><dd>本地演示数据 · 不发起真实网络请求</dd></div>
          <div><dt>技术栈</dt><dd>Vue 3 · Tauri 2 · Rust · Axum</dd></div>
        </dl>

        <div class="setting-row">
          <div class="setting-text">
            <strong>演示数据</strong>
            <span>恢复示例服务器、连接记录与默认设置。</span>
          </div>
          <LgButton size="small" type="error" secondary @click="onReset">重置</LgButton>
        </div>
      </LgSurface>

      <RenderSettingsPanel :table-rows="2000" />
    </section>
  </div>
</template>

<style scoped>
.settings-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.settings-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.12fr) minmax(0, 0.88fr);
  gap: 14px;
  align-items: start;
}

.settings-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}

.settings-card--wide {
  grid-column: 1 / -1;
}

.settings-card__head {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.settings-card__icon {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 34px;
  height: 34px;
  border: 1px solid var(--lg-border);
  border-radius: 10px;
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.settings-card__title {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.settings-card__title h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 650;
  letter-spacing: -0.01em;
}

.settings-card__title p {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 11.5px;
  line-height: 1.5;
}

.theme-scopes {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
}

.theme-scope {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-bg-soft);
}

.theme-scope__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}

.theme-scope__head div {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.theme-scope__head strong {
  font-size: 12.5px;
  font-weight: 600;
}

.theme-scope__head span {
  color: var(--lg-text-muted);
  font-size: 11px;
}

.theme-scope__badge {
  flex: 0 0 auto;
  padding: 2px 7px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  color: var(--lg-text-muted);
  font-size: 10px;
  white-space: nowrap;
}

.theme-gallery {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.theme-gallery__section {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.theme-gallery__label {
  color: var(--lg-text-muted);
  font-size: 10.5px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.theme-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(112px, 1fr));
  gap: 7px;
}

.theme-card {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 6px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-surface);
  cursor: pointer;
  transition:
    border-color var(--lg-dur-fast) var(--lg-ease),
    box-shadow var(--lg-dur-fast) var(--lg-ease),
    transform var(--lg-dur-fast) var(--lg-ease);
}

.theme-card:hover {
  border-color: var(--lg-border-strong);
  transform: translateY(-1px);
}

.theme-card.is-active {
  border-color: var(--lg-primary);
  box-shadow: 0 0 0 1px var(--lg-primary);
}

.theme-card:focus-within {
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.theme-card__radio {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.theme-preview {
  display: flex;
  gap: 4px;
  height: 58px;
  padding: 5px;
  border: 1px solid rgba(127, 127, 127, 0.18);
  border-radius: 6px;
  overflow: hidden;
  background-size: cover;
}

.theme-preview__side {
  display: flex;
  flex: 0 0 24%;
  flex-direction: column;
  gap: 3px;
  padding: 4px;
  border: 1px solid;
  border-radius: 4px;
}

.theme-preview__side i {
  height: 3px;
  border-radius: 2px;
  opacity: 0.8;
}

.theme-preview__side i:first-child {
  width: 80%;
}

.theme-preview__side i:not(:first-child) {
  width: 60%;
  opacity: 0.4;
}

.theme-preview__main {
  display: grid;
  flex: 1;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-template-rows: 18px 1fr;
  gap: 4px;
  min-width: 0;
}

.theme-preview__tile {
  display: flex;
  align-items: flex-end;
  padding: 3px;
  border: 1px solid;
  border-radius: 4px;
}

.theme-preview__tile i {
  width: 60%;
  height: 3px;
  border-radius: 2px;
}

.theme-preview__row {
  grid-column: 1 / -1;
  border: 1px solid;
  border-radius: 4px;
}

.theme-preview__window {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  margin: 2px 8px 2px 2px;
  padding: 4px;
  border: 1px solid;
  border-radius: 4px;
  backdrop-filter: blur(2px);
}

.theme-preview__window > i {
  width: 45%;
  height: 3px;
  border-radius: 2px;
}

.theme-preview__lines {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.theme-preview__lines i {
  height: 3px;
  border-radius: 2px;
  opacity: 0.55;
}

.theme-preview__lines i:nth-child(2) {
  width: 75%;
}

.theme-preview__lines i:nth-child(3) {
  width: 55%;
  opacity: 0.9;
}

.theme-preview__taskbar {
  position: absolute;
  right: 5px;
  bottom: 5px;
  left: 33%;
  display: flex;
  align-items: center;
  gap: 3px;
  height: 9px;
  padding: 0 3px;
  border: 1px solid;
  border-radius: 3px;
  backdrop-filter: blur(2px);
}

.theme-preview {
  position: relative;
}

.theme-preview__taskbar i {
  width: 5px;
  height: 5px;
  border-radius: 1px;
  opacity: 0.7;
}

.theme-preview__taskbar i:first-child {
  opacity: 1;
}

.theme-card__meta {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.theme-card__meta strong {
  font-size: 11.5px;
  font-weight: 600;
}

.theme-card__meta small {
  overflow: hidden;
  color: var(--lg-text-muted);
  font-size: 10px;
  line-height: 1.3;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.theme-card__check {
  position: absolute;
  top: 9px;
  right: 9px;
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--lg-primary);
  color: var(--lg-on-primary);
  opacity: 0;
  transform: scale(0.7);
  transition:
    opacity var(--lg-dur-fast) var(--lg-ease),
    transform var(--lg-dur-fast) var(--lg-ease);
}

.theme-card.is-active .theme-card__check {
  opacity: 1;
  transform: scale(1);
}

.settings-note {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 11px;
  line-height: 1.6;
}

.settings-note :deep(.lg-icon) {
  margin-top: 2px;
  flex: 0 0 auto;
  color: var(--lg-info);
}

.setting-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding-top: 10px;
  border-top: 1px solid var(--lg-border);
}

.setting-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.setting-text strong {
  font-size: 12.5px;
  font-weight: 600;
}

.setting-text span {
  color: var(--lg-text-muted);
  font-size: 11px;
  line-height: 1.5;
}

.settings-segment {
  display: inline-flex;
  flex: 0 0 auto;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
}

.settings-segment button {
  padding: 5px 10px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--lg-text-muted);
  font-size: 11px;
  white-space: nowrap;
  cursor: pointer;
}

.settings-segment button:hover {
  color: var(--lg-text);
}

.settings-segment button.is-active {
  border-color: var(--lg-border);
  background: var(--lg-surface);
  color: var(--lg-text);
  font-weight: 600;
  box-shadow: var(--lg-shadow-sm);
}

.about-list {
  display: flex;
  flex-direction: column;
  gap: 0;
  margin: 0;
}

.about-list div {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 0;
  border-top: 1px solid var(--lg-border);
  font-size: 11.5px;
}

.about-list div:first-child {
  border-top: 0;
}

.about-list dt {
  color: var(--lg-text-muted);
}

.about-list dd {
  margin: 0;
  color: var(--lg-text-secondary);
  text-align: right;
}

@media (max-width: 1180px) {
  .settings-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .settings-card--wide {
    grid-column: auto;
  }

  .theme-scopes {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 640px) {
  .theme-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .setting-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 8px;
  }

  .settings-segment {
    width: 100%;
    justify-content: space-between;
  }

  .settings-segment button {
    flex: 1;
  }
}
</style>
