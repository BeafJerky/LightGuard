<script setup lang="ts">
import { themeSwitching } from './product/theme';
</script>

<template>
  <Transition name="theme-mask">
    <div v-if="themeSwitching" class="theme-mask" role="status" aria-live="polite">
      <span class="theme-mask__spinner" aria-hidden="true" />
      <span>正在切换主题…</span>
    </div>
  </Transition>
</template>

<style scoped>
.theme-mask {
  position: absolute;
  inset: 0;
  z-index: 900;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 10px;
  background: color-mix(in srgb, var(--lg-bg) 82%, transparent);
  color: var(--lg-text-secondary);
  font-size: var(--lg-fs-sm);
  cursor: wait;
}

.theme-mask__spinner {
  width: 18px;
  height: 18px;
  border: 2px solid var(--lg-primary);
  border-top-color: transparent;
  border-radius: 50%;
  animation: theme-mask-spin 0.8s linear infinite;
  opacity: 0.85;
}

@keyframes theme-mask-spin {
  to {
    transform: rotate(360deg);
  }
}

.theme-mask-leave-active {
  transition: opacity 160ms var(--lg-ease);
}

.theme-mask-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .theme-mask__spinner {
    animation-duration: 2s;
  }

  .theme-mask-leave-active {
    transition-duration: 0.01ms;
  }
}
</style>
