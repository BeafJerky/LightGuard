<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgIcon, LgSparkline, LgStatusPill } from '@lightguard/lightui';
import { clamp, round } from '../data';
import { statusLabel, statusTone } from './apps';
import type { Server } from '../types';

const props = defineProps<{ server: Server }>();

type MetricKey = 'cpu' | 'mem' | 'net';

const metric = ref<MetricKey>('cpu');

const metrics: Array<{ key: MetricKey; label: string; icon: string }> = [
  { key: 'cpu', label: 'CPU', icon: 'cpu' },
  { key: 'mem', label: '内存', icon: 'memory' },
  { key: 'net', label: '网络', icon: 'activity' },
];

const downSeries = computed(() => props.server.history.net.slice(-120));

const upSeries = computed(() => {
  const ratio = props.server.metrics.down > 0
    ? props.server.metrics.up / props.server.metrics.down
    : 0.3;
  return downSeries.value.map((value, index) =>
    round(clamp(value * ratio * (0.86 + Math.sin(index * 0.9) * 0.14), 0, 400), 1),
  );
});

const series = computed<number[]>(() => {
  if (metric.value === 'cpu') return props.server.history.cpu.slice(-120);
  if (metric.value === 'mem') return props.server.history.mem.slice(-120);
  return downSeries.value;
});

const unit = computed(() => (metric.value === 'net' ? 'MB/s' : '%'));

const maxValue = computed(() => {
  if (metric.value === 'net') return undefined;
  return 100;
});

const color = computed(() => {
  if (metric.value === 'cpu') return 'var(--lg-primary)';
  if (metric.value === 'mem') return 'var(--lg-accent)';
  return 'var(--lg-info)';
});

const summary = computed(() => {
  const values = series.value;
  if (!values.length) return { current: 0, avg: 0, peak: 0, low: 0 };
  let sum = 0;
  let peak = values[0];
  let low = values[0];
  for (const value of values) {
    sum += value;
    if (value > peak) peak = value;
    if (value < low) low = value;
  }
  return {
    current: values[values.length - 1],
    avg: round(sum / values.length, 1),
    peak: round(peak, 1),
    low: round(low, 1),
  };
});

const coreCount = computed(() => Math.min(Math.max(props.server.cpuCores, 1), 16));

const cores = computed(() =>
  Array.from({ length: coreCount.value }, (_, index) =>
    Math.round(
      clamp(
        props.server.metrics.cpu +
          Math.sin(index * 1.37 + props.server.metrics.cpu * 0.11) * 13,
        1,
        99,
      ),
    ),
  ),
);

const topProcesses = computed(() => props.server.processes.slice(0, 5));

function barTone(value: number) {
  return value >= 90 ? 'is-danger' : value >= 80 ? 'is-warning' : '';
}

function formatValue(value: number) {
  return metric.value === 'net' ? value.toFixed(1) : Math.round(value).toString();
}
</script>

<template>
  <div class="lgapp">
    <div class="lgapp-head">
      <div>
        <h2>性能监视器</h2>
        <p>{{ server.name }} · 采样间隔 5 秒 · 最近 10 分钟</p>
      </div>
      <LgStatusPill :label="statusLabel(server.status)" :tone="statusTone(server.status)" />
    </div>

    <div class="lgapp-nav" role="tablist" aria-label="性能指标">
      <button
        v-for="item in metrics"
        :key="item.key"
        type="button"
        role="tab"
        :aria-selected="metric === item.key"
        :class="{ 'is-active': metric === item.key }"
        @click="metric = item.key"
      >
        <LgIcon :name="item.icon" :size="13" />
        {{ item.label }}
      </button>
    </div>

    <div class="lgapp-chart">
      <div class="lgapp-chart__head">
        <strong>{{ metrics.find((item) => item.key === metric)?.label }} 使用率</strong>
        <span>{{ summary.current.toFixed(metric === 'net' ? 1 : 0) }} {{ unit }}</span>
      </div>
      <LgSparkline
        :values="series"
        :height="104"
        :max="maxValue"
        :color="color"
        :label="`${metrics.find((item) => item.key === metric)?.label} 趋势`"
      />
    </div>

    <div class="lgapp-kpis">
      <div class="lgapp-kpi">
        <span>当前</span>
        <strong>{{ formatValue(summary.current) }} <small>{{ unit }}</small></strong>
        <small>{{ server.lastSync }}更新</small>
      </div>
      <div class="lgapp-kpi">
        <span>平均</span>
        <strong>{{ formatValue(summary.avg) }} <small>{{ unit }}</small></strong>
        <small>最近 120 个采样点</small>
      </div>
      <div class="lgapp-kpi">
        <span>峰值 / 低谷</span>
        <strong>{{ formatValue(summary.peak) }} / {{ formatValue(summary.low) }}</strong>
        <small>{{ unit }}</small>
      </div>
    </div>

    <template v-if="metric === 'cpu'">
      <div class="lgapp-section">
        <h3>逻辑核心{{ coreCount > 8 ? `（显示前 8 / 共 ${coreCount}）` : '' }}</h3>
        <div class="lgapp-core-grid">
          <div v-for="(value, index) in cores.slice(0, 8)" :key="index" class="lgapp-core">
            <span>Core {{ index }} <b>{{ value }}%</b></span>
            <div class="lgapp-bar">
              <i :class="barTone(value)" :style="{ width: `${value}%` }" />
            </div>
          </div>
        </div>
      </div>

      <div class="lgapp-section">
        <h3>占用最高的进程</h3>
        <div class="lgapp-table">
          <div class="lgapp-table__head">
            <span>PID</span><span>名称</span><span>用户</span><span>CPU</span><span>内存</span><span>状态</span>
          </div>
          <div v-for="proc in topProcesses" :key="proc.pid" class="lgapp-table__row">
            <span class="is-mono">{{ proc.pid }}</span>
            <span>{{ proc.name }}</span>
            <span>{{ proc.user }}</span>
            <span class="is-mono" :class="{ 'is-hot': proc.cpu > 20 }">{{ proc.cpu.toFixed(1) }}%</span>
            <span class="is-mono">{{ proc.mem.toFixed(1) }}%</span>
            <span>{{ proc.state }}</span>
          </div>
        </div>
      </div>
    </template>

    <template v-else-if="metric === 'mem'">
      <div class="lgapp-kpis">
        <div class="lgapp-kpi">
          <span>总内存</span>
          <strong>{{ server.memTotal }} GB</strong>
          <small>{{ server.memType }}</small>
        </div>
        <div class="lgapp-kpi">
          <span>已使用</span>
          <strong>{{ round((server.memTotal * server.metrics.mem) / 100, 1) }} GB</strong>
          <small>{{ Math.round(server.metrics.mem) }}% 占用</small>
        </div>
        <div class="lgapp-kpi">
          <span>可用</span>
          <strong>{{ round(server.memTotal * (1 - server.metrics.mem / 100), 1) }} GB</strong>
          <small>含缓存与缓冲区</small>
        </div>
      </div>

      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>交换分区</span><strong>4 GB · 使用 0 MB</strong></div>
        <div class="lgapp-info__row"><span>缓存 / 缓冲</span><strong>2.8 GB</strong></div>
        <div class="lgapp-info__row"><span>OOM 事件</span><strong>最近 7 天 0 次</strong></div>
      </div>
    </template>

    <template v-else>
      <div class="lgapp-section">
        <h3>实时吞吐</h3>
        <div class="lgapp-spec-grid is-three">
          <div class="lgapp-metric">
            <div class="lgapp-metric__top">
              <span>下行</span>
              <strong>{{ server.metrics.down.toFixed(1) }} MB/s</strong>
            </div>
            <div class="lgapp-bar">
              <i :style="{ width: `${clamp(server.metrics.down, 2, 100)}%` }" />
            </div>
          </div>
          <div class="lgapp-metric">
            <div class="lgapp-metric__top">
              <span>上行</span>
              <strong>{{ server.metrics.up.toFixed(1) }} MB/s</strong>
            </div>
            <div class="lgapp-bar">
              <i class="is-warning" :style="{ width: `${clamp(server.metrics.up * 2, 2, 100)}%` }" />
            </div>
          </div>
          <div class="lgapp-metric">
            <div class="lgapp-metric__top">
              <span>SSH 延迟</span>
              <strong>{{ server.latency }} ms</strong>
            </div>
            <div class="lgapp-bar">
              <i :class="server.latency > 80 ? 'is-warning' : ''" :style="{ width: `${clamp(server.latency, 4, 100)}%` }" />
            </div>
          </div>
        </div>
      </div>

      <div class="lgapp-chart">
        <div class="lgapp-chart__head">
          <strong>上行流量</strong>
          <span>{{ server.metrics.up.toFixed(1) }} MB/s</span>
        </div>
        <LgSparkline
          :values="upSeries"
          :height="80"
          color="var(--lg-warning)"
          label="上行流量趋势"
        />
      </div>

      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>网卡</span><strong>{{ server.interfaces[0]?.name ?? 'eth0' }} · {{ server.interfaces[0]?.speed ?? '1 Gbps' }}</strong></div>
        <div class="lgapp-info__row"><span>累计流量</span><strong>下行 1.42 TB · 上行 386 GB</strong></div>
      </div>
    </template>

    <p class="lgapp-note">
      <LgIcon name="info" :size="13" />
      性能数据由本地原型模拟。接入真实 SSH 后，采样命令为只读的 /proc 与 ss，不修改服务器状态。
    </p>
  </div>
</template>
