import { ref } from 'vue';

export const isFullscreen = ref(false);

let toggler: (() => void) | null = null;

export function registerFullscreenToggler(fn: (() => void) | null) {
  toggler = fn;
}

export function toggleFullscreen() {
  toggler?.();
}
