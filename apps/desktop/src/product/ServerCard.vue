<script setup lang="ts">
import { computed } from 'vue';
import { LgButton, LgDropdown, LgIcon, LgSparkline, LgStatusPill } from '@lightguard/lightui';
import type { Server } from './types';
import { toast } from './ui';

const props = defineProps<{ server: Server }>();

const emit = defineEmits<{
  open: [server: Server];
  retry: [server: Server];
  credentials: [server: Server];
  remove: [server: Server];
}>();

const alive = computed(() => props.server.status === 'online' || props.server.status === 'warning');
const connecting = computed(() => props.server.status === 'connecting');

const pill = computed(() => {
  switch (props.server.status) {
    case 'online':
      return { label: '在线', tone: 'online' as const };
    case 'warning':
      return { label: '需关注', tone: 'warning' as const };
    case 'connecting':
      return { label: '连接中', tone: 'info' as const };
    case 'error':
      return { label: '连接失败', tone: 'danger' as const };
    default:
      return { label: '已离线', tone: 'muted' as const };
  }
});

const cpuSeries = computed(() => props.server.history.cpu.slice(-36));
const memSeries = computed(() => props.server.history.mem.slice(-36));

const level = (value: number, warnAt = 72, dangerAt = 88) =>
  value >= dangerAt ? 'danger' : value >= warnAt ? 'warning' : 'normal';

const menuOptions = computed(() => {
  const options: Array<{ key: string; label: string }> = [
    { key: 'open', label: '进入桌面' },
    { key: 'copy', label: '复制地址' },
    { key: 'retry', label: '重试连接' },
  ];
  if (props.server.needsAuth) options.push({ key: 'credentials', label: '更新凭据' });
  options.push({ key: 'remove', label: '移除服务器' });
  return options;
});

function onMenu(key: string) {
  if (key === 'open') emit('open', props.server);
  else if (key === 'retry') emit('retry', props.server);
  else if (key === 'credentials') emit('credentials', props.server);
  else if (key === 'remove') emit('remove', props.server);
  else if (key === 'copy') copyAddress();
}

async function copyAddress() {
  const address = `${props.server.ip}:${props.server.port}`;
  try {
    await navigator.clipboard.writeText(address);
    toast({ title: '地址已复制', message: address, tone: 'success' });
  } catch {
    toast({ title: '无法访问剪贴板', message: address, tone: 'warning' });
  }
}
</script>

<template>
  <article class="server-card" :data-status="server.status">
    <header class="server-card__head">
      <div class="server-card__title">
        <div class="server-card__pills">
          <LgStatusPill :label="pill.label" :tone="pill.tone" />
          <span class="server-card__group">{{ server.group }}</span>
          <span v-if="server.needsAuth" class="server-card__auth">需重新认证</span>
        </div>
        <h3>
          <button type="button" class="server-card__name" @click="emit('open', server)">
            {{ server.name }}
          </button>
        </h3>
        <p>{{ server.role }} · {{ server.region }}</p>
      </div>

      <LgDropdown trigger="click" placement="bottom-end" :options="menuOptions" @select="onMenu">
        <button type="button" class="server-card__more" :aria-label="`${server.name} 更多操作`">
          <LgIcon name="more" :size="17" />
        </button>
      </LgDropdown>
    </header>

    <div class="server-card__address">
      <span class="mono">{{ server.ip }}:{{ server.port }}</span>
      <span class="server-card__latency">
        <LgIcon name="wifi" :size="12" />
        {{ alive ? `${server.latency} ms` : '—' }}
      </span>
    </div>

    <p class="server-card__spec">
      {{ server.cpuCores ? `${server.cpuCores} vCPU` : '规格待读取' }}
      · {{ server.memTotal ? `${server.memTotal} GB` : '—' }}
      · {{ server.os }}
    </p>

    <div v-if="alive" class="server-card__metrics">
      <div class="metric" :data-level="level(server.metrics.cpu)">
        <div class="metric__head">
          <span>CPU</span>
          <strong>{{ server.metrics.cpu }}%</strong>
        </div>
        <div class="metric__bar"><i :style="{ width: `${server.metrics.cpu}%` }" /></div>
        <LgSparkline
          :values="cpuSeries"
          :height="26"
          :max="100"
          color="var(--lg-primary)"
          :label="`${server.name} CPU 趋势`"
        />
      </div>

      <div class="metric" :data-level="level(server.metrics.mem)">
        <div class="metric__head">
          <span>内存</span>
          <strong>{{ server.metrics.mem }}%</strong>
        </div>
        <div class="metric__bar"><i :style="{ width: `${server.metrics.mem}%` }" /></div>
        <LgSparkline
          :values="memSeries"
          :height="26"
          :max="100"
          color="var(--lg-accent)"
          :label="`${server.name} 内存趋势`"
        />
      </div>

      <div class="metric metric--disk" :data-level="level(server.metrics.disk, 78, 90)">
        <div class="metric__head">
          <span>磁盘</span>
          <strong>{{ server.metrics.disk }}%</strong>
        </div>
        <div class="metric__bar"><i :style="{ width: `${server.metrics.disk}%` }" /></div>
        <div class="metric__net">
          <span><LgIcon name="arrow-down" :size="11" />{{ server.metrics.down }} MB/s</span>
          <span><LgIcon name="arrow-up" :size="11" />{{ server.metrics.up }} MB/s</span>
        </div>
      </div>
    </div>

    <div v-else class="server-card__idle">
      <LgIcon :name="server.status === 'connecting' ? 'refresh' : 'alert'" :size="15" />
      <span>
        {{ connecting ? '正在建立连接…' : '暂无实时数据，恢复连接后自动刷新。' }}
      </span>
    </div>

    <div v-if="server.error" class="server-card__notice is-error">
      <strong>{{ server.error.message }}</strong>
      <span>{{ server.error.hint }}</span>
    </div>
    <div v-else-if="server.warning" class="server-card__notice is-warning">
      <strong>需要关注</strong>
      <span>{{ server.warning }}</span>
    </div>
    <div v-else-if="server.notice" class="server-card__notice is-info">
      <strong>提示</strong>
      <span>{{ server.notice }}</span>
    </div>

    <div class="server-card__tags">
      <span v-for="tag in server.tags" :key="tag" class="tag">{{ tag }}</span>
      <span class="server-card__sync">{{ server.lastSync }}</span>
    </div>

    <footer class="server-card__foot">
      <LgButton v-if="server.needsAuth" size="tiny" type="warning" ghost @click="emit('credentials', server)">
        更新凭据
      </LgButton>
      <LgButton
        v-if="!alive"
        size="small"
        :loading="connecting"
        :disabled="connecting"
        @click="emit('retry', server)"
      >
        <template #icon><LgIcon name="refresh" :size="14" /></template>
        重试连接
      </LgButton>
      <LgButton
        size="small"
        type="primary"
        :disabled="!alive"
        :title="alive ? undefined : '连接恢复后可进入桌面'"
        @click="emit('open', server)"
      >
        <template #icon><LgIcon name="monitor" :size="14" /></template>
        进入桌面
      </LgButton>
    </footer>
  </article>
</template>

<style scoped>
.server-card {
  display: flex;
  flex-direction: column;
  gap: 11px;
  padding: 16px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-lg);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-sm);
  transition:
    border-color var(--lg-dur-base) var(--lg-ease),
    transform var(--lg-dur-base) var(--lg-ease),
    box-shadow var(--lg-dur-base) var(--lg-ease);
}

.server-card:hover {
  border-color: var(--lg-border-strong);
  box-shadow: var(--lg-shadow-lg);
  transform: translateY(-1px);
}

.server-card[data-status='error'] {
  border-color: color-mix(in srgb, var(--lg-danger) 34%, var(--lg-border));
}

.server-card[data-status='warning'] {
  border-color: color-mix(in srgb, var(--lg-warning) 32%, var(--lg-border));
}

.server-card__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
}

.server-card__title {
  min-width: 0;
}

.server-card__pills {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.server-card__group,
.server-card__auth {
  padding: 2px 7px;
  border-radius: var(--lg-radius-full);
  background: var(--lg-hover);
  color: var(--lg-text-muted);
  font-size: 10px;
}

.server-card__auth {
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.server-card__title h3 {
  margin: 7px 0 0;
  font-size: 15px;
  font-weight: 650;
  letter-spacing: 0.01em;
}

.server-card__name {
  padding: 0;
  border: 0;
  background: none;
  color: var(--lg-text);
  font: inherit;
  cursor: pointer;
  text-align: left;
}

.server-card__name:hover {
  color: var(--lg-primary);
}

.server-card__title p {
  margin: 2px 0 0;
  color: var(--lg-text-muted);
  font-size: 11px;
}

.server-card__more {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.server-card__more:hover {
  border-color: var(--lg-border);
  background: var(--lg-hover);
  color: var(--lg-text);
}

.server-card__address {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 9px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text-secondary);
  font-size: 12px;
}

.mono {
  font-family: var(--lg-font-mono);
  font-size: 11.5px;
  letter-spacing: 0.02em;
}

.server-card__latency {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.server-card__spec {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 11px;
  overflow-wrap: anywhere;
}

.server-card__metrics {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.metric {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 8px 9px 6px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  min-width: 0;
}

.metric__head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 6px;
  color: var(--lg-text-muted);
  font-size: 10px;
}

.metric__head strong {
  color: var(--lg-text);
  font-family: var(--lg-font-mono);
  font-size: 12px;
  font-weight: 600;
}

.metric__bar {
  height: 4px;
  overflow: hidden;
  border-radius: var(--lg-radius-full);
  background: color-mix(in srgb, var(--lg-text) 8%, transparent);
}

.metric__bar i {
  display: block;
  height: 100%;
  border-radius: inherit;
  background: var(--lg-primary);
  transition: width var(--lg-dur-slow) var(--lg-ease);
}

.metric[data-level='warning'] .metric__bar i {
  background: var(--lg-warning);
}

.metric[data-level='danger'] .metric__bar i {
  background: var(--lg-danger);
}

.metric--disk {
  padding-bottom: 8px;
}

.metric__net {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  color: var(--lg-text-muted);
  font-size: 10px;
}

.metric__net span {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.server-card__idle {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 14px 10px;
  border: 1px dashed var(--lg-border-strong);
  border-radius: var(--lg-radius-sm);
  color: var(--lg-text-muted);
  font-size: 11.5px;
}

.server-card__notice {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  border-left: 2px solid var(--lg-info);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-info-soft);
  color: var(--lg-text-secondary);
  font-size: 11px;
  line-height: 1.5;
}

.server-card__notice strong {
  color: var(--lg-info);
  font-size: 11px;
  font-weight: 600;
}

.server-card__notice.is-warning {
  border-left-color: var(--lg-warning);
  background: var(--lg-warning-soft);
}

.server-card__notice.is-warning strong {
  color: var(--lg-warning);
}

.server-card__notice.is-error {
  border-left-color: var(--lg-danger);
  background: var(--lg-danger-soft);
}

.server-card__notice.is-error strong {
  color: var(--lg-danger);
}

.server-card__tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px;
  margin-top: auto;
}

.tag {
  padding: 2px 7px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  color: var(--lg-text-muted);
  font-size: 10px;
}

.server-card__sync {
  margin-left: auto;
  color: var(--lg-text-muted);
  font-size: 10px;
}

.server-card__foot {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 7px;
  padding-top: 10px;
  border-top: 1px solid var(--lg-border);
}

@media (max-width: 560px) {
  .server-card__metrics {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .metric--disk {
    grid-column: 1 / -1;
  }
}
</style>
