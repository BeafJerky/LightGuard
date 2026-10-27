<script setup lang="ts">
import { computed } from 'vue';
import CanvasSurface from './CanvasSurface.vue';
import { useRenderBackend } from './useRenderBackend';
import { createDesktopEffectsDraw, type EffectsIntensity } from './paint/effects';

const props = withDefaults(
  defineProps<{
    intensity?: EffectsIntensity;
    paused?: boolean;
  }>(),
  { intensity: 'low', paused: false },
);

const backend = useRenderBackend('desktop.effects');
const draw = computed(() => createDesktopEffectsDraw(props.intensity));
</script>

<template>
  <CanvasSurface
    v-if="backend === 'canvas'"
    class="lg-desktop-effects"
    :draw="draw"
    :redraw-key="intensity"
    :paused="paused"
    animated
    aria-hidden="true"
  />
</template>

<style scoped>
.lg-desktop-effects {
  position: absolute;
  inset: 0;
  z-index: 0;
  pointer-events: none;
}
</style>
