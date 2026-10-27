<script setup lang="ts">
import { computed } from 'vue';
import { motion } from 'motion-v';
import { usePreferredReducedMotion } from '@vueuse/core';

type PresetName = 'fade-up' | 'pop' | 'slide-left';

const props = withDefaults(
  defineProps<{
    preset?: PresetName;
    delay?: number;
  }>(),
  {
    preset: 'fade-up',
    delay: 0,
  },
);

const MotionDiv = motion.div;
const reduced = usePreferredReducedMotion();

const presets: Record<PresetName, Record<string, unknown>> = {
  'fade-up': {
    initial: { opacity: 0, y: 12 },
    animate: { opacity: 1, y: 0 },
    exit: { opacity: 0, y: 6 },
    transition: { type: 'spring', stiffness: 420, damping: 34, mass: 0.7 },
  },
  pop: {
    initial: { opacity: 0, scale: 0.96, y: 6 },
    animate: { opacity: 1, scale: 1, y: 0 },
    exit: { opacity: 0, scale: 0.98 },
    transition: { type: 'spring', stiffness: 500, damping: 30, mass: 0.6 },
  },
  'slide-left': {
    initial: { opacity: 0, x: 18 },
    animate: { opacity: 1, x: 0 },
    exit: { opacity: 0, x: -10 },
    transition: { type: 'spring', stiffness: 380, damping: 36, mass: 0.8 },
  },
};

const state = computed(() => {
  if (reduced.value === 'reduce') {
    return {
      initial: false,
      animate: { opacity: 1 },
      exit: { opacity: 0 },
      transition: { duration: 0 },
    };
  }
  const preset = presets[props.preset];
  return {
    ...preset,
    transition: { ...(preset.transition as object), delay: props.delay / 1000 },
  };
});
</script>

<template>
  <MotionDiv
    class="lg-motion"
    :initial="state.initial"
    :animate="state.animate"
    :exit="state.exit"
    :transition="state.transition"
  >
    <slot />
  </MotionDiv>
</template>
