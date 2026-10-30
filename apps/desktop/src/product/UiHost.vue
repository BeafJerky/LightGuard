<script setup lang="ts">
import { ref, watch } from 'vue';
import { LgButton, LgCheckbox, LgIcon, LgModal } from '@lightguard/lightui';
import type { CloseAction } from './types';
import {
  closePromptState,
  confirmState,
  dismissToast,
  resolveClosePrompt,
  resolveConfirm,
  toasts,
  type ToastTone,
} from './ui';

const closeRemember = ref(false);

watch(closePromptState, (value) => {
  if (value) closeRemember.value = false;
});

function chooseClose(action: CloseAction) {
  resolveClosePrompt({ action, remember: closeRemember.value });
}

function onCloseShow(value: boolean) {
  if (!value) resolveClosePrompt(null);
}

const icons: Record<ToastTone, string> = {
  success: 'check',
  info: 'info',
  warning: 'alert',
  error: 'alert',
};

function onShow(value: boolean) {
  if (!value) resolveConfirm(false);
}
</script>

<template>
  <div class="ui-toast-stack" role="status" aria-live="polite" aria-atomic="false">
    <TransitionGroup name="ui-toast">
      <div
        v-for="item in toasts"
        :key="item.id"
        class="ui-toast"
        :class="`ui-toast--${item.tone ?? 'info'}`"
      >
        <span class="ui-toast__icon" aria-hidden="true">
          <LgIcon :name="icons[item.tone ?? 'info']" :size="15" />
        </span>
        <div class="ui-toast__body">
          <strong>{{ item.title }}</strong>
          <p v-if="item.message">{{ item.message }}</p>
        </div>
        <button class="ui-toast__close" type="button" aria-label="关闭提示" @click="dismissToast(item.id)">
          <LgIcon name="x" :size="13" />
        </button>
      </div>
    </TransitionGroup>
  </div>

  <LgModal
    :show="Boolean(confirmState)"
    preset="card"
    class="ui-confirm"
    :mask-closable="false"
    :bordered="false"
    :close-on-esc="true"
    :title="confirmState?.title"
    @update:show="onShow"
  >
    <div class="ui-confirm__body">
      <span class="ui-confirm__mark" :class="{ 'is-danger': confirmState?.tone === 'danger' }">
        <LgIcon :name="confirmState?.tone === 'danger' ? 'alert' : 'help'" :size="18" />
      </span>
      <p>{{ confirmState?.message }}</p>
    </div>
    <template #footer>
      <div class="ui-confirm__actions">
        <LgButton size="small" @click="resolveConfirm(false)">
          {{ confirmState?.cancelLabel ?? '取消' }}
        </LgButton>
        <LgButton
          size="small"
          :type="confirmState?.tone === 'danger' ? 'error' : 'primary'"
          @click="resolveConfirm(true)"
        >
          {{ confirmState?.confirmLabel ?? '确定' }}
        </LgButton>
      </div>
    </template>
  </LgModal>

  <LgModal
    :show="Boolean(closePromptState)"
    preset="card"
    class="ui-confirm"
    :mask-closable="false"
    :bordered="false"
    :close-on-esc="true"
    title="关闭 LightGuard"
    @update:show="onCloseShow"
  >
    <div class="ui-confirm__body">
      <span class="ui-confirm__mark">
        <LgIcon name="monitor" :size="18" />
      </span>
      <p>最小化到托盘后 SSH 会话会保持连接；退出应用将断开全部会话。</p>
    </div>
    <LgCheckbox
      class="ui-close__remember"
      :checked="closeRemember"
      @update:checked="(value: boolean) => (closeRemember = value)"
    >
      记住我的选择，下次不再询问
    </LgCheckbox>
    <template #footer>
      <div class="ui-confirm__actions">
        <LgButton size="small" @click="resolveClosePrompt(null)">取消</LgButton>
        <LgButton size="small" @click="chooseClose('tray')">最小化到托盘</LgButton>
        <LgButton size="small" type="error" secondary @click="chooseClose('quit')">退出应用</LgButton>
      </div>
    </template>
  </LgModal>
</template>

<style scoped>
.ui-toast-stack {
  position: fixed;
  right: clamp(14px, 2.4vw, 28px);
  bottom: clamp(14px, 2.4vw, 28px);
  z-index: 4000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: min(360px, calc(100vw - 28px));
  pointer-events: none;
}

.ui-toast {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 10px;
  align-items: start;
  padding: 12px 12px 12px 14px;
  border: 1px solid var(--lg-border-strong);
  border-left: 3px solid var(--lg-info);
  border-radius: var(--lg-radius-md);
  background: var(--lg-surface-glass);
  box-shadow: var(--lg-shadow-lg);
  backdrop-filter: blur(14px);
  pointer-events: auto;
}

.ui-toast--success {
  border-left-color: var(--lg-success);
}

.ui-toast--warning {
  border-left-color: var(--lg-warning);
}

.ui-toast--error {
  border-left-color: var(--lg-danger);
}

.ui-toast__icon {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border-radius: 8px;
  background: var(--lg-info-soft);
  color: var(--lg-info);
}

.ui-toast--success .ui-toast__icon {
  background: var(--lg-success-soft);
  color: var(--lg-success);
}

.ui-toast--warning .ui-toast__icon {
  background: var(--lg-warning-soft);
  color: var(--lg-warning);
}

.ui-toast--error .ui-toast__icon {
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.ui-toast__body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.ui-toast__body strong {
  font-size: var(--lg-fs-body);
  font-weight: 600;
  color: var(--lg-text);
}

.ui-toast__body p {
  margin: 0;
  font-size: var(--lg-fs-sm);
  line-height: 1.5;
  color: var(--lg-text-secondary);
  overflow-wrap: anywhere;
}

.ui-toast__close {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--lg-text-muted);
  cursor: pointer;
}

.ui-toast__close:hover {
  background: var(--lg-hover);
  color: var(--lg-text);
}

.ui-toast-enter-active,
.ui-toast-leave-active {
  transition:
    opacity var(--lg-dur-base) var(--lg-ease),
    transform var(--lg-dur-base) var(--lg-ease);
}

.ui-toast-enter-from,
.ui-toast-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.98);
}

.ui-confirm__body {
  display: flex;
  gap: 12px;
  align-items: flex-start;
}

.ui-confirm__mark {
  display: grid;
  flex: 0 0 auto;
  place-items: center;
  width: 34px;
  height: 34px;
  border-radius: 10px;
  background: var(--lg-info-soft);
  color: var(--lg-info);
}

.ui-confirm__mark.is-danger {
  background: var(--lg-danger-soft);
  color: var(--lg-danger);
}

.ui-confirm__body p {
  margin: 0;
  padding-top: 6px;
  font-size: var(--lg-fs-body);
  line-height: 1.6;
  color: var(--lg-text-secondary);
}

.ui-confirm__actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}

.ui-close__remember {
  margin-top: 14px;
}

@media (prefers-reduced-motion: reduce) {
  .ui-toast-enter-active,
  .ui-toast-leave-active {
    transition-duration: 0.01ms;
  }
}
</style>
