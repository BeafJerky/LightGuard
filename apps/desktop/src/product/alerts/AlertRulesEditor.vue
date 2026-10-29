<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { LgIcon, LgSwitch } from '@lightguard/lightui';
import { normalizeAlertRules, normalizeAlertSettings } from './settings';
import type { AlertRules, AlertSettings } from '../types';

const props = defineProps<{
  settings: AlertSettings;
  servers: Array<{ id: string; name: string }>;
}>();

const emit = defineEmits<{ (event: 'update', value: AlertSettings): void }>();

const targetId = ref('');
const draft = reactive({
  rules: normalizeAlertRules(props.settings.rules),
  perServer: { ...props.settings.perServer } as AlertSettings['perServer'],
});

watch(
  () => props.settings,
  (value) => {
    draft.rules = normalizeAlertRules(value.rules);
    draft.perServer = { ...value.perServer };
  },
  { deep: true },
);

// 被删除的服务器不应继续占着“生效范围”。
watch(
  () => props.servers,
  (servers) => {
    if (targetId.value && !servers.some((server) => server.id === targetId.value)) {
      targetId.value = '';
    }
  },
  { deep: true },
);

const overridden = computed(() => Boolean(targetId.value && draft.perServer[targetId.value]));

// 每台服务器的编辑都以“全局 + 已有覆盖”的完整规则为起点，写回时保存整份规则；
// 因此一旦写入覆盖，这台服务器之后不再跟随全局改动，界面需要讲清楚这一点。
const rules = computed<AlertRules>(() =>
  targetId.value
    ? normalizeAlertRules(draft.perServer[targetId.value], draft.rules)
    : draft.rules,
);

function publish() {
  // 始终发出全新对象：父级 settings 与全局规则都可能是共享引用，不能就地改写。
  emit('update', normalizeAlertSettings({ rules: draft.rules, perServer: draft.perServer }));
}

function commit(next: AlertRules) {
  const normalized = normalizeAlertRules(next, draft.rules);
  if (targetId.value) {
    draft.perServer = { ...draft.perServer, [targetId.value]: normalized };
  } else {
    draft.rules = normalized;
  }
  publish();
}

function resetOverride() {
  if (!targetId.value) return;
  const next = { ...draft.perServer };
  delete next[targetId.value];
  draft.perServer = next;
  publish();
}

type PercentKey = 'cpu' | 'memory' | 'disk';

function setPercent(key: PercentKey, field: 'threshold' | 'sustainSeconds', raw: string) {
  const next: AlertRules = { ...rules.value };
  next[key] = { ...next[key], [field]: Number(raw) };
  commit(next);
}

function setLoad(field: 'multiplier' | 'sustainSeconds', raw: string) {
  commit({ ...rules.value, load: { ...rules.value.load, [field]: Number(raw) } });
}

function setGrace(raw: string) {
  commit({ ...rules.value, offline: { ...rules.value.offline, graceSeconds: Number(raw) } });
}

function setToggle(key: 'hostkey' | 'login', enabled: boolean) {
  commit({ ...rules.value, [key]: { enabled } });
}
</script>

<template>
  <div class="rules-editor">
    <div class="rules-editor__scope">
      <label for="rules-target">生效范围</label>
      <select id="rules-target" v-model="targetId" class="rules-editor__select">
        <option value="">全部服务器（全局）</option>
        <option v-for="server in servers" :key="server.id" :value="server.id">
          {{ server.name }}{{ draft.perServer[server.id] ? '（独立）' : '' }}
        </option>
      </select>
      <span v-if="overridden" class="rules-editor__flag">独立规则</span>
      <button v-if="overridden" type="button" class="rules-editor__reset" @click="resetOverride">
        恢复跟随全局
      </button>
      <p v-if="overridden" class="rules-editor__note" role="status">
        这台服务器使用独立规则，之后修改全局不再影响它；恢复跟随全局会丢弃这些独立值。
      </p>
      <p v-else-if="targetId" class="rules-editor__note">
        尚未覆盖：编辑任意字段会为该服务器复制一份独立规则，之后全局改动不再影响它。
      </p>
    </div>

    <div class="rules-editor__rows">
      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="cpu" :size="15" />CPU</span>
        <label class="rules-editor__field">
          阈值
          <input
            type="number"
            min="50"
            max="100"
            step="1"
            :value="rules.cpu.threshold"
            aria-label="CPU 阈值（%）"
            @change="($event) => setPercent('cpu', 'threshold', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">%</span>
        </label>
        <label class="rules-editor__field">
          持续
          <input
            type="number"
            min="60"
            max="3600"
            step="10"
            :value="rules.cpu.sustainSeconds"
            aria-label="CPU 持续秒数"
            @change="($event) => setPercent('cpu', 'sustainSeconds', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">秒</span>
        </label>
      </div>

      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="memory" :size="15" />内存</span>
        <label class="rules-editor__field">
          阈值
          <input
            type="number"
            min="50"
            max="100"
            step="1"
            :value="rules.memory.threshold"
            aria-label="内存阈值（%）"
            @change="($event) => setPercent('memory', 'threshold', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">%</span>
        </label>
        <label class="rules-editor__field">
          持续
          <input
            type="number"
            min="60"
            max="3600"
            step="10"
            :value="rules.memory.sustainSeconds"
            aria-label="内存持续秒数"
            @change="($event) => setPercent('memory', 'sustainSeconds', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">秒</span>
        </label>
      </div>

      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="disk" :size="15" />磁盘</span>
        <label class="rules-editor__field">
          阈值
          <input
            type="number"
            min="50"
            max="100"
            step="1"
            :value="rules.disk.threshold"
            aria-label="磁盘阈值（%）"
            @change="($event) => setPercent('disk', 'threshold', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">%</span>
        </label>
        <label class="rules-editor__field">
          持续
          <input
            type="number"
            min="60"
            max="3600"
            step="10"
            :value="rules.disk.sustainSeconds"
            aria-label="磁盘持续秒数"
            @change="($event) => setPercent('disk', 'sustainSeconds', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">秒</span>
        </label>
      </div>

      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="activity" :size="15" />负载</span>
        <label class="rules-editor__field">
          倍数
          <input
            type="number"
            min="1"
            max="10"
            step="0.1"
            :value="rules.load.multiplier"
            aria-label="负载倍数"
            @change="($event) => setLoad('multiplier', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">× 核数</span>
        </label>
        <label class="rules-editor__field">
          持续
          <input
            type="number"
            min="60"
            max="3600"
            step="10"
            :value="rules.load.sustainSeconds"
            aria-label="负载持续秒数"
            @change="($event) => setLoad('sustainSeconds', ($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">秒</span>
        </label>
      </div>

      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="wifi" :size="15" />离线宽限</span>
        <label class="rules-editor__field">
          连续断开
          <input
            type="number"
            min="30"
            max="300"
            step="10"
            :value="rules.offline.graceSeconds"
            aria-label="离线宽限秒数"
            @change="setGrace(($event.target as HTMLInputElement).value)"
          />
          <span class="rules-editor__unit">秒</span>
        </label>
      </div>

      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="key" :size="15" />主机指纹变更</span>
        <span class="rules-editor__toggle">
          <LgSwitch
            :value="rules.hostkey.enabled"
            aria-label="主机指纹变更提醒"
            @update:value="(value: boolean) => setToggle('hostkey', value)"
          />
        </span>
      </div>

      <div class="rules-editor__row">
        <span class="rules-editor__name"><LgIcon name="users" :size="15" />登录异常</span>
        <span class="rules-editor__hint">最近日志中失败 5 次</span>
        <span class="rules-editor__toggle">
          <LgSwitch
            :value="rules.login.enabled"
            aria-label="登录异常提醒"
            @update:value="(value: boolean) => setToggle('login', value)"
          />
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.rules-editor {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.rules-editor__scope {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}

.rules-editor__scope label {
  color: var(--lg-text-muted);
  font-size: 11px;
}

.rules-editor__select {
  padding: 5px 8px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text);
  font-size: 11.5px;
}

.rules-editor__select:focus-visible,
.rules-editor__reset:focus-visible,
.rules-editor__field input:focus-visible {
  outline: 2px solid var(--lg-focus);
  outline-offset: 2px;
}

.rules-editor__flag {
  padding: 2px 7px;
  border-radius: var(--lg-radius-full);
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
  font-size: 10px;
}

.rules-editor__reset {
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--lg-primary);
  font-size: 11px;
  cursor: pointer;
}

.rules-editor__note {
  flex: 0 0 100%;
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 10.5px;
  line-height: 1.5;
}

.rules-editor__rows {
  display: flex;
  flex-direction: column;
}

.rules-editor__row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 9px 0;
  border-top: 1px solid var(--lg-border);
}

.rules-editor__name {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 118px;
  font-size: 12px;
  font-weight: 600;
}

.rules-editor__field {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--lg-text-muted);
  font-size: 11px;
}

.rules-editor__field input {
  width: 72px;
  padding: 4px 6px;
  border: 1px solid var(--lg-border);
  border-radius: 6px;
  background: var(--lg-bg-soft);
  color: var(--lg-text);
  font-size: 11.5px;
}

.rules-editor__unit {
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.rules-editor__hint {
  margin-left: auto;
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.rules-editor__toggle {
  display: inline-flex;
  margin-left: auto;
}

@media (max-width: 640px) {
  .rules-editor__name {
    width: 100%;
    min-width: 0;
  }

  .rules-editor__hint {
    margin-left: 0;
  }
}
</style>
