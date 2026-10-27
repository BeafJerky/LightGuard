<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    value: string | number;
    unit?: string;
    hint?: string;
    tone?: 'default' | 'primary' | 'success' | 'warning' | 'danger';
  }>(),
  { tone: 'default' },
);
</script>

<template>
  <div class="lg-kpi" :data-tone="tone">
    <div class="lg-kpi__label">{{ label }}</div>
    <div class="lg-kpi__value">
      <span>{{ value }}</span>
      <small v-if="unit">{{ unit }}</small>
    </div>
    <div v-if="hint || $slots.footer" class="lg-kpi__foot">
      <slot name="footer">{{ hint }}</slot>
    </div>
  </div>
</template>
