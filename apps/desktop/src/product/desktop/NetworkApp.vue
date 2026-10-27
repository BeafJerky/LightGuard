<script setup lang="ts">
import { computed } from 'vue';
import { LgIcon, LgSparkline, LgStatusPill } from '@lightguard/lightui';
import { statusLabel, statusTone } from './apps';
import type { Server } from '../types';

const props = defineProps<{ server: Server }>();

const series = computed(() => props.server.history.net.slice(-120));

const primary = computed(() => props.server.interfaces[0] ?? null);

const totalTraffic = computed(() => {
  const values = series.value;
  const avg = values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0;
  return {
    down: (avg * 0.36).toFixed(2),
    up: ((avg * 0.36 * props.server.metrics.up) / Math.max(0.1, props.server.metrics.down)).toFixed(2),
  };
});
</script>

<template>
  <div class="lgapp">
    <div class="lgapp-head">
      <div>
        <h2>网络</h2>
        <p>{{ server.name }} · {{ primary?.name ?? 'eth0' }} · {{ primary?.speed ?? '—' }}</p>
      </div>
      <LgStatusPill :label="statusLabel(server.status)" :tone="statusTone(server.status)" />
    </div>

    <div class="lgapp-kpis">
      <div class="lgapp-kpi">
        <span>下行</span>
        <strong>{{ server.metrics.down.toFixed(1) }} <small>MB/s</small></strong>
        <small>最近 10 分钟均值 {{ totalTraffic.down }}</small>
      </div>
      <div class="lgapp-kpi">
        <span>上行</span>
        <strong>{{ server.metrics.up.toFixed(1) }} <small>MB/s</small></strong>
        <small>最近 10 分钟均值 {{ totalTraffic.up }}</small>
      </div>
      <div class="lgapp-kpi">
        <span>SSH 延迟</span>
        <strong>{{ server.latency }} <small>ms</small></strong>
        <small>{{ server.latency > 80 ? '延迟偏高' : '连接质量良好' }}</small>
      </div>
    </div>

    <div class="lgapp-chart">
      <div class="lgapp-chart__head">
        <strong>下行流量</strong>
        <span>{{ server.metrics.down.toFixed(1) }} MB/s</span>
      </div>
      <LgSparkline :values="series" :height="92" color="var(--lg-info)" label="网络下行趋势" />
    </div>

    <div class="lgapp-section">
      <h3>网卡</h3>
      <div v-if="server.interfaces.length === 0" class="lgapp-empty">
        <strong>暂无网卡信息</strong>
        <span>连接恢复后会重新采集网络接口。</span>
      </div>
      <div v-else class="lgapp-drive-grid">
        <article v-for="iface in server.interfaces" :key="iface.name" class="lgapp-drive">
          <div class="lgapp-drive__top">
            <span class="lgapp-drive__icon"><LgIcon name="network" :size="16" /></span>
            <div class="lgapp-drive__name">
              <strong>{{ iface.name }}</strong>
              <span>{{ iface.speed }}</span>
            </div>
            <LgStatusPill :label="iface.name === primary?.name ? '主用' : '回环'" :tone="iface.name === primary?.name ? 'online' : 'muted'" />
          </div>
          <div class="lgapp-info">
            <div class="lgapp-info__row"><span>IPv4</span><strong class="lgapp-mono">{{ iface.ip }}</strong></div>
            <div class="lgapp-info__row"><span>MAC</span><strong class="lgapp-mono">{{ iface.mac }}</strong></div>
          </div>
          <div class="lgapp-drive__foot">
            <span>下行 <strong>{{ iface.down.toFixed(1) }} MB/s</strong></span>
            <span>上行 <strong>{{ iface.up.toFixed(1) }} MB/s</strong></span>
          </div>
        </article>
      </div>
    </div>

    <div class="lgapp-section">
      <h3>路由与 DNS</h3>
      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>默认网关</span><strong class="lgapp-mono">{{ server.gateway }}</strong></div>
        <div class="lgapp-info__row"><span>DNS</span><strong class="lgapp-mono">{{ server.dns }}</strong></div>
        <div class="lgapp-info__row"><span>公网地址</span><strong class="lgapp-mono">{{ server.publicIp }}</strong></div>
        <div class="lgapp-info__row"><span>MTU</span><strong class="lgapp-mono">{{ server.mtu }}</strong></div>
        <div class="lgapp-info__row"><span>SSH 端口</span><strong class="lgapp-mono">{{ server.port }}</strong></div>
      </div>
    </div>

    <p class="lgapp-note">
      <LgIcon name="info" :size="13" />
      网络信息来自只读的 ip addr、ip route 与 ss 命令。关闭面板不会中断 SSH 会话。
    </p>
  </div>
</template>
