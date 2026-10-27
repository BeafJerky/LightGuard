<script setup lang="ts">
withDefaults(
  defineProps<{
    name: string;
    size?: number | string;
    label?: string;
  }>(),
  { size: 18 },
);
</script>

<template>
  <svg
    class="lg-icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :aria-hidden="label ? undefined : 'true'"
    :role="label ? 'img' : undefined"
    :aria-label="label"
  >
    <use :href="`#lg-i-${name}`" />
  </svg>
</template>
