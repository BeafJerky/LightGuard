<script setup lang="ts">
import { computed, h, reactive, ref } from 'vue';
import {
  LgAlert,
  LgButton,
  LgCard,
  LgCheckbox,
  LgDataTable,
  LgDesktopEffects,
  LgDivider,
  LgDropdown,
  LgForm,
  LgFormItem,
  LgIcon,
  LgInput,
  LgInputNumber,
  LgKpi,
  LgModal,
  LgMotion,
  LgPopconfirm,
  LgPopover,
  LgProgress,
  LgRadio,
  LgRadioGroup,
  LgSelect,
  LgSparkline,
  LgStatusPill,
  LgSurface,
  LgSwitch,
  LgTabs,
  LgTabPane,
  LgTag,
  LgTerminal,
  LgTooltip,
  LgVirtualTable,
  themeOptions,
  useLgDialog,
  useLgMessage,
} from '@lightguard/lightui';
import RenderSettingsPanel from '../render/RenderSettingsPanel.vue';
import { renderSettings } from '../render/settings';
import DesktopIconLayer from '../desktop/DesktopIconLayer.vue';
import type {
  DataTableColumns,
  DropdownOption,
  FormRules,
  SelectOption,
  ThemeId,
  ThemeScope,
} from '@lightguard/lightui';

const props = withDefaults(
  defineProps<{
    themeId: ThemeId;
    scope: ThemeScope;
    compact?: boolean;
  }>(),
  { compact: false },
);

const emit = defineEmits<{
  'select-theme': [id: ThemeId];
}>();

const message = useLgMessage();
const dialog = useLgDialog();

const currentOption = computed(
  () => themeOptions.find((option) => option.id === props.themeId) ?? themeOptions[0],
);

const packLabels: Record<string, string> = {
  default: '基础',
  claude: '纸墨',
  pixel: '像素',
  nes: '红白机',
  tech: '科技',
};

const statusLabel = computed(() =>
  currentOption.value.mode === 'system' ? '跟随系统' : packLabels[currentOption.value.pack] ?? currentOption.value.pack,
);

/* ---------- 主题切换 ---------- */
function pickTheme(id: ThemeId) {
  emit('select-theme', id);
}

/* ---------- KPI / 图表 ---------- */
const cpuTrend = [18, 24, 21, 33, 29, 42, 38, 51, 47, 44, 39, 46, 52, 48, 57, 61];
const memTrend = [41, 43, 42, 44, 46, 45, 48, 47, 49, 52, 51, 53, 52, 55, 54, 56];
const diskTrend = [62, 62, 62, 63, 63, 63, 63, 63, 64, 64, 64, 64, 64, 64, 64, 64];

/* ---------- 表单 ---------- */
const formModel = reactive({
  name: 'web-prod-01',
  host: '10.0.14.23',
  port: 22,
  auth: 'key',
  remember: true,
});

const formRules: FormRules = {
  name: { required: true, message: '请填写服务器名称', trigger: ['blur', 'input'] },
  host: { required: true, message: '请填写主机地址', trigger: ['blur', 'input'] },
};

type FormApi = { validate: () => Promise<unknown>; restoreValidation: () => void };
const formRef = ref<FormApi | null>(null);

async function submitForm() {
  try {
    await formRef.value?.validate();
    message.success(`已校验 ${formModel.name} → ${formModel.host}:${formModel.port}`);
  } catch {
    message.error('表单尚未通过校验，弹层仍应落在当前作用域');
  }
}

function resetForm() {
  formRef.value?.restoreValidation();
  message.info('已清除校验状态');
}

/* ---------- 表格 ---------- */
interface ProcRow {
  key: string;
  name: string;
  pid: number;
  cpu: number;
  mem: number;
  state: 'running' | 'sleep' | 'stopped';
}

const procRows: ProcRow[] = [
  { key: '1', name: 'lightguard-agent', pid: 1042, cpu: 3.4, mem: 128, state: 'running' },
  { key: '2', name: 'nginx', pid: 2188, cpu: 1.2, mem: 64, state: 'running' },
  { key: '3', name: 'postgres', pid: 3301, cpu: 6.8, mem: 512, state: 'running' },
  { key: '4', name: 'systemd-journald', pid: 612, cpu: 0.3, mem: 42, state: 'sleep' },
  { key: '5', name: 'certbot.timer', pid: 4410, cpu: 0, mem: 12, state: 'stopped' },
];

const stateLabel: Record<ProcRow['state'], string> = {
  running: '运行中',
  sleep: '休眠',
  stopped: '已停止',
};

const columns: DataTableColumns<ProcRow> = [
  { title: '进程', key: 'name', minWidth: 170 },
  { title: 'PID', key: 'pid', width: 88 },
  {
    title: 'CPU',
    key: 'cpu',
    width: 92,
    render: (row) => h('span', { class: 'mono' }, `${row.cpu.toFixed(1)}%`),
  },
  {
    title: '内存',
    key: 'mem',
    width: 100,
    render: (row) => h('span', { class: 'mono' }, `${row.mem} MB`),
  },
  {
    title: '状态',
    key: 'state',
    width: 104,
    render: (row) =>
      h(
        LgTag,
        { size: 'small', type: row.state === 'running' ? 'success' : 'default', bordered: false },
        { default: () => stateLabel[row.state] },
      ),
  },
];

/* ---------- 虚拟表格 ---------- */
const virtualRowCount = ref(2000);
const virtualSelection = ref('尚未选择行');

const virtualColumns = [
  { key: 'time', title: '时间', width: 1 },
  { key: 'name', title: '进程', width: 2 },
  { key: 'pid', title: 'PID', width: 0.8, align: 'right' as const },
  { key: 'cpu', title: 'CPU', width: 0.8, align: 'right' as const },
  { key: 'mem', title: '内存', width: 0.9, align: 'right' as const },
  { key: 'threads', title: '线程', width: 0.7, align: 'right' as const },
  { key: 'state', title: '状态', width: 0.9 },
];

const processNames = [
  'lightguard-agent', 'nginx', 'postgres', 'redis-server', 'node',
  'cargo', 'dockerd', 'sshd', 'systemd-journald', 'certbot.timer',
];
const processStates = ['运行中', '运行中', '休眠', '运行中', '已停止'];

function syntheticProcessLog(count: number) {
  const rows: Record<string, string | number>[] = [];
  for (let index = 0; index < count; index += 1) {
    const seconds = (index * 13) % 86400;
    const hh = String(Math.floor(seconds / 3600)).padStart(2, '0');
    const mm = String(Math.floor((seconds % 3600) / 60)).padStart(2, '0');
    const ss = String(seconds % 60).padStart(2, '0');
    rows.push({
      time: `${hh}:${mm}:${ss}`,
      name: processNames[(index * 7) % processNames.length],
      pid: 1000 + ((index * 97) % 60000),
      cpu: `${(((index * 37) % 900) / 10).toFixed(1)}%`,
      mem: `${12 + ((index * 53) % 900)} MB`,
      threads: 2 + ((index * 11) % 48),
      state: processStates[index % processStates.length],
    });
  }
  return rows;
}

const virtualRows = computed(() => syntheticProcessLog(virtualRowCount.value));

function onVirtualSelect(index: number, row: Record<string, string | number>) {
  virtualSelection.value = `第 ${index + 1} 行 · ${row.name} · PID ${row.pid} · ${row.state}`;
}

/* ---------- 弹层 ---------- */
const modalOpen = ref(false);
const selected = ref<string | null>('key');
const authOptions: SelectOption[] = [
  { label: 'SSH 密钥', value: 'key' },
  { label: '密码', value: 'password' },
  { label: 'SSH Agent', value: 'agent' },
];

const dropdownOptions: DropdownOption[] = [
  { label: '复制连接命令', key: 'copy', icon: () => h(LgIcon, { name: 'terminal', size: 16 }) },
  { label: '打开终端', key: 'terminal', icon: () => h(LgIcon, { name: 'terminal', size: 16 }) },
  { type: 'divider', key: 'd1' },
  { label: '断开连接', key: 'disconnect', icon: () => h(LgIcon, { name: 'power', size: 16 }) },
];

function handleDropdown(key: string) {
  message.info(`菜单项：${key}`);
}

function openDialog() {
  dialog.warning({
    title: '确认断开？',
    content: '该操作会关闭当前会话，弹层应继承桌面作用域的主题。',
    positiveText: '断开',
    negativeText: '取消',
    onPositiveClick: () => message.success('已模拟断开'),
  });
}

/* ---------- 动效 ---------- */
const motionRound = ref(0);
const motionPresets = [
  { preset: 'fade-up' as const, title: 'fade-up', note: '列表项入场：spring(420, 34, 0.7)' },
  { preset: 'pop' as const, title: 'pop', note: '卡片出现：spring(500, 30, 0.6)' },
  { preset: 'slide-left' as const, title: 'slide-left', note: '抽屉 / 面板：spring(380, 36, 0.8)' },
];

/* ---------- 桌面 ---------- */
const activeTab = ref('overview');
const desktopIcons = [
  { name: 'monitor', label: '此电脑' },
  { name: 'terminal', label: '终端' },
  { name: 'folder', label: '文件' },
  { name: 'globe', label: '网络' },
  { name: 'settings', label: '设置' },
  { name: 'power', label: '电源' },
];
</script>

<template>
  <div class="lab-content" :class="{ 'lab-content--compact': compact }">
    <header class="lab-head">
      <div class="lab-head__title">
        <span class="lab-head__scope">{{ scope === 'console' ? '控制台' : '服务器桌面' }}</span>
        <strong>{{ currentOption.label }}</strong>
        <span class="lab-head__desc">{{ currentOption.description }}</span>
      </div>
      <LgStatusPill :label="statusLabel" tone="info" />
    </header>

    <div class="theme-switcher" role="group" aria-label="切换当前作用域主题">
      <button
        v-for="option in themeOptions"
        :key="option.id"
        type="button"
        class="theme-chip"
        :class="{ 'is-active': themeId === option.id }"
        :aria-pressed="themeId === option.id"
        :title="option.description"
        @click="pickTheme(option.id)"
      >
        <span class="theme-chip__dot" :data-pack="option.pack" />
        {{ option.label }}
      </button>
    </div>

    <!-- ============ 控制台 ============ -->
    <template v-if="!compact">
      <section class="kpi-row">
        <LgSurface tone="solid" padding="md">
          <LgKpi label="CPU 负载" value="38.2" unit="%" hint="8 核 · 近 5 分钟" tone="primary">
            <template #footer>
              <LgSparkline :values="cpuTrend" :height="44" label="CPU 负载趋势" />
            </template>
          </LgKpi>
        </LgSurface>
        <LgSurface tone="solid" padding="md">
          <LgKpi label="内存占用" value="5.4" unit="GB / 16 GB" hint="缓存 1.2 GB">
            <template #footer>
              <LgSparkline :values="memTrend" :height="44" color="var(--lg-accent)" label="内存趋势" />
            </template>
          </LgKpi>
        </LgSurface>
        <LgSurface tone="solid" padding="md">
          <LgKpi label="磁盘使用" value="64" unit="%" hint="根分区 128 GB" tone="warning">
            <template #footer>
              <LgSparkline :values="diskTrend" :height="44" color="var(--lg-warning)" label="磁盘趋势" />
            </template>
          </LgKpi>
        </LgSurface>
      </section>

      <div class="lab-grid">
        <LgSurface tone="solid" padding="lg" class="panel">
          <div class="panel-head">
            <h2><LgIcon name="terminal" :size="16" /> 基础控件</h2>
            <LgTag size="small" :bordered="false" type="success">Naive 封装</LgTag>
          </div>

          <div class="control-row">
            <LgButton type="primary">连接</LgButton>
            <LgButton>次要操作</LgButton>
            <LgButton tertiary>文字按钮</LgButton>
            <LgButton type="error" secondary>危险</LgButton>
            <LgButton quaternary circle aria-label="设置">
              <template #icon><LgIcon name="settings" :size="16" /></template>
            </LgButton>
          </div>

          <div class="control-row">
            <LgButton type="primary" ghost>
              <template #icon><LgIcon name="bolt" :size="16" /></template>
              带图标
            </LgButton>
            <LgButton loading>加载中</LgButton>
            <LgButton disabled>禁用</LgButton>
            <LgTooltip>
              <template #trigger>
                <LgButton dashed>悬浮提示</LgButton>
              </template>
              弹层挂在当前作用域的 portal 上
            </LgTooltip>
          </div>

          <LgDivider />

          <div class="field-grid">
            <LgInput placeholder="搜索服务器 / IP / 标签" clearable>
              <template #prefix><LgIcon name="search" :size="16" /></template>
            </LgInput>
            <LgInputNumber :min="1" :max="65535" :default-value="22" />
          </div>

          <div class="field-grid">
            <LgSelect v-model:value="selected" :options="authOptions" placeholder="认证方式" />
            <LgDropdown :options="dropdownOptions" trigger="click" @select="handleDropdown">
              <LgButton block>更多操作</LgButton>
            </LgDropdown>
          </div>

          <div class="control-row control-row--wrap">
            <LgSwitch :default-value="true" />
            <LgCheckbox default-checked>记住此设备</LgCheckbox>
            <LgRadioGroup default-value="balanced">
              <LgRadio value="balanced">均衡</LgRadio>
              <LgRadio value="strict">严格</LgRadio>
            </LgRadioGroup>
          </div>

          <div class="progress-stack">
            <LgProgress type="line" :percentage="72" indicator-placement="inside" />
            <LgProgress type="line" :percentage="38" status="warning" :show-indicator="false" />
            <LgProgress type="line" :percentage="94" status="error" :show-indicator="false" />
          </div>

          <div class="control-row control-row--wrap">
            <LgTag type="success">在线</LgTag>
            <LgTag type="warning">维护中</LgTag>
            <LgTag type="error">离线</LgTag>
            <LgTag type="info">仅内网</LgTag>
            <LgTag :bordered="false">标签</LgTag>
          </div>

          <LgAlert type="info" title="主题桥接" :bordered="false">
            控件颜色来自同一份 token，Naive 组件与自有组件共用 --lg-* 变量。
          </LgAlert>
        </LgSurface>

        <LgSurface tone="solid" padding="lg" class="panel">
          <div class="panel-head">
            <h2><LgIcon name="key" :size="16" /> 表单与校验</h2>
            <LgTag size="small" :bordered="false">useLgMessage</LgTag>
          </div>

          <LgForm ref="formRef" :model="formModel" :rules="formRules" label-placement="top">
            <LgFormItem label="服务器名称" path="name">
              <LgInput v-model:value="formModel.name" placeholder="例如 web-prod-01" />
            </LgFormItem>
            <div class="field-grid">
              <LgFormItem label="主机" path="host">
                <LgInput v-model:value="formModel.host" placeholder="IP 或域名" />
              </LgFormItem>
              <LgFormItem label="端口" path="port">
                <LgInputNumber v-model:value="formModel.port" :min="1" :max="65535" />
              </LgFormItem>
            </div>
            <LgFormItem label="认证方式">
              <LgRadioGroup v-model:value="formModel.auth">
                <LgRadio value="key">密钥</LgRadio>
                <LgRadio value="password">密码</LgRadio>
              </LgRadioGroup>
            </LgFormItem>
            <LgFormItem>
              <LgCheckbox v-model:checked="formModel.remember">保存到钥匙串</LgCheckbox>
            </LgFormItem>
            <div class="control-row">
              <LgButton type="primary" @click="submitForm">校验并连接</LgButton>
              <LgButton @click="resetForm">重置</LgButton>
            </div>
          </LgForm>

          <LgDivider />

          <div class="panel-head panel-head--tight">
            <h2><LgIcon name="layers" :size="16" /> 弹层作用域</h2>
          </div>
          <div class="control-row control-row--wrap">
            <LgButton @click="modalOpen = true">打开 Modal</LgButton>
            <LgButton @click="openDialog">打开 Dialog</LgButton>
            <LgPopover trigger="click" placement="bottom-start">
              <template #trigger>
                <LgButton>Popover</LgButton>
              </template>
              Popover 的 DOM 在 portal 内，仍能读到本作用域的 --lg-* 变量。
            </LgPopover>
            <LgPopconfirm @positive-click="message.success('已模拟删除')">
              <template #trigger>
                <LgButton type="error" secondary>Popconfirm</LgButton>
              </template>
              确认删除这条连接记录？
            </LgPopconfirm>
          </div>
        </LgSurface>

        <LgSurface tone="solid" padding="lg" class="panel panel--wide">
          <div class="panel-head">
            <h2><LgIcon name="database" :size="16" /> 表格</h2>
            <span class="panel-note">LgDataTable 直接透传 Naive 列渲染</span>
          </div>
          <LgDataTable
            :columns="columns"
            :data="procRows"
            :bordered="false"
            :single-line="false"
            size="small"
            :pagination="false"
          />
        </LgSurface>

        <RenderSettingsPanel :table-rows="virtualRowCount" />

        <LgSurface tone="solid" padding="lg" class="panel panel--wide">
          <div class="panel-head">
            <h2><LgIcon name="database" :size="16" /> 虚拟表格</h2>
            <div class="virtual-head">
              <span class="panel-note">{{ virtualSelection }}</span>
              <LgRadioGroup v-model:value="virtualRowCount" size="small">
                <LgRadio :value="2000">2,000 行</LgRadio>
                <LgRadio :value="10000">10,000 行</LgRadio>
              </LgRadioGroup>
            </div>
          </div>
          <p class="panel-note">
            合成进程日志。auto 模式下 ≥2,000 行自动走 Canvas，强制 DOM 时使用窗口化渲染；滚动、点击、方向键 / PageUp / PageDown / Home / End 在两种后端下行为一致。
          </p>
          <LgVirtualTable
            :columns="virtualColumns"
            :rows="virtualRows"
            :height="360"
            aria-label="合成进程日志"
            @select="onVirtualSelect"
          />
        </LgSurface>

        <LgSurface tone="solid" padding="lg" class="panel panel--wide">
          <div class="panel-head">
            <h2><LgIcon name="terminal" :size="16" /> 终端</h2>
            <span class="panel-note">xterm.js · 本地模拟流 · 切换渲染器不重建会话</span>
          </div>
          <LgTerminal :height="340" label="web-prod-01 · 模拟 Shell" simulated />
        </LgSurface>

        <LgSurface tone="glass" padding="lg" class="panel">
          <div class="panel-head">
            <h2><LgIcon name="spark" :size="16" /> 动效</h2>
            <LgButton size="small" @click="motionRound += 1">重播</LgButton>
          </div>
          <div :key="motionRound" class="motion-stack">
            <LgMotion
              v-for="(item, index) in motionPresets"
              :key="item.title"
              :preset="item.preset"
              :delay="index * 70"
            >
              <div class="motion-card">
                <strong>{{ item.title }}</strong>
                <span>{{ item.note }}</span>
              </div>
            </LgMotion>
          </div>
          <p class="panel-note">
            prefers-reduced-motion 时自动退化为无位移的即时状态；像素 / 红白机主题的 CSS 过渡使用 steps() 阶梯缓动。
          </p>
        </LgSurface>
      </div>
    </template>

    <!-- ============ 服务器桌面 ============ -->
    <template v-else>
      <div class="desk">
        <div class="desk__wallpaper">
          <LgDesktopEffects :intensity="renderSettings.effects.intensity" />
          <DesktopIconLayer :items="desktopIcons" aria-label="服务器桌面图标" />
          <div class="desk__readout">
            <span class="desk__host">web-prod-01</span>
            <span class="desk__meta">Ubuntu 24.04 · 10.0.14.23 · 在线 12 天</span>
          </div>
        </div>

        <div class="win">
          <div class="win__bar">
            <div class="win__dots" aria-hidden="true"><i /><i /><i /></div>
            <span class="win__title">系统监控</span>
            <LgDropdown :options="dropdownOptions" trigger="click" @select="handleDropdown">
              <button type="button" class="win__menu" aria-label="窗口菜单">
                <LgIcon name="more" :size="16" />
              </button>
            </LgDropdown>
          </div>

          <div class="win__body">
            <LgTabs v-model:value="activeTab" type="segment" size="small">
              <LgTabPane name="overview" tab="概览">
                <div class="win__kpis">
                  <LgKpi label="CPU" value="38" unit="%" tone="primary" />
                  <LgKpi label="内存" value="5.4" unit="GB" />
                  <LgKpi label="磁盘" value="64" unit="%" tone="warning" />
                </div>
                <LgSparkline :values="cpuTrend" :height="52" label="CPU 趋势" />
                <LgProgress type="line" :percentage="64" :show-indicator="false" status="warning" />
              </LgTabPane>
              <LgTabPane name="process" tab="进程">
                <LgDataTable
                  :columns="columns"
                  :data="procRows.slice(0, 4)"
                  size="small"
                  :bordered="false"
                  :single-line="false"
                  :pagination="false"
                />
              </LgTabPane>
              <LgTabPane name="terminal" tab="终端">
                <LgTerminal :height="220" label="模拟 Shell" simulated />
              </LgTabPane>
            </LgTabs>
          </div>
        </div>

        <div class="desk__dock">
          <DesktopIconLayer :items="desktopIcons.slice(0, 4)" variant="dock" aria-label="程序坞" />
        </div>
      </div>

      <LgSurface tone="solid" padding="md" class="desk-controls">
        <div class="control-row control-row--wrap">
          <LgButton size="small" type="primary" @click="modalOpen = true">Modal</LgButton>
          <LgButton size="small" @click="openDialog">Dialog</LgButton>
          <LgPopconfirm @positive-click="message.success('已模拟重启')">
            <template #trigger>
              <LgButton size="small" type="warning" secondary>重启服务</LgButton>
            </template>
            确认重启 lightguard-agent？
          </LgPopconfirm>
          <LgSwitch :default-value="true" size="small" />
        </div>
        <p class="panel-note">桌面作用域的弹层与提示都不应染上控制台主题。</p>
      </LgSurface>
    </template>

    <LgModal v-model:show="modalOpen" title="连接详情" class="lg-lab-modal">
      <div class="modal-body">
        <p>这个 Modal 通过 LgModal 的 teleport 默认值挂到了当前作用域的 portal。</p>
        <LgInput :default-value="formModel.host" readonly>
          <template #prefix><LgIcon name="globe" :size="16" /></template>
        </LgInput>
        <div class="control-row">
          <LgTag type="success">密钥认证</LgTag>
          <LgTag type="info">仅内网</LgTag>
        </div>
      </div>
      <template #footer>
        <div class="control-row control-row--end">
          <LgButton @click="modalOpen = false">取消</LgButton>
          <LgButton type="primary" @click="modalOpen = false">保存</LgButton>
        </div>
      </template>
    </LgModal>
  </div>
</template>

<style scoped>
.lab-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
  color: var(--lg-text);
}

.lab-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.lab-head__title {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}

.lab-head__scope {
  padding: 2px 7px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.lab-head strong {
  font-family: var(--lg-font-display);
  font-size: var(--lg-fs-sub);
  letter-spacing: 0.01em;
}

.lab-head__desc {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
}

/* ---------- 主题选择 ---------- */
.theme-switcher {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.theme-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 10px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-full);
  background: transparent;
  color: var(--lg-text-secondary);
  font: inherit;
  font-size: var(--lg-fs-sm);
  cursor: pointer;
  transition:
    border-color var(--lg-dur-fast) var(--lg-ease),
    background-color var(--lg-dur-fast) var(--lg-ease),
    color var(--lg-dur-fast) var(--lg-ease);
}

.theme-chip:hover {
  border-color: var(--lg-border-strong);
  color: var(--lg-text);
}

.theme-chip:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.theme-chip.is-active {
  border-color: var(--lg-primary);
  background: var(--lg-primary-soft);
  color: var(--lg-primary);
}

.theme-chip__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--lg-primary);
}

.theme-chip__dot[data-pack='claude'] { background: #b45309; }
.theme-chip__dot[data-pack='pixel'] { border-radius: 1px; background: #f5b544; }
.theme-chip__dot[data-pack='nes'] { border-radius: 1px; background: #e60012; }
.theme-chip__dot[data-pack='tech'] { background: #00e5ff; box-shadow: 0 0 8px rgba(0, 229, 255, 0.6); }

/* ---------- KPI ---------- */
.kpi-row {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

/* ---------- 面板 ---------- */
.lab-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: 12px;
}

.panel--wide {
  grid-column: 1 / -1;
}

.panel {
  min-width: 0;
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 14px;
}

.panel-head--tight {
  margin-bottom: 10px;
}

.panel-head h2 {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  margin: 0;
  font-family: var(--lg-font-display);
  font-size: var(--lg-fs-body);
  font-weight: 600;
  letter-spacing: 0.01em;
}

.panel-head h2 :deep(.lg-icon) {
  color: var(--lg-primary);
}

.panel-note {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-sm);
  line-height: 1.6;
}

.virtual-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 14px;
}

.lab-grid .panel--wide > .panel-note {
  margin: 0 0 10px;
}

.control-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  min-width: 0;
}

.control-row:last-child {
  margin-bottom: 0;
}

.control-row--wrap {
  flex-wrap: wrap;
}

.control-row--end {
  justify-content: flex-end;
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-bottom: 12px;
}

.progress-stack {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 14px;
}

.motion-stack {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 12px;
}

.motion-card {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface-2);
}

.motion-card strong {
  font-size: var(--lg-fs-sm);
}

.motion-card span {
  color: var(--lg-text-muted);
  font-size: var(--lg-fs-label);
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

/* ---------- 桌面作用域 ---------- */
.desk {
  position: relative;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-height: 420px;
  border: var(--lg-border-width) solid var(--lg-border);
  border-radius: var(--lg-radius-lg);
  background-image: var(--lg-desktop-wallpaper);
  background-size: cover;
}

.desk__wallpaper {
  position: relative;
  flex: 1;
  min-height: 260px;
  padding: 14px;
}

.desk__readout {
  position: absolute;
  right: 14px;
  bottom: 12px;
  z-index: 1;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
  color: var(--lg-desktop-text, var(--lg-text));
  text-shadow: var(--lg-desktop-text-shadow, none);
}

.desk__host {
  font-family: var(--lg-font-mono);
  font-size: var(--lg-fs-sm);
}

.desk__meta {
  color: var(--lg-desktop-text-muted, var(--lg-text-muted));
  font-size: var(--lg-fs-label);
}

.win {
  margin: 0 12px 12px;
  overflow: hidden;
  border: var(--lg-border-width) solid var(--lg-border-strong);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface);
  box-shadow: var(--lg-shadow-lg);
}

.win__bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border-bottom: var(--lg-border-width) solid var(--lg-border);
  background: var(--lg-surface-2);
}

.win__dots {
  display: flex;
  gap: 4px;
}

.win__dots i {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--lg-border-strong);
}

.win__dots i:first-child { background: var(--lg-danger); }
.win__dots i:nth-child(2) { background: var(--lg-warning); }
.win__dots i:nth-child(3) { background: var(--lg-success); }

.win__title {
  flex: 1;
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
}

.win__menu {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: var(--lg-radius-sm);
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.win__menu:hover {
  background: var(--lg-hover, var(--lg-surface-3));
  color: var(--lg-text);
}

.win__body {
  padding: 12px;
}

.win__kpis {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-bottom: 12px;
}

.desk__dock {
  display: flex;
  justify-content: center;
  gap: 6px;
  padding: 8px;
  border-top: var(--lg-border-width) solid var(--lg-border);
  background: var(--lg-surface-glass);
  backdrop-filter: blur(var(--lg-panel-blur));
}

.desk-controls {
  margin-top: 12px;
}

@media (max-width: 1180px) {
  .lab-grid {
    grid-template-columns: minmax(0, 1fr);
  }

  .kpi-row {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 720px) {
  .kpi-row {
    grid-template-columns: minmax(0, 1fr);
  }

  .field-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
