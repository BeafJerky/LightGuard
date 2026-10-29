<script setup lang="ts">
import { computed, ref } from 'vue';
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
import AlertRulesEditor from './alerts/AlertRulesEditor.vue';
import { PERMIT_OPTIONS } from './alerts/settings';
import { APP_BUILD, APP_VERSION } from './data';
import {
  backendAvailable,
  clearLocalData,
  restartLiveUpdates,
  state,
  updateSettings,
} from './store';
import { ssh, toSshError } from './ssh';
import { switchScopeTheme } from './theme';
import { confirm, toast } from './ui';
import type {
  AlertSettings,
  CloseBehavior,
  DesktopBarMode,
  DesktopDockMode,
  ThemeChoices,
} from './types';

type SettingsTab = 'appearance' | 'general' | 'security' | 'alerts' | 'data';

const tabs: Array<{ id: SettingsTab; label: string; icon: string }> = [
  { id: 'appearance', label: '外观', icon: 'sun' },
  { id: 'general', label: '通用', icon: 'refresh' },
  { id: 'security', label: '安全', icon: 'shield' },
  { id: 'alerts', label: '通知', icon: 'bell' },
  { id: 'data', label: '数据与关于', icon: 'database' },
];

const tab = ref<SettingsTab>('appearance');

const activeLabel = computed(
  () => tabs.find((item) => item.id === tab.value)?.label ?? '设置',
);

function onTabKeydown(event: KeyboardEvent) {
  const step = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
  if (!step) return;
  event.preventDefault();
  const index = tabs.findIndex((item) => item.id === tab.value);
  const next = tabs[(index + step + tabs.length) % tabs.length];
  if (!next) return;
  tab.value = next.id;
  (event.currentTarget as HTMLElement)
    .querySelector<HTMLElement>(`[data-tab="${next.id}"]`)
    ?.focus();
}

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

const closeOptions: Array<{ value: CloseBehavior; label: string }> = [
  { value: 'ask', label: '每次询问' },
  { value: 'tray', label: '最小化到托盘' },
  { value: 'quit', label: '退出应用' },
];

const closeMessages: Record<CloseBehavior, string> = {
  ask: '点击关闭时会询问去向',
  tray: '关闭窗口后驻留托盘，SSH 会话保持连接',
  quit: '关闭窗口即退出应用并断开全部会话',
};

const barOptions: Array<{ value: DesktopBarMode; label: string }> = [
  { value: 'floating', label: '悬浮胶囊' },
  { value: 'full', label: '通栏' },
];

function setDesktopBar(value: DesktopBarMode) {
  updateSettings({ desktopBar: value });
  toast({
    title: '状态条样式已更新',
    message:
      value === 'full'
        ? '通栏状态条已启用，桌面窗口会自动让出顶部区域'
        : '已恢复为悬浮胶囊状态条',
    tone: 'info',
    duration: 2400,
  });
}

const dockOptions: Array<{ value: DesktopDockMode; label: string }> = [
  { value: 'floating', label: '悬浮' },
  { value: 'full', label: '通栏' },
];

function setDesktopDock(value: DesktopDockMode) {
  updateSettings({ desktopDock: value });
  toast({
    title: 'Dock 样式已更新',
    message:
      value === 'floating'
        ? '悬浮 Dock 已启用，桌面窗口可以延伸到 Dock 下方'
        : '通栏 Dock 已启用，窗口底部会预留 Dock 空间',
    tone: 'info',
    duration: 2400,
  });
}

function setCloseBehavior(value: CloseBehavior) {
  updateSettings({ closeBehavior: value });
  toast({
    title: '关闭行为已更新',
    message: closeMessages[value],
    tone: 'info',
    duration: 2400,
  });
}

function toggleSetting(key: 'hostKey' | 'clipboardClear' | 'notifyFailure' | 'notifyThreshold', value: boolean) {
  updateSettings({ [key]: value });
  const title = {
    hostKey: '主机密钥校验',
    clipboardClear: '敏感内容自动清除',
    notifyFailure: '连接与安全提醒',
    notifyThreshold: '资源阈值提醒',
  }[key];
  toast({ title: `${title}${value ? '已开启' : '已关闭'}`, tone: value ? 'success' : 'info' });
}

function setPermitMinutes(value: number) {
  if (value !== 5 && value !== 15 && value !== 30) return;
  updateSettings({ permit: { defaultMinutes: value } });
  toast({
    title: '默认许可时长已更新',
    message: `解锁后 ${value} 分钟自动上锁`,
    tone: 'info',
    duration: 2200,
  });
}

const alertServers = computed(() =>
  state.servers.map((server) => ({ id: server.id, name: server.name })),
);

function onAlertSettings(next: AlertSettings) {
  updateSettings({ alerts: next });
}

const notifyTesting = ref(false);

const notifyError = ref('');

async function testNotify() {
  if (!backendAvailable || notifyTesting.value) return;
  notifyTesting.value = true;
  notifyError.value = '';
  try {
    const sent = await ssh.testNotification();
    if (sent) {
      toast({ title: '测试通知已发送', message: '请在系统通知中心查看', tone: 'success' });
    } else {
      notifyError.value = '系统未发出通知：请在 Windows 设置 → 通知中允许 LightGuard。';
    }
  } catch (error) {
    const parsed = toSshError(error);
    notifyError.value = parsed.hint || parsed.message;
  } finally {
    notifyTesting.value = false;
  }
}

async function onReset() {
  const ok = await confirm({
    title: '清空本地数据',
    message:
      '将断开全部会话，删除已保存的密码、私钥、主机指纹、服务器与连接记录，并恢复默认设置。此操作不可撤销。',
    confirmLabel: '清空',
    tone: 'danger',
  });
  if (!ok) return;
  await clearLocalData();
  toast({ title: '本地数据已清空', message: '服务器、凭据、记录与偏好已恢复默认', tone: 'success' });
}
</script>

<template>
  <div class="settings-page">
    <div class="settings-tabs" role="tablist" aria-label="设置分类" @keydown="onTabKeydown">
      <button
        v-for="item in tabs"
        :id="`settings-tab-${item.id}`"
        :key="item.id"
        type="button"
        role="tab"
        :data-tab="item.id"
        :aria-selected="tab === item.id"
        :class="{ 'is-active': tab === item.id }"
        @click="tab = item.id"
      >
        <LgIcon :name="item.icon" :size="14" />
        <span>{{ item.label }}</span>
      </button>
    </div>

    <section
      v-if="tab === 'appearance'"
      class="settings-grid"
      role="tabpanel"
      :aria-label="activeLabel"
    >
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

        <div class="setting-row">
          <div class="setting-text">
            <strong>服务器桌面状态条</strong>
            <span>悬浮为居中胶囊；通栏横跨桌面顶部，最大化窗口自动让出状态条区域。</span>
          </div>
          <div class="settings-segment" role="group" aria-label="服务器桌面状态条">
            <button
              v-for="option in barOptions"
              :key="option.value"
              type="button"
              :class="{ 'is-active': state.settings.desktopBar === option.value }"
              :aria-pressed="state.settings.desktopBar === option.value"
              @click="setDesktopBar(option.value)"
            >
              {{ option.label }}
            </button>
          </div>
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>服务器桌面 Dock</strong>
            <span>悬浮为底部居中胶囊，窗口可延伸到 Dock 下方；通栏贴底横跨整个桌面，窗口底部自动预留空间。</span>
          </div>
          <div class="settings-segment" role="group" aria-label="服务器桌面 Dock">
            <button
              v-for="option in dockOptions"
              :key="option.value"
              type="button"
              :class="{ 'is-active': state.settings.desktopDock === option.value }"
              :aria-pressed="state.settings.desktopDock === option.value"
              @click="setDesktopDock(option.value)"
            >
              {{ option.label }}
            </button>
          </div>
        </div>

        <p class="settings-note">
          <LgIcon name="info" :size="13" />
          两套选择分别保存。切换控制台主题不会改变服务器桌面，反之亦然；进入对应界面时才会应用各自主题。
        </p>
      </LgSurface>
    </section>

    <section
      v-if="tab === 'general'"
      class="settings-grid"
      role="tabpanel"
      :aria-label="activeLabel"
    >
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

      <LgSurface tone="solid" padding="lg" class="settings-card">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="monitor" :size="17" /></span>
          <div class="settings-card__title">
            <h2>窗口与后台</h2>
            <p>关闭窗口后的去向与托盘驻留</p>
          </div>
        </header>

        <div class="setting-row">
          <div class="setting-text">
            <strong>关闭窗口时</strong>
            <span>驻留托盘期间 SSH 会话保持连接；退出应用会断开全部会话。</span>
          </div>
          <div class="settings-segment" role="group" aria-label="关闭窗口时">
            <button
              v-for="option in closeOptions"
              :key="option.value"
              type="button"
              :class="{ 'is-active': state.settings.closeBehavior === option.value }"
              :aria-pressed="state.settings.closeBehavior === option.value"
              @click="setCloseBehavior(option.value)"
            >
              {{ option.label }}
            </button>
          </div>
        </div>
      </LgSurface>

      <RenderSettingsPanel :table-rows="2000" />
    </section>

    <section
      v-if="tab === 'security'"
      class="settings-grid"
      role="tabpanel"
      :aria-label="activeLabel"
    >
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
            <span>密码与私钥由系统凭据库派生的主密钥加密后保存在本机配置目录；Web 形态不保存凭据。</span>
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

      <LgSurface tone="solid" padding="lg" class="settings-card settings-card--wide">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="shield" :size="17" /></span>
          <div class="settings-card__title">
            <h2>运维许可</h2>
            <p>写操作在许可有效期内才可执行</p>
          </div>
          <LgTag size="small" :bordered="false" type="warning">仅内存</LgTag>
        </header>

        <div class="setting-row">
          <div class="setting-text">
            <strong>默认解锁时长</strong>
            <span>连接条解锁后，到期自动重新上锁；只影响之后发起的解锁</span>
          </div>
          <div class="settings-segment" role="group" aria-label="默认解锁时长">
            <button
              v-for="value in PERMIT_OPTIONS"
              :key="value"
              type="button"
              :class="{ 'is-active': state.settings.permit.defaultMinutes === value }"
              :aria-pressed="state.settings.permit.defaultMinutes === value"
              @click="setPermitMinutes(value)"
            >
              {{ value }} 分钟
            </button>
          </div>
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>终端命令不受约束</strong>
            <span>许可只拦截 LightGuard 发起的写操作；SSH 终端里输入的命令照常执行。</span>
          </div>
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>审计保留</strong>
            <span>最近 30 天或 500 条（先到者为准），不支持单独删除</span>
          </div>
          <LgTag size="small" :bordered="false" type="info">只读</LgTag>
        </div>
      </LgSurface>
    </section>

    <section
      v-if="tab === 'alerts'"
      class="settings-grid"
      role="tabpanel"
      :aria-label="activeLabel"
    >
      <LgSurface tone="solid" padding="lg" class="settings-card settings-card--wide">
        <header class="settings-card__head">
          <span class="settings-card__icon"><LgIcon name="bell" :size="17" /></span>
          <div class="settings-card__title">
            <h2>通知</h2>
            <p>只在需要你介入时提醒</p>
          </div>
        </header>

        <div class="setting-row">
          <div class="setting-text">
            <strong>连接与安全提醒</strong>
            <span>离线、主机指纹变更或登录异常时提醒</span>
          </div>
          <LgSwitch
            :value="state.settings.notifyFailure"
            aria-label="连接与安全提醒"
            @update:value="(value: boolean) => toggleSetting('notifyFailure', value)"
          />
        </div>

        <div class="setting-row">
          <div class="setting-text">
            <strong>资源阈值提醒</strong>
            <span>CPU / 内存 / 磁盘 / 负载持续超过阈值时提醒</span>
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
            <span v-if="backendAvailable">发送一条系统原生通知，检查权限与样式</span>
            <span v-else>仅桌面客户端可用</span>
            <span v-if="notifyError" class="setting-warn" role="alert">{{ notifyError }}</span>
          </div>
          <LgButton size="small" :disabled="!backendAvailable || notifyTesting" @click="testNotify">
            {{ notifyTesting ? '发送中…' : '发送测试' }}
          </LgButton>
        </div>

        <div class="setting-row rules-section">
          <div class="setting-text">
            <strong>告警规则</strong>
            <span>更改后立即生效；选择单台服务器可写入独立规则</span>
          </div>
        </div>
        <AlertRulesEditor
          :settings="state.settings.alerts"
          :servers="alertServers"
          @update="onAlertSettings"
        />
      </LgSurface>
    </section>

    <section
      v-if="tab === 'data'"
      class="settings-grid"
      role="tabpanel"
      :aria-label="activeLabel"
    >
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

      <LgSurface tone="solid" padding="lg" class="settings-card settings-card--wide">
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
          <div><dt>数据</dt><dd>桌面端经 SSH 只读采集 · 数据仅保存在本机</dd></div>
          <div><dt>技术栈</dt><dd>Vue 3 · Tauri 2 · Rust · russh</dd></div>
        </dl>

        <div class="setting-row">
          <div class="setting-text">
            <strong>清空本地数据</strong>
            <span>断开全部会话，删除服务器、凭据、主机指纹与连接记录。</span>
          </div>
          <LgButton size="small" type="error" secondary @click="onReset">清空</LgButton>
        </div>
      </LgSurface>
    </section>
  </div>
</template>

<style scoped>
.settings-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.settings-tabs {
  display: inline-flex;
  flex-wrap: wrap;
  align-self: flex-start;
  gap: 4px;
  padding: 3px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: var(--lg-surface);
}

.settings-tabs button {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 14px;
  border: 0;
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-muted);
  font-size: 12px;
  cursor: pointer;
  transition:
    background var(--lg-dur-fast) var(--lg-ease),
    color var(--lg-dur-fast) var(--lg-ease);
}

.settings-tabs button:hover {
  color: var(--lg-text);
}

.settings-tabs button.is-active {
  background: var(--lg-primary);
  color: var(--lg-on-primary);
  font-weight: 600;
}

.settings-tabs button:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 1px;
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

.setting-warn.setting-warn {
  color: var(--lg-danger);
}

.rules-section {
  border-top: 0;
  padding-top: 4px;
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
