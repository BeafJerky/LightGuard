<script setup lang="ts">
import { computed, ref } from 'vue';
import { LgIcon, LgStatusPill } from '@lightguard/lightui';
import { clamp, round } from '../data';
import { statusLabel, statusTone } from './apps';
import { toast } from '../ui';
import type { Server } from '../types';

const props = defineProps<{ server: Server }>();

type TabKey = 'overview' | 'hardware' | 'system' | 'security';

const tab = ref<TabKey>('overview');

const tabs: Array<{ key: TabKey; label: string; icon: string }> = [
  { key: 'overview', label: '概览', icon: 'dashboard' },
  { key: 'hardware', label: '硬件', icon: 'cpu' },
  { key: 'system', label: '系统', icon: 'sliders' },
  { key: 'security', label: '安全', icon: 'shield' },
];

const cores = computed(() => {
  const count = Math.min(Math.max(props.server.cpuCores, 1), 8);
  return Array.from({ length: count }, (_, index) =>
    Math.round(clamp(props.server.metrics.cpu + Math.sin(index * 1.7 + 0.6) * 11, 1, 99)),
  );
});

function barTone(value: number) {
  return value >= 90 ? 'is-danger' : value >= 80 ? 'is-warning' : '';
}

function memoryUsed() {
  return round((props.server.memTotal * props.server.metrics.mem) / 100, 1);
}

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
    <div class="lgapp-nav" role="tablist" aria-label="系统信息分区">
      <button
        v-for="item in tabs"
        :key="item.key"
        type="button"
        role="tab"
        :aria-selected="tab === item.key"
        :class="{ 'is-active': tab === item.key }"
        @click="tab = item.key"
      >
        <LgIcon :name="item.icon" :size="13" />
        {{ item.label }}
      </button>
    </div>

    <template v-if="tab === 'overview'">
      <div class="lgapp-hero">
        <span class="lgapp-hero__avatar"><LgIcon name="server" :size="22" /></span>
        <div class="lgapp-hero__text">
          <h2>{{ server.name }}</h2>
          <p>{{ server.role }} · {{ server.os }}</p>
        </div>
        <LgStatusPill :label="statusLabel(server.status)" :tone="statusTone(server.status)" />
      </div>

      <div class="lgapp-section">
        <div class="lgapp-spec-grid">
          <div class="lgapp-spec">
            <span>处理器</span>
            <strong>{{ server.cpuCores }} vCPU</strong>
            <small>{{ server.cpuModel }}</small>
          </div>
          <div class="lgapp-spec">
            <span>内存</span>
            <strong>{{ server.memTotal }} GB</strong>
            <small>{{ server.memType }}</small>
          </div>
          <div class="lgapp-spec">
            <span>磁盘</span>
            <strong>{{ server.diskTotal }} GB</strong>
            <small>{{ server.disks.length }} 个挂载点</small>
          </div>
          <div class="lgapp-spec">
            <span>运行时长</span>
            <strong>{{ server.uptime }}</strong>
            <small>{{ server.kernel }}</small>
          </div>
        </div>
      </div>

      <div class="lgapp-section">
        <h3>实时负载</h3>
        <div class="lgapp-spec-grid is-three">
          <div class="lgapp-metric">
            <div class="lgapp-metric__top">
              <span>CPU</span>
              <strong>{{ Math.round(server.metrics.cpu) }}%</strong>
            </div>
            <div class="lgapp-bar">
              <i :class="barTone(server.metrics.cpu)" :style="{ width: `${server.metrics.cpu}%` }" />
            </div>
          </div>
          <div class="lgapp-metric">
            <div class="lgapp-metric__top">
              <span>内存</span>
              <strong>{{ Math.round(server.metrics.mem) }}%</strong>
            </div>
            <div class="lgapp-bar">
              <i :class="barTone(server.metrics.mem)" :style="{ width: `${server.metrics.mem}%` }" />
            </div>
          </div>
          <div class="lgapp-metric">
            <div class="lgapp-metric__top">
              <span>磁盘</span>
              <strong>{{ Math.round(server.metrics.disk) }}%</strong>
            </div>
            <div class="lgapp-bar">
              <i :class="barTone(server.metrics.disk)" :style="{ width: `${server.metrics.disk}%` }" />
            </div>
          </div>
        </div>
      </div>

      <div class="lgapp-section">
        <h3>网络</h3>
        <div class="lgapp-info">
          <div class="lgapp-info__row">
            <span>主机地址</span>
            <strong class="lgapp-mono">{{ server.ip }}:{{ server.port }}</strong>
          </div>
          <div class="lgapp-info__row">
            <span>下行 / 上行</span>
            <strong>{{ server.metrics.down.toFixed(1) }} MB/s · {{ server.metrics.up.toFixed(1) }} MB/s</strong>
          </div>
          <div class="lgapp-info__row">
            <span>SSH 延迟</span>
            <strong>{{ server.latency }} ms</strong>
          </div>
        </div>
      </div>
    </template>

    <template v-else-if="tab === 'hardware'">
      <div class="lgapp-head">
        <div>
          <h2>硬件</h2>
          <p>{{ server.cpuModel }} · {{ server.cpuCores }} vCPU</p>
        </div>
        <LgStatusPill :label="server.arch" tone="info" />
      </div>

      <div class="lgapp-section">
        <h3>处理器</h3>
        <div class="lgapp-spec-grid">
          <div class="lgapp-spec">
            <span>型号</span>
            <strong>{{ server.cpuModel }}</strong>
            <small>{{ server.cpuFreq }}</small>
          </div>
          <div class="lgapp-spec">
            <span>核心 / 线程</span>
            <strong>{{ server.cpuCores }} vCPU</strong>
            <small>当前负载 {{ server.metrics.cpu.toFixed(1) }}%</small>
          </div>
          <div class="lgapp-spec">
            <span>内存</span>
            <strong>{{ server.memTotal }} GB {{ server.memType }}</strong>
            <small>已用 {{ memoryUsed() }} GB</small>
          </div>
          <div class="lgapp-spec">
            <span>磁盘容量</span>
            <strong>{{ server.diskTotal }} GB</strong>
            <small>{{ server.disks.length }} 个挂载点</small>
          </div>
        </div>
      </div>

      <div class="lgapp-section">
        <h3>逻辑核心负载{{ server.cpuCores > 8 ? `（显示前 8 / 共 ${server.cpuCores}）` : '' }}</h3>
        <div class="lgapp-core-grid">
          <div v-for="(value, index) in cores" :key="index" class="lgapp-core">
            <span>Core {{ index }} <b>{{ value }}%</b></span>
            <div class="lgapp-bar">
              <i :class="barTone(value)" :style="{ width: `${value}%` }" />
            </div>
          </div>
        </div>
      </div>

      <div class="lgapp-section">
        <h3>温度与负载</h3>
        <div class="lgapp-kpis">
          <div class="lgapp-kpi">
            <span>CPU 温度</span>
            <strong>{{ server.temp }}°C</strong>
            <small>{{ server.temp >= 70 ? '偏高' : '正常范围' }}</small>
          </div>
          <div class="lgapp-kpi">
            <span>1 分钟负载</span>
            <strong>{{ server.load[0].toFixed(2) }}</strong>
            <small>5 分钟 {{ server.load[1].toFixed(2) }} · 15 分钟 {{ server.load[2].toFixed(2) }}</small>
          </div>
          <div class="lgapp-kpi">
            <span>内核</span>
            <strong>{{ server.kernel }}</strong>
            <small>{{ server.arch }}</small>
          </div>
        </div>
      </div>
    </template>

    <template v-else-if="tab === 'system'">
      <div class="lgapp-head">
        <div>
          <h2>系统</h2>
          <p>{{ server.os }}</p>
        </div>
        <LgStatusPill :label="server.timezone" tone="info" />
      </div>

      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>主机名</span><strong>{{ server.name }}</strong></div>
        <div class="lgapp-info__row"><span>操作系统</span><strong>{{ server.os }}</strong></div>
        <div class="lgapp-info__row"><span>内核版本</span><strong class="lgapp-mono">{{ server.kernel }}</strong></div>
        <div class="lgapp-info__row"><span>架构</span><strong>{{ server.arch }}</strong></div>
        <div class="lgapp-info__row"><span>时区</span><strong>{{ server.timezone }}</strong></div>
        <div class="lgapp-info__row"><span>启动时间</span><strong>{{ server.bootTime }}</strong></div>
        <div class="lgapp-info__row"><span>运行时长</span><strong>{{ server.uptime }}</strong></div>
        <div class="lgapp-info__row"><span>服务器侧依赖</span><strong>无 · SSH 直连</strong></div>
      </div>

      <p class="lgapp-note">
        <LgIcon name="info" :size="13" />
        系统信息通过只读 SSH 命令采集（uname、/proc、/etc/os-release 等），不会上传任何文件，也不会修改服务器配置。
      </p>
    </template>

    <template v-else>
      <div class="lgapp-head">
        <div>
          <h2>安全</h2>
          <p>主机密钥与会话加密状态</p>
        </div>
        <LgStatusPill label="已校验" tone="online" />
      </div>

      <div class="lgapp-info">
        <div class="lgapp-info__row"><span>登录用户</span><strong>{{ server.user }}</strong></div>
        <div class="lgapp-info__row"><span>认证方式</span><strong>{{ server.authLabel }}</strong></div>
        <div class="lgapp-info__row"><span>SSH 端口</span><strong class="lgapp-mono">{{ server.port }}</strong></div>
        <div class="lgapp-info__row"><span>加密算法</span><strong class="lgapp-mono">chacha20-poly1305@openssh.com</strong></div>
        <div class="lgapp-info__row"><span>主机密钥校验</span><strong>TOFU · 首次连接已确认</strong></div>
        <div class="lgapp-info__row"><span>上次同步</span><strong>{{ server.lastSync }}</strong></div>
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

      <p class="lgapp-note">
        <LgIcon name="lock" :size="13" />
        LightGuard 只读取系统信息。若主机指纹发生变化，连接会被阻断并要求人工确认，以防止中间人攻击。
      </p>
    </template>
  </div>
</template>
