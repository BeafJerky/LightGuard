<script setup lang="ts">
import owlDark from '../assets/brand/owl-dark.png';
import owlLight from '../assets/brand/owl-light.png';

withDefaults(
  defineProps<{
    size: number;
    radius?: string;
  }>(),
  { radius: 'var(--lg-radius-sm)' },
);
</script>

<template>
  <span
    class="brand-mark"
    :style="{ width: `${size}px`, height: `${size}px`, borderRadius: radius }"
    aria-hidden="true"
  >
    <img class="brand-mark__img brand-mark__img--light" :src="owlLight" alt="" draggable="false" />
    <img class="brand-mark__img brand-mark__img--dark" :src="owlDark" alt="" draggable="false" />
  </span>
</template>

<style scoped>
.brand-mark {
  position: relative;
  display: block;
  flex: 0 0 auto;
  overflow: hidden;
  user-select: none;
}

.brand-mark__img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: contain;
  pointer-events: none;
  -webkit-user-drag: none;
}

[data-lg-mode='light'] .brand-mark__img--dark,
[data-lg-mode='dark'] .brand-mark__img--light {
  display: none;
}
</style>
