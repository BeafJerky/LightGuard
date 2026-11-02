<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgButton, LgProgress, LgTag } from '@lightguard/lightui';
import { openUrl } from '@tauri-apps/plugin-opener';
import { APP_VERSION } from '../data';
import { confirm } from '../ui';
import { gateMessage, updateGate } from './gate';
import { updateController } from './index';

const state = updateController.state;
const deferred = ref(false);

const percent = computed(() => {
  const { downloaded, total } = state.progress;
  if (!total || total <= 0) return 0;
  return Math.min(100, Math.round((downloaded / total) * 100));
});

function formatBytes(bytes?: number) {
  if (!bytes || bytes <= 0) return '—';
  const units = ['B', 'KB', 'MB', 'GB'];
  const index = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  return `${(bytes / 1024 ** index).toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatDate(value?: string) {
  if (!value) return '';
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString('zh-CN');
}

async function checkNow() {
  deferred.value = false;
  await updateController.check({ manual: true });
}

async function confirmInstall() {
  const gate = updateGate();
  const message = gateMessage(gate);
  if (!message) return true;
  return confirm({
    title: '安装更新',
    message,
    confirmLabel: '继续安装',
    tone: 'danger',
  });
}

async function downloadNow() {
  if (!(await confirmInstall())) return;
  await updateController.download();
}

async function installNow() {
  if (!(await confirmInstall())) return;
  const installed = await updateController.install();
  if (!installed) return;
  const { relaunch } = await import('@tauri-apps/plugin-process');
  await relaunch();
}

const releaseUrl = 'https://github.com/BeafJerky/LightGuard/releases';

const PROXIES = [
  { label: 'wget.la', prefix: 'https://wget.la/' },
  { label: 'gh-proxy', prefix: 'https://gh-proxy.com/' },
  { label: 'boki', prefix: 'https://github.boki.moe/' },
  { label: 'ghfast', prefix: 'https://ghfast.top/' },
  { label: 'gh-proxy.org', prefix: 'https://gh-proxy.org/' },
  { label: 'ghproxy.net', prefix: 'https://ghproxy.net/' },
  { label: 'ghproxy.vip', prefix: 'https://ghproxy.vip/' },
  { label: 'fastgit', prefix: 'https://fastgit.cc/' },
];

// 清单可能来自直连或代理镜像；镜像清单的 url 已经带前缀，先还原成 GitHub 原始地址，
// 否则会拼出 `https://gh-proxy.com/https://gh-proxy.com/...`。
function directAssetUrl(value?: string) {
  if (!value) return '';
  const index = value.indexOf('https://github.com/');
  return index >= 0 ? value.slice(index) : '';
}

const proxyLinks = computed(() => {
  const url = directAssetUrl(state.info?.url);
  if (!url) return [];
  return PROXIES.map((proxy) => ({ label: proxy.label, href: `${proxy.prefix}${url}` }));
});

async function openExternal(url: string) {
  await openUrl(url).catch(() => undefined);
}
</script>

<template>
  <div class="update-panel">
    <div class="update-panel__row">
      <div class="setting-text">
        <strong>自动更新</strong>
        <span v-if="state.phase === 'idle'">启动后自动检查；当前版本 v{{ APP_VERSION }}</span>
        <span v-else-if="state.phase === 'checking'">正在检查更新…</span>
        <span v-else-if="state.phase === 'up-to-date'">已是最新版本。</span>
        <span v-else-if="state.phase === 'available'">发现新版本，确认后下载。</span>
        <span v-else-if="state.phase === 'downloading'">正在下载并校验更新包…</span>
        <span v-else-if="state.phase === 'ready'">更新包已就绪，重启后安装。</span>
        <span v-else-if="state.phase === 'installing'">正在安装，应用即将重启…</span>
        <span v-else-if="state.phase === 'error'">更新失败。</span>
      </div>
      <div class="update-panel__actions">
        <LgButton
          size="small"
          :disabled="state.phase === 'checking' || state.phase === 'downloading' || state.phase === 'installing'"
          @click="checkNow"
        >
          {{ state.phase === 'checking' ? '检查中…' : '检查更新' }}
        </LgButton>
      </div>
    </div>

    <div
      v-if="state.info && ['available', 'downloading', 'ready', 'installing'].includes(state.phase)"
      class="update-panel__body"
    >
      <div class="update-panel__headline">
        <strong>v{{ state.info.version }}</strong>
        <LgTag size="small" :bordered="false" type="info">可更新</LgTag>
        <span v-if="state.info.date">{{ formatDate(state.info.date) }}</span>
        <span v-if="state.info.size">{{ formatBytes(state.info.size) }}</span>
      </div>
      <p v-if="state.info.body && !deferred" class="update-panel__notes">{{ state.info.body }}</p>
      <template v-if="state.phase === 'downloading'">
        <LgProgress :percentage="percent" :height="6" />
        <span class="update-panel__progress">
          {{ formatBytes(state.progress.downloaded) }}
          <template v-if="state.progress.total"> / {{ formatBytes(state.progress.total) }}</template>
        </span>
      </template>
      <div v-if="state.phase === 'available'" class="update-panel__actions">
        <LgButton size="small" type="primary" @click="downloadNow">下载并安装</LgButton>
      </div>
      <div v-else-if="state.phase === 'ready'" class="update-panel__actions">
        <LgButton v-if="!deferred" size="small" @click="deferred = true">稍后</LgButton>
        <LgButton size="small" type="primary" @click="installNow">重启并安装</LgButton>
      </div>
      <div v-else-if="state.phase === 'installing'" class="update-panel__actions">
        <LgButton size="small" disabled>正在安装…</LgButton>
      </div>
    </div>

    <div v-if="state.phase === 'error'" class="update-panel__error" role="alert">
      <span>{{ state.error?.message }}</span>
      <div class="update-panel__actions">
        <LgButton size="small" @click="state.info ? downloadNow() : checkNow()">重试</LgButton>
        <LgButton size="small" secondary @click="openExternal(releaseUrl)">打开发布页</LgButton>
        <LgButton
          v-for="link in proxyLinks"
          :key="link.href"
          size="small"
          secondary
          @click="openExternal(link.href)"
        >
          {{ link.label }} 备用下载
        </LgButton>
      </div>
      <p class="update-panel__hint">
        如果清单可访问但更新包被阻断，可手动下载安装包；手动安装包同样来自 GitHub Releases。
      </p>
    </div>
  </div>
</template>

<style scoped>
.update-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-top: 12px;
  border-top: 1px solid var(--lg-border);
}

.update-panel__row,
.update-panel__headline,
.update-panel__actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.update-panel__row {
  justify-content: space-between;
}

/* SettingsPage.vue 的 .setting-text 只在它自己的 scoped 块里生效，不会穿透到本组件；
   这里按同样的值复刻一份，保证状态行与其它设置行一致。 */
.setting-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.setting-text strong {
  font-size: var(--lg-fs-body);
  font-weight: 600;
}

.setting-text span {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  line-height: 1.5;
}

.update-panel__body,
.update-panel__error {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--lg-border);
  border-radius: 10px;
  background: var(--lg-surface-soft, var(--lg-surface));
}

.update-panel__error {
  color: var(--lg-danger, var(--lg-error, #c0392b));
}

.update-panel__notes {
  margin: 0;
  white-space: pre-wrap;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
}

.update-panel__progress,
.update-panel__hint {
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-xs, 12px);
}

.update-panel__hint {
  margin: 0;
}
</style>
