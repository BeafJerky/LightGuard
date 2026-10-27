<script setup lang="ts">
import { computed, ref } from 'vue';
import {
  LgButton,
  LgCheckbox,
  LgIcon,
  LgInput,
  LgModal,
  LgSwitch,
  LgTag,
} from '@lightguard/lightui';
import {
  BackupError,
  backupConnectionKey,
  createBackup,
  decryptBackup,
  downloadBackup,
  getPreview,
  readBackupFile,
  summaryFromBackup,
  validatePayload,
  type BackupFileInfo,
  type BackupPreview,
} from './backup';
import { applyBackup, setBackupSummary, state } from './store';
import { confirm, toast } from './ui';

const includeSettings = ref(true);
const includeConnections = ref(true);
const encrypt = ref(true);
const password = ref('');
const confirmPassword = ref('');
const exporting = ref(false);
const reading = ref(false);
const decrypting = ref(false);

const fileInput = ref<HTMLInputElement | null>(null);
const passwordOpen = ref(false);
const passwordError = ref('');
const decryptPassword = ref('');
const pendingFile = ref<BackupFileInfo | null>(null);

const previewOpen = ref(false);
const preview = ref<BackupPreview | null>(null);
const applySettings = ref(true);
const applyConnections = ref(true);
const strategy = ref<'merge' | 'replace'>('merge');
const applying = ref(false);

const summary = computed(() => state.backup);
const summaryText = computed(() => {
  if (!summary.value) return '尚未备份';
  const date = new Date(summary.value.at);
  const formatted = Number.isNaN(date.getTime()) ? summary.value.at : date.toLocaleString('zh-CN', { hour12: false });
  return `${formatted} · ${summary.value.hosts} 台连接`;
});

const errorText = ref('');

function pickFile() {
  fileInput.value?.click();
}

function handleError(error: unknown) {
  if (error instanceof BackupError) return error.message;
  return '操作失败，请重试。';
}

async function toggleEncryption(value: boolean) {
  if (value) {
    encrypt.value = true;
    return;
  }
  const ok = await confirm({
    title: '关闭备份加密',
    message:
      '未加密的 .lgbackup 可以被任何人直接读取。连接元数据不含口令，但仍可能暴露主机地址、用户名与分组信息。',
    confirmLabel: '仍然关闭',
    tone: 'danger',
  });
  encrypt.value = !ok;
}

async function onExport() {
  errorText.value = '';
  if (!includeSettings.value && !includeConnections.value) {
    errorText.value = '请至少选择一项备份范围';
    return;
  }
  if (encrypt.value) {
    if (password.value.length < 8) {
      errorText.value = '加密口令至少需要 8 个字符';
      return;
    }
    if (password.value !== confirmPassword.value) {
      errorText.value = '两次输入的口令不一致';
      return;
    }
  }
  exporting.value = true;
  try {
    const result = await createBackup({
      settings: state.settings,
      themes: state.themes,
      servers: state.servers,
      includeSettings: includeSettings.value,
      includeConnections: includeConnections.value,
      encrypt: encrypt.value,
      password: password.value,
    });
    downloadBackup(result.fileName, result.json);
    setBackupSummary(summaryFromBackup(result.hosts, result.encrypted));
    password.value = '';
    confirmPassword.value = '';
    toast({
      title: '备份已导出',
      message: `${result.hosts} 台连接 · ${result.encrypted ? '已加密' : '未加密'}`,
      tone: result.encrypted ? 'success' : 'warning',
    });
  } catch (error) {
    errorText.value = handleError(error);
  } finally {
    exporting.value = false;
  }
}

async function onFileChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = '';
  if (!file) return;
  errorText.value = '';
  reading.value = true;
  try {
    const info = await readBackupFile(file);
    if (info.encrypted) {
      pendingFile.value = info;
      decryptPassword.value = '';
      passwordError.value = '';
      passwordOpen.value = true;
      return;
    }
    const result = validatePayload(info.envelope.payload);
    openPreview(result.payload, result.warnings, false, info.createdAt);
  } catch (error) {
    errorText.value = handleError(error);
  } finally {
    reading.value = false;
  }
}

function openPreview(
  payload: BackupPreview['payload'],
  warnings: string[],
  encrypted: boolean,
  createdAt: string,
) {
  preview.value = getPreview(payload, warnings, encrypted, createdAt, state.servers);
  applySettings.value = payload.scope.settings;
  applyConnections.value = payload.scope.connections;
  strategy.value = 'merge';
  previewOpen.value = true;
}

async function onDecrypt() {
  if (!pendingFile.value) return;
  passwordError.value = '';
  if (!decryptPassword.value) {
    passwordError.value = '请输入备份口令';
    return;
  }
  decrypting.value = true;
  try {
    const result = await decryptBackup(pendingFile.value.envelope, decryptPassword.value);
    passwordOpen.value = false;
    openPreview(result.payload, result.warnings, true, pendingFile.value.createdAt);
  } catch (error) {
    passwordError.value = handleError(error);
  } finally {
    decrypting.value = false;
  }
}

const formatDate = (iso: string) => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString('zh-CN', { hour12: false });
};

async function applyImport() {
  const current = preview.value;
  if (!current) return;
  const scope = {
    settings: applySettings.value && current.payload.scope.settings,
    connections: applyConnections.value && current.payload.scope.connections,
  };
  if (!scope.settings && !scope.connections) {
    toast({ title: '请至少选择一项恢复范围', tone: 'warning' });
    return;
  }
  if (strategy.value === 'replace') {
    const parts: string[] = [];
    if (scope.connections) parts.push(`当前 ${state.servers.length} 台连接`);
    if (scope.settings) parts.push('全部设置与主题');
    const ok = await confirm({
      title: '替换当前配置',
      message: `将用备份替换${parts.join('与')}，当前配置不会保留。此操作不可撤销。`,
      confirmLabel: '替换',
      tone: 'danger',
    });
    if (!ok) return;
  }
  applying.value = true;
  try {
    const result = await applyBackup(current.payload, strategy.value, scope);
    previewOpen.value = false;
    toast({
      title: '恢复完成',
      message: `新增 ${result.added} · 更新 ${result.updated} · 跳过 ${result.skipped}`,
      tone: 'success',
      duration: 4200,
    });
    if (result.added > 0) {
      toast({
        title: '新连接需重新认证',
        message: '凭据未包含在备份中，请在总览中更新凭据后再连接。',
        tone: 'info',
        duration: 5200,
      });
    }
  } catch (error) {
    toast({ title: '恢复未完成', message: handleError(error), tone: 'error' });
  } finally {
    applying.value = false;
  }
}

function closePreview() {
  previewOpen.value = false;
  preview.value = null;
}
</script>

<template>
  <div class="backup">
    <div class="backup-summary" :class="{ 'is-empty': !summary }">
      <div class="backup-summary__icon">
        <LgIcon name="database" :size="18" />
      </div>
      <div class="backup-summary__body">
        <strong>{{ summary ? '最近一次备份' : '尚未备份' }}</strong>
        <span>{{ summaryText }}</span>
      </div>
      <LgTag
        v-if="summary"
        size="small"
        :bordered="false"
        :type="summary.encrypted ? 'success' : 'warning'"
      >
        {{ summary.encrypted ? '已加密' : '未加密' }}
      </LgTag>
    </div>

    <div class="backup-block">
      <h3>导出范围</h3>
      <div class="backup-scopes">
        <LgCheckbox v-model:checked="includeSettings">
          应用设置
          <span class="backup-muted">主题、刷新、安全与通知</span>
        </LgCheckbox>
        <LgCheckbox v-model:checked="includeConnections">
          SSH 连接
          <span class="backup-muted">名称、地址、用户与分组</span>
        </LgCheckbox>
      </div>

      <div class="backup-row">
        <div class="backup-row__text">
          <strong>包含凭据</strong>
          <span>凭据不会写入配置文件备份；导入后需重新输入或重新认证。</span>
        </div>
        <div class="backup-row__control">
          <LgSwitch :value="false" disabled aria-label="包含凭据（不可用）" />
          <LgTag size="small" :bordered="false" type="warning">不可用</LgTag>
        </div>
      </div>

      <div class="backup-row">
        <div class="backup-row__text">
          <strong>使用口令加密</strong>
          <span>AES-256-GCM + PBKDF2-SHA256（600,000 次迭代）。关闭后文件标记为未加密。</span>
        </div>
        <LgSwitch :value="encrypt" aria-label="使用口令加密" @update:value="toggleEncryption" />
      </div>

      <div v-if="encrypt" class="backup-passwords">
        <label class="backup-field">
          <span>备份口令</span>
          <LgInput v-model:value="password" type="password" show-password-on="click" placeholder="至少 8 个字符" />
        </label>
        <label class="backup-field">
          <span>确认口令</span>
          <LgInput v-model:value="confirmPassword" type="password" show-password-on="click" placeholder="再次输入" />
        </label>
      </div>
      <p v-else class="backup-warning">
        <LgIcon name="alert" :size="14" />
        当前备份将不加密，文件中不包含口令，但请妥善保管。
      </p>

      <p v-if="errorText" class="backup-error">
        <LgIcon name="alert" :size="14" />
        {{ errorText }}
      </p>

      <div class="backup-actions">
        <LgButton type="primary" size="small" :loading="exporting" @click="onExport">
          <template #icon><LgIcon name="arrow-down" :size="14" /></template>
          导出 .lgbackup
        </LgButton>
        <LgButton size="small" :loading="reading" @click="pickFile">
          <template #icon><LgIcon name="arrow-up" :size="14" /></template>
          导入备份
        </LgButton>
        <span class="backup-hint">文件上限 5MB · 最多 200 台连接</span>
        <input
          ref="fileInput"
          class="backup-file"
          type="file"
          accept=".lgbackup,application/json,application/octet-stream"
          @change="onFileChange"
        />
      </div>
    </div>

    <p class="backup-note">
      <LgIcon name="shield" :size="14" />
      备份只包含设置与连接元数据，不含口令、私钥或终端内容；导入的连接会标记为“需重新认证”。
    </p>

    <LgModal
      :show="passwordOpen"
      preset="card"
      class="backup-modal"
      :bordered="false"
      :mask-closable="false"
      title="输入备份口令"
      @update:show="(value: boolean) => !value && (passwordOpen = false)"
    >
      <div class="backup-modal__body">
        <p class="backup-modal__hint">
          {{ pendingFile ? formatDate(pendingFile.createdAt) : '' }} 导出的加密备份。口令只用于本次解密，不会保存。
        </p>
        <label class="backup-field">
          <span>备份口令</span>
          <LgInput
            v-model:value="decryptPassword"
            type="password"
            show-password-on="click"
            placeholder="输入导出时设置的口令"
            :status="passwordError ? 'error' : undefined"
            @keyup.enter="onDecrypt"
          />
          <span v-if="passwordError" class="backup-error">{{ passwordError }}</span>
        </label>
      </div>
      <template #footer>
        <div class="backup-modal__actions">
          <LgButton size="small" @click="passwordOpen = false">取消</LgButton>
          <LgButton size="small" type="primary" :loading="decrypting" @click="onDecrypt">
            解密并预览
          </LgButton>
        </div>
      </template>
    </LgModal>

    <LgModal
      :show="previewOpen"
      preset="card"
      class="backup-modal backup-modal--wide"
      :bordered="false"
      :mask-closable="false"
      title="导入预览"
      @update:show="(value: boolean) => !value && closePreview()"
    >
      <div v-if="preview" class="backup-preview">
        <div class="backup-preview__meta">
          <div><span>备份时间</span><strong>{{ formatDate(preview.createdAt) }}</strong></div>
          <div><span>应用版本</span><strong>{{ preview.payload.appVersion }}</strong></div>
          <div><span>连接数</span><strong>{{ preview.payload.connections.length }}</strong></div>
          <div><span>设置项</span><strong>{{ preview.settingsCount }}</strong></div>
          <div><span>重复连接</span><strong>{{ preview.duplicateCount }}</strong></div>
          <div>
            <span>文件状态</span>
            <strong>{{ preview.encrypted ? '已加密' : '未加密' }}</strong>
          </div>
        </div>

        <div v-if="preview.duplicateCount > 0" class="backup-preview__duplicates">
          <span>重复项</span>
          <ul>
            <li v-for="item in preview.duplicates" :key="item.id">
              {{ backupConnectionKey(item) }}
            </li>
            <li v-if="preview.duplicateCount > preview.duplicates.length">
              还有 {{ preview.duplicateCount - preview.duplicates.length }} 项
            </li>
          </ul>
        </div>

        <p v-if="preview.warnings.length" class="backup-warning">
          <LgIcon name="alert" :size="14" />
          {{ preview.warnings.join('；') }}
        </p>

        <div class="backup-preview__scopes">
          <span>恢复范围</span>
          <LgCheckbox
            v-model:checked="applySettings"
            :disabled="!preview.payload.scope.settings"
          >
            应用设置
            <span class="backup-muted">{{ preview.payload.scope.settings ? '可用' : '备份未包含' }}</span>
          </LgCheckbox>
          <LgCheckbox
            v-model:checked="applyConnections"
            :disabled="!preview.payload.scope.connections"
          >
            SSH 连接
            <span class="backup-muted">{{ preview.payload.scope.connections ? `${preview.payload.connections.length} 台` : '备份未包含' }}</span>
          </LgCheckbox>
        </div>

        <div class="backup-strategy" role="radiogroup" aria-label="恢复策略">
          <button
            type="button"
            role="radio"
            :aria-checked="strategy === 'merge'"
            class="backup-strategy__option"
            :class="{ 'is-active': strategy === 'merge' }"
            @click="strategy = 'merge'"
          >
            <strong>合并</strong>
            <small>保留现有连接，同 ID 更新非敏感字段，重复地址跳过。</small>
          </button>
          <button
            type="button"
            role="radio"
            :aria-checked="strategy === 'replace'"
            class="backup-strategy__option is-danger"
            :class="{ 'is-active': strategy === 'replace' }"
            @click="strategy = 'replace'"
          >
            <strong>替换</strong>
            <small>清空所选范围内的现有配置，再写入备份内容。</small>
          </button>
        </div>

        <p class="backup-note">
          <LgIcon name="lock" :size="14" />
          备份不含凭据；新增连接会标记为“需重新认证”，需在系统中更新凭据后才能连接。
        </p>
      </div>

      <template #footer>
        <div class="backup-modal__actions">
          <LgButton size="small" @click="closePreview">取消</LgButton>
          <LgButton
            size="small"
            :type="strategy === 'replace' ? 'error' : 'primary'"
            :loading="applying"
            @click="applyImport"
          >
            {{ strategy === 'replace' ? '替换并恢复' : '合并并恢复' }}
          </LgButton>
        </div>
      </template>
    </LgModal>
  </div>
</template>

<style scoped>
.backup {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.backup-summary {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-md);
  background: var(--lg-bg-soft);
}

.backup-summary.is-empty {
  border-style: dashed;
}

.backup-summary__icon {
  display: grid;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--lg-info-soft);
  color: var(--lg-info);
}

.backup-summary__body {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.backup-summary__body strong {
  font-size: 12px;
  font-weight: 600;
}

.backup-summary__body span {
  color: var(--lg-text-muted);
  font-size: 11px;
  overflow-wrap: anywhere;
}

.backup-block {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.backup-block h3 {
  margin: 0;
  color: var(--lg-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.backup-scopes {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
}

.backup-scopes :deep(.n-checkbox) {
  align-items: flex-start;
}

.backup-muted {
  margin-left: 6px;
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.backup-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  padding: 9px 0;
  border-top: 1px solid var(--lg-border);
}

.backup-row__text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.backup-row__text strong {
  font-size: 12px;
  font-weight: 500;
}

.backup-row__text span {
  color: var(--lg-text-muted);
  font-size: 11px;
  line-height: 1.45;
}

.backup-row__control {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}

.backup-passwords {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.backup-field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.backup-field > span:first-child {
  color: var(--lg-text-secondary);
  font-size: 11.5px;
}

.backup-warning {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 0;
  padding: 8px 10px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-warning-soft);
  color: var(--lg-text-secondary);
  font-size: 11px;
  line-height: 1.5;
}

.backup-warning svg {
  flex: 0 0 auto;
  color: var(--lg-warning);
}

.backup-error {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0;
  color: var(--lg-danger);
  font-size: 11px;
}

.backup-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding-top: 4px;
}

.backup-hint {
  color: var(--lg-text-muted);
  font-size: 10.5px;
}

.backup-file {
  display: none;
}

.backup-note {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 10.5px;
  line-height: 1.55;
}

.backup-note svg {
  flex: 0 0 auto;
}

.backup-modal {
  width: min(480px, calc(100vw - 28px));
}

.backup-modal--wide {
  width: min(620px, calc(100vw - 28px));
}

.backup-modal__body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.backup-modal__hint {
  margin: 0;
  color: var(--lg-text-muted);
  font-size: 11.5px;
  line-height: 1.55;
}

.backup-modal__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.backup-preview {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.backup-preview__meta {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 7px;
}

.backup-preview__meta > div {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 7px 9px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  min-width: 0;
}

.backup-preview__meta span {
  color: var(--lg-text-muted);
  font-size: 10px;
}

.backup-preview__meta strong {
  overflow: hidden;
  color: var(--lg-text-secondary);
  font-size: 11px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.backup-preview__duplicates {
  padding: 8px 10px;
  border-radius: var(--lg-radius-sm);
  background: var(--lg-warning-soft);
  font-size: 11px;
}

.backup-preview__duplicates > span {
  color: var(--lg-warning);
  font-weight: 600;
}

.backup-preview__duplicates ul {
  margin: 4px 0 0;
  padding-left: 16px;
  color: var(--lg-text-secondary);
  font-family: var(--lg-font-mono);
  font-size: 10.5px;
}

.backup-preview__scopes {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 14px;
  padding-top: 2px;
}

.backup-preview__scopes > span:first-child {
  color: var(--lg-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.backup-strategy {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.backup-strategy__option {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px;
  border: 1px solid var(--lg-border);
  border-radius: var(--lg-radius-sm);
  background: var(--lg-bg-soft);
  color: var(--lg-text-secondary);
  text-align: left;
  cursor: pointer;
}

.backup-strategy__option strong {
  color: var(--lg-text);
  font-size: 12px;
}

.backup-strategy__option small {
  color: var(--lg-text-muted);
  font-size: 10.5px;
  line-height: 1.45;
}

.backup-strategy__option:hover {
  border-color: var(--lg-border-strong);
}

.backup-strategy__option.is-active {
  border-color: color-mix(in srgb, var(--lg-success) 40%, var(--lg-border));
  background: var(--lg-success-soft);
}

.backup-strategy__option.is-danger.is-active {
  border-color: color-mix(in srgb, var(--lg-danger) 44%, var(--lg-border));
  background: var(--lg-danger-soft);
}

@media (max-width: 620px) {
  .backup-scopes,
  .backup-passwords,
  .backup-strategy,
  .backup-preview__meta {
    grid-template-columns: minmax(0, 1fr);
  }

  .backup-row {
    align-items: flex-start;
    flex-direction: column;
    gap: 8px;
  }
}
</style>
