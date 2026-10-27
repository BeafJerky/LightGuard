<script setup lang="ts">
withDefaults(
  defineProps<{
    label: string;
    tone?: 'online' | 'warning' | 'danger' | 'muted' | 'info';
  }>(),
  { tone: 'online' },
);
</script>

<template>
  <span class="lg-pill" :data-tone="tone">
    <span class="lg-pill__dot" aria-hidden="true" />
    {{ label }}
  </span>
</template>
