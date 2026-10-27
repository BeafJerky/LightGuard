<script setup lang="ts">
withDefaults(
  defineProps<{
    as?: string;
    tone?: 'solid' | 'glass' | 'plain';
    padding?: 'none' | 'sm' | 'md' | 'lg';
    interactive?: boolean;
  }>(),
  {
    as: 'div',
    tone: 'solid',
    padding: 'md',
    interactive: false,
  },
);
</script>

<template>
  <component
    :is="as"
    class="lg-surface"
    :class="[`lg-surface--${tone}`, `lg-surface--pad-${padding}`, { 'is-interactive': interactive }]"
  >
    <slot />
  </component>
</template>
