<script setup lang="ts">
import { computed } from 'vue';
import { LgButton, LgIcon, LgStatusPill } from '@lightguard/lightui';
import { APP_VERSION } from '../data';
import { statusLabel, statusTone } from './apps';
import { toast } from '../ui';
import type { Server } from '../types';

const props = defineProps<{
  server: Server;
  session: string;
  startedAt: string;
}>();

const emit = defineEmits<{ disconnect: [] }>();

const sessionAge = computed(() => {
  const started = new Date(props.startedAt).getTime();
  if (!Number.isFinite(started)) return '—';
  const minutes = Math.max(0, Math.floor((Date.now() - started) / 60000));
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = Math.floor(minutes / 60);
  return `${hours} 小时 ${minutes % 60} 分钟`;
});

const startedTime = computed(() => {
  const date = new Date(props.startedAt);
  if (!Number.isFinite(date.getTime())) return props.startedAt || '—';
  return `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
});

const hostKeyText = computed(() => {
  if (props.server.hostKeyState === 'new') return 'TOFU · 首次连接已记录';
  if (props.server.hostKeyState === 'known') return 'TOFU · 指纹与记录一致';
  return '—';
});

async function copy(text: string, label: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast({ title: `${label}已复制`, tone: 'success', duration: 2000 });
  } catch {
    toast({ title: '复制失败', message: '当前环境不允许访问剪贴板。', tone: 'warning' });
  }
}
</script>

<template>
  <div class="lgapp">
    <div class="lgapp-hero">
      <span class="lgapp-hero__avatar"><LgIcon name="key" :size="22" /></span>
      <div class="lgapp-hero__text">
        <h2>{{ server.name }} · SSH 会话</h2>
        <p>{{ server.user }}@{{ server.ip }}:{{ server.port }} · {{ server.authLabel }}</p>
      </div>
      <LgStatusPill :label="statusLabel(server.status)" :tone="statusTone(server.status)" />
    </div>

    <div class="lgapp-kpis">
      <div class="lgapp-kpi">
        <span>SSH 延迟</span>
        <strong>{{ server.latency }} <small>ms</small></strong>
        <small>{{ server.latency > 80 ? '偏高' : '良好' }}</small>
      </div>
      <div class="lgapp-kpi">
        <span>会话时长</span>
        <strong>{{ sessionAge }}</strong>
        <small>自 {{ startedTime }}</small>
      </div>
      <div class="lgapp-kpi">
        <span>认证方式</span>
        <strong>{{ server.authLabel }}</strong>
        <small>凭据不离开钥匙串</small>
      </div>
    </div>

    <div class="lgapp-section">
      <h3>会话</h3>
      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>会话 ID</span><strong class="lgapp-mono">{{ session }}</strong></div>
        <div class="lgapp-info__row"><span>目标地址</span><strong class="lgapp-mono">{{ server.ip }}:{{ server.port }}</strong></div>
        <div class="lgapp-info__row"><span>登录用户</span><strong>{{ server.user }}</strong></div>
        <div class="lgapp-info__row"><span>启动时间</span><strong>{{ startedAt }}</strong></div>
        <div class="lgapp-info__row"><span>客户端</span><strong>LightGuard Desktop v{{ APP_VERSION }}</strong></div>
        <div class="lgapp-info__row"><span>来源</span><strong>桌面客户端 · 本机直连</strong></div>
      </div>
    </div>

    <div class="lgapp-section">
      <h3>加密与主机密钥</h3>
      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>加密算法</span><strong class="lgapp-mono">{{ server.encryption?.cipher ?? '—' }}</strong></div>
        <div class="lgapp-info__row"><span>密钥交换</span><strong class="lgapp-mono">{{ server.encryption?.kex ?? '—' }}</strong></div>
        <div class="lgapp-info__row"><span>消息认证</span><strong class="lgapp-mono">{{ server.encryption?.mac ?? '—' }}</strong></div>
        <div class="lgapp-info__row"><span>主机密钥校验</span><strong>{{ hostKeyText }}</strong></div>
      </div>
      <div class="lgapp-fingerprint">
        <span>{{ server.fingerprint }}</span>
        <button
          type="button"
          class="lgapp-copy"
          aria-label="复制主机指纹"
          @click="copy(server.fingerprint, '主机指纹')"
        >
          <LgIcon name="copy" :size="13" />
        </button>
      </div>
    </div>

    <div class="lgapp-actions">
      <LgButton size="small" @click="copy(session, '会话 ID')">
        <template #icon><LgIcon name="copy" :size="14" /></template>
        复制会话 ID
      </LgButton>
      <LgButton size="small" type="error" secondary @click="emit('disconnect')">
        <template #icon><LgIcon name="power" :size="14" /></template>
        断开连接
      </LgButton>
    </div>

    <p class="lgapp-note">
      <LgIcon name="shield" :size="13" />
      连接记录只保存会话 ID、目标地址与主机指纹等元数据，不包含口令、私钥或终端内容。
    </p>
  </div>
</template>
